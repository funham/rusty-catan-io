use std::{
    fs,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use catan_agents::{
    bot::BotPolicy,
    greedy::GreedyAgent,
    lazy::LazyAgent,
    random::RandomAgent,
    remote_agent::{CliRole, CliToHost, read_frame},
};
use catan_core::gameplay::{
    game::{run::RunOptions, state::SetupGameState},
    primitives::player::PlayerId,
};

use crate::{
    config::{self, FieldConfig, InitialStateConfig, MatchConfig, ObserverConfig, PlayerConfig},
    persistence::PersistenceObserver,
    remote_seat::{RemoteCliOutputObserver, RemoteCliSeat},
    snapshot,
    sync_host::{OutputObserver, Seat, SyncGameHost, bot_seat},
};

pub fn load_config(path: &Path) -> Result<MatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    let mut config: MatchConfig = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))?;
    config::resolve_paths(&mut config, path.parent().unwrap_or_else(|| Path::new(".")));
    Ok(config)
}

pub fn run_match(config: MatchConfig) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("config must contain at least one player".to_owned());
    }

    let exe =
        std::env::current_exe().map_err(|err| format!("failed to find current exe: {err}"))?;
    let seats = build_seats(&config.players, &exe)?;
    let observers = build_observers(&config.observers, &exe)?;
    let options = RunOptions {
        max_turns: config.limits.max_turns,
        max_invalid_actions: config.limits.max_invalid_actions,
        ..RunOptions::default()
    };
    let engine = build_initial_engine(&config, config.players.len(), options)?;

    match &config.dice {
        crate::config::DiceConfig::Random => {}
    }

    let mut host = SyncGameHost::from_engine(engine, seats);
    for observer in observers {
        host.add_observer(observer);
    }
    if let Some(observer) = PersistenceObserver::from_config(&config.persistence)
        .map_err(|err| format!("failed to initialize persistence: {err}"))?
    {
        host.add_observer(Box::new(observer));
    }
    host.start();
    let result = host.run_to_result();
    log::info!("match result: {result:?}");
    Ok(())
}

fn build_initial_engine(
    config: &MatchConfig,
    player_count: usize,
    options: RunOptions,
) -> Result<catan_core::gameplay::game::engine::GameEngine, String> {
    match &config.initial {
        InitialStateConfig::Fresh => {
            let init = build_initial_state(&config.field, player_count)?;
            Ok(catan_core::gameplay::game::engine::GameEngine::from_init(
                init, options,
            ))
        }
        InitialStateConfig::Snapshot { path } => {
            let loaded = snapshot::load_checkpoint(path)
                .map_err(|err| format!("failed to load snapshot {}: {err}", path.display()))?;
            let snapshot = loaded.snapshot;
            let snapshot_players = snapshot.state.table().players.count();
            if snapshot_players != player_count {
                return Err(format!(
                    "snapshot has {snapshot_players} players but config declares {player_count}"
                ));
            }
            Ok(
                catan_core::gameplay::game::engine::GameEngine::from_snapshot(
                    snapshot,
                    loaded.board,
                    options,
                ),
            )
        }
    }
}

fn build_seats(players: &[PlayerConfig], exe: &Path) -> Result<Vec<Box<dyn Seat>>, String> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| {
            let player_id = PlayerId::try_from(id).map_err(|err| err.to_string())?;
            match player {
                PlayerConfig::Lazy => Ok(bot_seat(
                    Box::new(LazyAgent::new(player_id)) as Box<dyn BotPolicy>
                )),
                PlayerConfig::Greedy => Ok(bot_seat(
                    Box::new(GreedyAgent::new(player_id)) as Box<dyn BotPolicy>
                )),
                PlayerConfig::Random => Ok(bot_seat(
                    Box::new(RandomAgent::new(player_id)) as Box<dyn BotPolicy>
                )),
                PlayerConfig::Cli => {
                    let stream = spawn_cli_child(exe, &CliChildSpec::player(player_id))?;
                    let seat = RemoteCliSeat::new(player_id, stream)
                        .map_err(|err| format!("failed to initialize remote CLI player: {err}"))?;
                    Ok(Box::new(seat) as Box<dyn Seat>)
                }
            }
        })
        .collect()
}

fn build_observers(
    observers: &[ObserverConfig],
    exe: &Path,
) -> Result<Vec<Box<dyn OutputObserver>>, String> {
    observers
        .iter()
        .map(|observer| {
            let spec = CliChildSpec::observer(observer);
            let stream = spawn_cli_child(exe, &spec)?;
            let observer = RemoteCliOutputObserver::new(spec.role.clone(), stream)
                .map_err(|err| format!("failed to initialize remote CLI observer: {err}"))?;
            Ok(Box::new(observer) as Box<dyn OutputObserver>)
        })
        .collect()
}

fn build_initial_state(
    config: &FieldConfig,
    player_count: usize,
) -> Result<SetupGameState, String> {
    match config {
        FieldConfig::Default => {
            let mut field = catan_core::gameplay::field::state::FieldBuildParam::default();
            field.n_players = player_count;
            Ok(SetupGameState::new(field))
        }
        FieldConfig::LayoutRef { path } => {
            let arrangement = catan_core::gameplay::field::ser::arrangement_from_json(path)
                .ok_or_else(|| format!("failed to read field layout {}", path.display()))?;
            Ok(SetupGameState::new(
                catan_core::gameplay::field::state::FieldBuildParam {
                    n_players: player_count,
                    arrangement,
                },
            ))
        }
    }
}

#[derive(Debug, Clone)]
struct CliChildSpec {
    role: CliRole,
    label: String,
}

impl CliChildSpec {
    fn player(player_id: PlayerId) -> Self {
        Self {
            role: CliRole::Player { player_id },
            label: format!("player:{player_id}"),
        }
    }

    fn observer(config: &ObserverConfig) -> Self {
        let role = match config {
            ObserverConfig::CliSpectator => CliRole::Spectator,
            ObserverConfig::CliPlayer { player_id } => CliRole::PlayerObserver {
                player_id: PlayerId::try_from(*player_id)
                    .expect("configured player id should fit in u8"),
            },
            ObserverConfig::CliOmniscient => CliRole::Omniscient,
            ObserverConfig::SnapshotObserver => CliRole::SnapshotObserver,
        };
        let label = match role {
            CliRole::PlayerObserver { player_id } => format!("player-observer:{player_id}"),
            _ => role.label().to_owned(),
        };
        Self { role, label }
    }

    fn socket_abbrev(&self) -> &'static str {
        self.role.socket_abbrev()
    }
}

fn spawn_cli_child(exe: &Path, spec: &CliChildSpec) -> Result<UnixStream, String> {
    let socket_path = unique_socket_path(spec, "game");
    let log_socket_path = unique_socket_path(spec, "log");
    for path in [&socket_path, &log_socket_path] {
        if path.exists() {
            fs::remove_file(path).map_err(|err| {
                format!("failed to remove stale socket {}: {err}", path.display())
            })?;
        }
    }
    let listener = UnixListener::bind(&socket_path)
        .map_err(|err| format!("failed to bind socket {}: {err}", socket_path.display()))?;
    let log_listener = UnixListener::bind(&log_socket_path).map_err(|err| {
        format!(
            "failed to bind log socket {}: {err}",
            log_socket_path.display()
        )
    })?;
    spawn_terminal(exe, &socket_path, &log_socket_path, &spec.label)?;
    let (stream, _) = listener
        .accept()
        .map_err(|err| format!("failed to accept CLI child connection: {err}"))?;
    let (log_stream, _) = log_listener
        .accept()
        .map_err(|err| format!("failed to accept CLI child log connection: {err}"))?;
    spawn_child_log_reader(spec.label.clone(), log_stream);
    Ok(stream)
}

fn unique_socket_path(spec: &CliChildSpec, channel: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let channel = match channel {
        "game" => "g",
        "log" => "l",
        other => other,
    };
    std::env::temp_dir().join(format!(
        "rc-{}-{channel}-{}-{now}.sock",
        spec.socket_abbrev(),
        std::process::id()
    ))
}

fn spawn_child_log_reader(role: String, mut stream: UnixStream) {
    std::thread::spawn(move || {
        loop {
            match read_frame::<CliToHost>(&mut stream) {
                Ok(CliToHost::Log {
                    level,
                    target,
                    message,
                }) => {
                    let level = log::Level::from(level);
                    for line in message.lines().filter(|line| !line.trim().is_empty()) {
                        log::log!(target: "catan_cli_child", level, "[{role}][{target}] {line}");
                    }
                }
                Ok(CliToHost::Error { message }) => {
                    log::error!(target: "catan_cli_child", "[{role}] remote CLI error: {message}");
                }
                Ok(other) => {
                    log::warn!(target: "catan_cli_child", "[{role}] unexpected log frame: {other:?}");
                }
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return,
                Err(err) => {
                    log::warn!(target: "catan_cli_child", "[{role}] log reader stopped: {err}");
                    return;
                }
            }
        }
    });
}

fn spawn_terminal(
    exe: &Path,
    socket_path: &Path,
    log_socket_path: &Path,
    role_arg: &str,
) -> Result<(), String> {
    const TERMINAL_ROWS: u16 = 44;
    const TERMINAL_COLS: u16 = 132;

    let exe = exe
        .to_str()
        .ok_or_else(|| format!("non-utf8 executable path: {}", exe.display()))?;
    let socket = socket_path
        .to_str()
        .ok_or_else(|| format!("non-utf8 socket path: {}", socket_path.display()))?;
    let log_socket = log_socket_path
        .to_str()
        .ok_or_else(|| format!("non-utf8 log socket path: {}", log_socket_path.display()))?;
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned());

    if cfg!(target_os = "macos") {
        let command = format!(
            "printf '\\033[8;{};{}t'; cd {} && RUST_LOG={} {} cli-child --socket {} --log-socket {} --role {}; echo; echo '[catan cli child exited - press enter to close]'; read _",
            TERMINAL_ROWS,
            TERMINAL_COLS,
            shell_quote(
                std::env::current_dir()
                    .map_err(|err| format!("failed to read current dir: {err}"))?
                    .to_string_lossy()
                    .as_ref()
            ),
            shell_quote(&rust_log),
            shell_quote(exe),
            shell_quote(socket),
            shell_quote(log_socket),
            shell_quote(role_arg),
        );
        let script = format!(
            "tell application \"Terminal\" to do script {}",
            apple_quote(&command)
        );
        Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|err| format!("failed to spawn Terminal.app: {err}"))?;
        return Ok(());
    }

    if cfg!(target_os = "linux") {
        let command = format!(
            "printf '\\033[8;{};{}t'; RUST_LOG={} {} cli-child --socket {} --log-socket {} --role {}; echo; echo '[catan cli child exited - press enter to close]'; read _",
            TERMINAL_ROWS,
            TERMINAL_COLS,
            shell_quote(&rust_log),
            shell_quote(exe),
            shell_quote(socket),
            shell_quote(log_socket),
            shell_quote(role_arg),
        );
        for terminal in [
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "alacritty",
            "xterm",
        ] {
            if command_exists(terminal) {
                let mut cmd = Command::new(terminal);
                match terminal {
                    "gnome-terminal" => {
                        cmd.args(["--", "sh", "-lc", &command]);
                    }
                    "konsole" => {
                        cmd.args(["-e", "sh", "-lc", &command]);
                    }
                    "xfce4-terminal" => {
                        cmd.args(["--command", &format!("sh -lc {}", shell_quote(&command))]);
                    }
                    "alacritty" | "xterm" | "x-terminal-emulator" => {
                        cmd.args(["-e", "sh", "-lc", &command]);
                    }
                    _ => unreachable!(),
                }
                cmd.spawn()
                    .map_err(|err| format!("failed to spawn {terminal}: {err}"))?;
                return Ok(());
            }
        }
        return Err("no supported Linux terminal emulator found".to_owned());
    }

    Err("CLI terminal spawning is supported only on macOS and Linux".to_owned())
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_quote(name)))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn apple_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use catan_agents::remote_agent::CliRole;
    use catan_core::gameplay::game::{engine::GameEngine, run::RunOptions, state::SetupGameState};

    use crate::{
        config::{
            DiceConfig, FieldConfig, InitialStateConfig, LimitsConfig, LoggingConfig, MatchConfig,
            ObserverConfig, PersistenceConfig, PlayerConfig,
        },
        host::{CliChildSpec, build_initial_engine, build_initial_state, unique_socket_path},
        snapshot::SnapshotStore,
    };

    #[test]
    fn snapshot_observer_config_maps_to_snapshot_role() {
        let spec = CliChildSpec::observer(&ObserverConfig::SnapshotObserver);

        assert!(matches!(spec.role, CliRole::SnapshotObserver));
        assert_eq!(spec.label, "snapshot-observer");
        assert_eq!(spec.socket_abbrev(), "snap");
    }

    #[test]
    fn cli_child_socket_names_use_short_role_and_channel_tokens() {
        let spec = CliChildSpec::observer(&ObserverConfig::SnapshotObserver);

        let game = unique_socket_path(&spec, "game");
        let log = unique_socket_path(&spec, "log");
        let game_name = game.file_name().unwrap().to_string_lossy();
        let log_name = log.file_name().unwrap().to_string_lossy();

        assert!(game_name.starts_with("rc-snap-g-"));
        assert!(log_name.starts_with("rc-snap-l-"));
        assert!(game_name.len() < 104);
        assert!(log_name.len() < 104);
    }

    #[test]
    fn default_field_uses_configured_player_count() {
        for player_count in [1, 2, 3, 4, 6] {
            let init = build_initial_state(&FieldConfig::Default, player_count).unwrap();

            assert_eq!(init.board.n_players, player_count);
            assert_eq!(init.players.count(), player_count);
            assert_eq!(init.builds.players().len(), player_count);
        }
    }

    #[test]
    fn snapshot_initial_engine_preserves_pending_decisions() {
        let dir = unique_test_dir();
        let mut engine = GameEngine::from_init(SetupGameState::default(), RunOptions::default());
        engine.start().expect("engine should start");
        let mut store = SnapshotStore::new_in(dir.clone()).unwrap();
        let snapshot_dir = store.write_checkpoint(&engine).unwrap();
        let config = MatchConfig {
            players: vec![
                PlayerConfig::Lazy,
                PlayerConfig::Lazy,
                PlayerConfig::Lazy,
                PlayerConfig::Lazy,
            ],
            observers: Vec::new(),
            initial: InitialStateConfig::Snapshot { path: snapshot_dir },
            field: FieldConfig::Default,
            dice: DiceConfig::default(),
            limits: LimitsConfig::default(),
            logging: LoggingConfig::default(),
            persistence: PersistenceConfig::default(),
        };

        let loaded = build_initial_engine(&config, config.players.len(), RunOptions::default())
            .expect("snapshot should load into an engine");

        assert!(loaded.is_started());
        assert_eq!(loaded.pending_decisions().count(), 1);

        std::fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rusty-catan-host-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
