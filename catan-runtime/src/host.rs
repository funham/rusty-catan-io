use std::{
    fs,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use catan_agents::{
    greedy::GreedyAgent,
    lazy::LazyAgent,
    random::RandomAgent,
    remote_agent::{RemoteCliAgent, RemoteCliObserver},
};
use catan_core::{
    agent::Agent,
    gameplay::game::{
        controller::{GameController, RunOptions},
        event::{GameObserver, ObserverKind},
        init::GameInitializationState,
    },
    math::dice::{DiceRoller, RandomDiceRoller},
};

use crate::config::{DiceConfig, FieldConfig, MatchConfig, ObserverConfig, PlayerConfig};

pub fn load_config(path: &Path) -> Result<MatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))
}

pub fn run_match(config: MatchConfig) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("config must contain at least one player".to_owned());
    }

    let exe =
        std::env::current_exe().map_err(|err| format!("failed to find current exe: {err}"))?;
    let agents = build_agents(&config.players, &exe)?;
    let observers = build_observers(&config.observers, &exe)?;
    let mut dice = build_dice(&config.dice);
    let init_state = build_initial_state(&config.field);
    let mut agents = agents;
    let state = GameController::init(init_state, &mut agents);
    let mut controller = GameController::new(state, agents);
    for observer in observers {
        controller.add_observer(observer);
    }

    let result = controller.run_with_options(
        dice.as_mut(),
        RunOptions {
            max_turns: config.limits.max_turns,
            max_invalid_actions: config.limits.max_invalid_actions,
        },
    );
    log::info!("match result: {result:?}");
    Ok(())
}

fn build_agents(players: &[PlayerConfig], exe: &Path) -> Result<Vec<Box<dyn Agent>>, String> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| match player {
            PlayerConfig::Lazy => Ok(Box::new(LazyAgent::new(id)) as Box<dyn Agent>),
            PlayerConfig::Greedy => Ok(Box::new(GreedyAgent::new(id)) as Box<dyn Agent>),
            PlayerConfig::Cli => {
                let stream = spawn_cli_child(exe, "player")?;
                let agent = RemoteCliAgent::new(id, stream)
                    .map_err(|err| format!("failed to initialize remote CLI player: {err}"))?;
                Ok(Box::new(agent) as Box<dyn Agent>)
            }
            PlayerConfig::Random => Ok(Box::new(RandomAgent::new(id)) as Box<dyn Agent>),
        })
        .collect()
}

fn build_observers(
    observers: &[ObserverConfig],
    exe: &Path,
) -> Result<Vec<Box<dyn GameObserver>>, String> {
    observers
        .iter()
        .map(|observer| {
            let (kind, role_arg) = match observer {
                ObserverConfig::CliSpectator => (ObserverKind::Spectator, "spectator".to_owned()),
                ObserverConfig::CliPlayer { player_id } => (
                    ObserverKind::Player(*player_id),
                    format!("player-observer:{player_id}"),
                ),
                ObserverConfig::CliOmniscient => {
                    (ObserverKind::Omniscient, "omniscient".to_owned())
                }
            };
            let stream = spawn_cli_child(exe, &role_arg)?;
            let observer = RemoteCliObserver::new(kind, stream)
                .map_err(|err| format!("failed to initialize remote CLI observer: {err}"))?;
            Ok(Box::new(observer) as Box<dyn GameObserver>)
        })
        .collect()
}

fn build_dice(config: &DiceConfig) -> Box<dyn DiceRoller> {
    match config {
        DiceConfig::Random => Box::new(RandomDiceRoller::new()),
    }
}

fn build_initial_state(config: &FieldConfig) -> GameInitializationState {
    match config {
        FieldConfig::Default => GameInitializationState::default(),
    }
}

fn spawn_cli_child(exe: &Path, role_arg: &str) -> Result<UnixStream, String> {
    let socket_path = unique_socket_path(role_arg);
    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|err| {
            format!(
                "failed to remove stale socket {}: {err}",
                socket_path.display()
            )
        })?;
    }
    let listener = UnixListener::bind(&socket_path)
        .map_err(|err| format!("failed to bind socket {}: {err}", socket_path.display()))?;
    spawn_terminal(exe, &socket_path, role_arg)?;
    let (stream, _) = listener
        .accept()
        .map_err(|err| format!("failed to accept CLI child connection: {err}"))?;
    Ok(stream)
}

fn unique_socket_path(role_arg: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let role = role_arg.replace(':', "-");
    std::env::temp_dir().join(format!(
        "rusty-catan-{role}-{}-{now}.sock",
        std::process::id()
    ))
}

fn spawn_terminal(exe: &Path, socket_path: &Path, role_arg: &str) -> Result<(), String> {
    let exe = exe
        .to_str()
        .ok_or_else(|| format!("non-utf8 executable path: {}", exe.display()))?;
    let socket = socket_path
        .to_str()
        .ok_or_else(|| format!("non-utf8 socket path: {}", socket_path.display()))?;
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned());

    if cfg!(target_os = "macos") {
        let command = format!(
            "cd {} && RUST_LOG={} {} cli-child --socket {} --role {}; echo; echo '[catan cli child exited - press enter to close]'; read _",
            shell_quote(
                std::env::current_dir()
                    .map_err(|err| format!("failed to read current dir: {err}"))?
                    .to_string_lossy()
                    .as_ref()
            ),
            shell_quote(&rust_log),
            shell_quote(exe),
            shell_quote(socket),
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
            "RUST_LOG={} {} cli-child --socket {} --role {}; echo; echo '[catan cli child exited - press enter to close]'; read _",
            shell_quote(&rust_log),
            shell_quote(exe),
            shell_quote(socket),
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
