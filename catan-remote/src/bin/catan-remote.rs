use std::{
    fs,
    os::unix::net::UnixListener,
    path::{Path, PathBuf},
};

use catan_core::gameplay::{game::run::RunOptions, primitives::player::PlayerId};
use catan_remote::{
    protocol::RemoteRole,
    runtime_adapter::{RemoteCliOutputObserver, RemoteCliSeat},
};
use catan_runtime::{
    config::{DiceConfig, MatchConfig, ObserverConfig, SeatConfig},
    host,
    persistence::PersistenceObserver,
    sync_host::{OutputObserver, Seat, SyncGameHost},
};

fn main() {
    env_logger::init();
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("host") => run_host(&args[2..]),
        Some("tui") => run_tui(&args[2..]),
        _ => Err(
            "usage: catan-remote host --config <path> --listen unix://<path>\n       catan-remote tui --connect unix://<path> --role <role>"
                .to_owned(),
        ),
    }
}

fn run_tui(args: &[String]) -> Result<(), String> {
    let connect =
        arg_value(args, "--connect").ok_or_else(|| "missing --connect unix://<path>".to_owned())?;
    let role = arg_value(args, "--role").unwrap_or_else(|| "unknown".to_owned());
    let path = unix_path(&connect)?;
    catan_remote::tui_adapter::run(&path, &role)
}

fn run_host(args: &[String]) -> Result<(), String> {
    let config_path = arg_value(args, "--config")
        .map(PathBuf::from)
        .ok_or_else(|| "missing --config <path>".to_owned())?;
    let listen =
        arg_value(args, "--listen").ok_or_else(|| "missing --listen unix://<path>".to_owned())?;
    let socket_path = unix_path(&listen)?;
    let config = host::load_config(&config_path)?;
    run_unix_host(config, &socket_path)
}

fn run_unix_host(config: MatchConfig, socket_path: &Path) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("config must contain at least one player".to_owned());
    }
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    if socket_path.exists() {
        fs::remove_file(socket_path).map_err(|err| {
            format!(
                "failed to remove stale socket {}: {err}",
                socket_path.display()
            )
        })?;
    }
    let listener = UnixListener::bind(socket_path)
        .map_err(|err| format!("failed to bind {}: {err}", socket_path.display()))?;
    log::info!("listening on unix://{}", socket_path.display());

    let seats = build_remote_host_seats(&config, &listener)?;
    let options = RunOptions {
        max_turns: config.limits.max_turns,
        max_invalid_actions: config.limits.max_invalid_actions,
        ..RunOptions::default()
    };
    let engine = host::build_initial_engine(&config, config.players.len(), options)?;
    match &config.dice {
        DiceConfig::Random => {}
    }

    let mut game_host = SyncGameHost::from_engine(engine, seats);
    for observer in build_remote_observers(&config, &listener)? {
        game_host.add_observer(observer);
    }
    if let Some(observer) = PersistenceObserver::from_config(&config.persistence)
        .map_err(|err| format!("failed to initialize persistence: {err}"))?
    {
        game_host.add_observer(Box::new(observer));
    }
    game_host.start();
    let result = game_host.run_to_result();
    log::info!("match result: {result:?}");
    Ok(())
}

fn build_remote_host_seats(
    config: &MatchConfig,
    listener: &UnixListener,
) -> Result<Vec<Box<dyn Seat>>, String> {
    config
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let player_id = PlayerId::try_from(index).map_err(|err| err.to_string())?;
            if let Some(seat) = host::build_bot_seat(player, player_id)? {
                return Ok(seat);
            }
            let SeatConfig::Remote = player else {
                unreachable!("build_bot_seat covers all non-remote players");
            };
            let (stream, _) = listener
                .accept()
                .map_err(|err| format!("failed to accept remote player {player_id}: {err}"))?;
            RemoteCliSeat::new(player_id, stream)
                .map(|seat| Box::new(seat) as Box<dyn Seat>)
                .map_err(|err| format!("failed to initialize remote player {player_id}: {err}"))
        })
        .collect()
}

fn build_remote_observers(
    config: &MatchConfig,
    listener: &UnixListener,
) -> Result<Vec<Box<dyn OutputObserver>>, String> {
    config
        .observers
        .iter()
        .map(|observer| {
            let role = observer_role(observer)?;
            let (stream, _) = listener
                .accept()
                .map_err(|err| format!("failed to accept remote observer: {err}"))?;
            RemoteCliOutputObserver::new(role, stream)
                .map(|observer| Box::new(observer) as Box<dyn OutputObserver>)
                .map_err(|err| format!("failed to initialize remote observer: {err}"))
        })
        .collect()
}

fn observer_role(config: &ObserverConfig) -> Result<RemoteRole, String> {
    Ok(match config {
        ObserverConfig::CliSpectator => RemoteRole::Spectator,
        ObserverConfig::CliPlayer { player_id } => RemoteRole::PlayerObserver {
            player_id: PlayerId::try_from(*player_id).map_err(|err| err.to_string())?,
        },
        ObserverConfig::CliOmniscient => RemoteRole::Omniscient,
        ObserverConfig::SnapshotObserver => RemoteRole::SnapshotObserver,
    })
}

fn unix_path(value: &str) -> Result<PathBuf, String> {
    value
        .strip_prefix("unix://")
        .map(PathBuf::from)
        .ok_or_else(|| format!("only unix:// endpoints are supported now, got {value}"))
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == name).then(|| window[1].clone()))
}
