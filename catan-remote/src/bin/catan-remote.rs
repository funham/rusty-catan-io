use std::{
    cell::RefCell,
    fs,
    os::unix::net::UnixListener,
    path::{Path, PathBuf},
    rc::Rc,
};

use catan_core::gameplay::{game::run::RunOptions, primitives::player::PlayerId};
use catan_remote::{
    config::{self, RemoteMatchConfig, RemoteObserverConfig, SeatConfig},
    protocol::RemoteRole,
    runtime_adapter::{RemoteCliOutputObserver, RemoteCliSeat},
};
use catan_runtime::{
    config::DiceConfig,
    host,
    persistence::PersistenceObserver,
    run_stats::RunStatsObserver,
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
    let config = config::load_config(&config_path)?;
    run_unix_host(config, &socket_path)
}

fn run_unix_host(config: RemoteMatchConfig, socket_path: &Path) -> Result<(), String> {
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

    let remote_seats = build_remote_host_seats(&config, &listener)?;
    let options = RunOptions {
        max_turns: config.limits.max_turns,
        max_invalid_actions: config.limits.max_invalid_actions,
        ..RunOptions::default()
    };
    let engine = host::build_initial_engine_from_parts(
        &config.initial,
        &config.field,
        config.players.len(),
        options,
    )?;
    match &config.dice {
        DiceConfig::Random => {}
    }

    let mut game_host = SyncGameHost::from_engine(engine, remote_seats.host_seats);
    let (stats_observer, stats_handle) = RunStatsObserver::new();
    game_host.add_observer(Box::new(stats_observer));
    let remote_observers = build_remote_observers(&config, &listener)?;
    for observer in remote_observers.host_observers {
        game_host.add_observer(observer);
    }
    if let Some(observer) = PersistenceObserver::from_config(&config.persistence)
        .map_err(|err| format!("failed to initialize persistence: {err}"))?
    {
        game_host.add_observer(Box::new(observer));
    }
    game_host.start();
    let result = game_host.run_to_result();
    let mut summary = stats_handle.summary();
    if summary.result.is_none() {
        summary.result = Some(result.clone());
    }
    for seat in remote_seats.summary_targets {
        seat.borrow_mut().send_summary(&summary);
    }
    for observer in remote_observers.summary_targets {
        observer.borrow_mut().send_summary(&summary);
    }
    log::info!("match result: {result:?}");
    Ok(())
}

fn build_remote_host_seats(
    config: &RemoteMatchConfig,
    listener: &UnixListener,
) -> Result<BuiltRemoteSeats, String> {
    let mut host_seats: Vec<Box<dyn Seat>> = Vec::new();
    let mut summary_targets = Vec::new();
    for (index, player) in config.players.iter().enumerate() {
        let player_id = PlayerId::try_from(index).map_err(|err| err.to_string())?;
        match player {
            SeatConfig::Bot(bot) => host_seats.push(host::build_bot_seat(bot, player_id)),
            SeatConfig::Remote => {
                let (stream, _) = listener
                    .accept()
                    .map_err(|err| format!("failed to accept remote player {player_id}: {err}"))?;
                let seat = RemoteCliSeat::new(player_id, stream).map_err(|err| {
                    format!("failed to initialize remote player {player_id}: {err}")
                })?;
                let seat = Rc::new(RefCell::new(seat));
                host_seats.push(Box::new(RemoteSeatSlot { seat: seat.clone() }));
                summary_targets.push(seat);
            }
        }
    }
    Ok(BuiltRemoteSeats {
        host_seats,
        summary_targets,
    })
}

struct RemoteSeatSlot {
    seat: Rc<RefCell<RemoteCliSeat>>,
}

impl Seat for RemoteSeatSlot {
    fn player_id(&self) -> PlayerId {
        self.seat.borrow().player_id()
    }

    fn on_frame(
        &mut self,
        frame: catan_runtime::sync_host::SeatFrame<'_>,
        commands: &mut catan_runtime::sync_host::SeatCommandBuffer,
    ) {
        self.seat.borrow_mut().on_frame(frame, commands);
    }
}

struct BuiltRemoteSeats {
    host_seats: Vec<Box<dyn Seat>>,
    summary_targets: Vec<Rc<RefCell<RemoteCliSeat>>>,
}

struct RemoteObserverSlot {
    observer: Rc<RefCell<RemoteCliOutputObserver>>,
}

impl OutputObserver for RemoteObserverSlot {
    fn on_output(&mut self, frame: catan_runtime::sync_host::ObserverFrame<'_>) {
        self.observer.borrow_mut().on_output(frame);
    }
}

struct BuiltRemoteObservers {
    host_observers: Vec<Box<dyn OutputObserver>>,
    summary_targets: Vec<Rc<RefCell<RemoteCliOutputObserver>>>,
}

fn build_remote_observers(
    config: &RemoteMatchConfig,
    listener: &UnixListener,
) -> Result<BuiltRemoteObservers, String> {
    let mut host_observers: Vec<Box<dyn OutputObserver>> = Vec::new();
    let mut summary_targets = Vec::new();
    for observer in &config.observers {
        let role = observer_role(observer)?;
        let (stream, _) = listener
            .accept()
            .map_err(|err| format!("failed to accept remote observer: {err}"))?;
        let observer = RemoteCliOutputObserver::new(role, stream)
            .map_err(|err| format!("failed to initialize remote observer: {err}"))?;
        let observer = Rc::new(RefCell::new(observer));
        host_observers.push(Box::new(RemoteObserverSlot {
            observer: observer.clone(),
        }));
        summary_targets.push(observer);
    }
    Ok(BuiltRemoteObservers {
        host_observers,
        summary_targets,
    })
}

fn observer_role(config: &RemoteObserverConfig) -> Result<RemoteRole, String> {
    Ok(match config {
        RemoteObserverConfig::TuiSpectator => RemoteRole::Spectator,
        RemoteObserverConfig::TuiPlayer { player_id } => RemoteRole::PlayerObserver {
            player_id: PlayerId::try_from(*player_id).map_err(|err| err.to_string())?,
        },
        RemoteObserverConfig::TuiOmniscient => RemoteRole::Omniscient,
        RemoteObserverConfig::TuiSnapshot => RemoteRole::SnapshotObserver,
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
