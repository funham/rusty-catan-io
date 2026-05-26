# Runtime Remote Config and Developer Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the runtime/remote boundary split by making `catan-runtime` bot-only for seats, moving remote seat/TUI observer config into `catan-remote`, fixing post-game summaries for remote player TUIs, and adding simple scripts plus docs for common launch flows.

**Architecture:** `catan-runtime` owns pure bot match configuration, reusable match setup, local observers, persistence, simulation, and benchmark execution. `catan-remote` owns mixed bot/remote seat configuration, TUI observer configuration, Unix socket orchestration, and remote launcher ergonomics. Shared gameplay setup stays callable from runtime without making runtime depend on remote.

**Tech Stack:** Rust 2024, serde/serde_json, bash, Cargo workspace binaries, existing `catan-runtime` host APIs, existing `catan-remote` Unix transport and TUI protocol.

---

## File Structure

- Modify `catan-runtime/src/config.rs`: remove `SeatConfig::Remote` and TUI observer config; make runtime player config bot-only.
- Modify `catan-runtime/src/host.rs`: accept `BotConfig` seats only; expose shared initial-engine helpers that do not require runtime `MatchConfig`.
- Modify `catan-runtime/src/bin/catan-bench.rs`: use `Vec<BotConfig>` and remove impossible remote validation.
- Modify `catan-runtime/src/main.rs`: keep bot-only default config.
- Create `catan-remote/src/config.rs`: own `RemoteMatchConfig`, `SeatConfig`, and `RemoteObserverConfig`.
- Modify `catan-remote/src/lib.rs`: export the new config module.
- Modify `catan-remote/src/bin/catan-remote.rs`: load remote config locally, build mixed seats, retain player summary targets, and send `GameSummary` to remote players and observers.
- Modify `catan-remote/src/runtime_adapter.rs`: add summary sending for remote player seats.
- Move remote configs from `catan-runtime/data/configurations/` to `catan-remote/data/configurations/`.
- Create `scripts/run-bots.sh`.
- Create `scripts/run-cli-single.sh`.
- Modify `README.md`.
- Replace stale `catan-runtime/readme.md` content with bot-runtime documentation.
- Create `catan-remote/readme.md`.
- Create `docs/configuration.md`.

## Task 1: Fix Post-Game Summary Delivery to Remote Players

**Files:**
- Modify: `catan-remote/src/runtime_adapter.rs`
- Modify: `catan-remote/src/bin/catan-remote.rs`
- Test: `catan-remote/src/runtime_adapter.rs`

- [ ] **Step 1: Add summary sending to remote player seats**

In `catan-remote/src/runtime_adapter.rs`, add this method to `impl RemoteCliSeat`:

```rust
pub fn send_summary(&mut self, summary: &catan_runtime::run_stats::GameSummary) {
    let _ = write_frame(
        &mut self.stream,
        &HostMessage::GameSummary {
            summary: summary.clone(),
        },
    );
}
```

- [ ] **Step 2: Keep remote seats addressable in the host binary**

In `catan-remote/src/bin/catan-remote.rs`, add a seat wrapper:

```rust
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
```

Change `build_remote_host_seats` to return `BuiltRemoteSeats`. For bot seats, push only to `host_seats`. For remote seats:

```rust
let seat = RemoteCliSeat::new(player_id, stream)
    .map_err(|err| format!("failed to initialize remote player {player_id}: {err}"))?;
let seat = Rc::new(RefCell::new(seat));
host_seats.push(Box::new(RemoteSeatSlot { seat: seat.clone() }));
summary_targets.push(seat);
```

- [ ] **Step 3: Send summaries to seats and observers**

In `run_unix_host`, replace the old `seats` binding with:

```rust
let remote_seats = build_remote_host_seats(&config, &listener)?;
let mut game_host = SyncGameHost::from_engine(engine, remote_seats.host_seats);
```

After `run_to_result`, send to both target lists:

```rust
for seat in remote_seats.summary_targets {
    seat.borrow_mut().send_summary(&summary);
}
for observer in remote_observers.summary_targets {
    observer.borrow_mut().send_summary(&summary);
}
```

- [ ] **Step 4: Add a regression test**

In `catan-remote/src/runtime_adapter.rs`, add a test that creates a connected `RemoteCliSeat`, calls `send_summary`, and asserts the client side reads `HostMessage::GameSummary`.

Use the same UnixStream pair pattern already used by `remote_cli_seat_queues_submit_command_from_child`.

```rust
#[test]
fn remote_cli_seat_sends_game_summary() {
    let (host_stream, mut client_stream) = std::os::unix::net::UnixStream::pair().unwrap();
    let client = std::thread::spawn(move || {
        let hello = read_frame::<HostMessage>(&mut client_stream).unwrap();
        assert!(matches!(hello, HostMessage::Hello { .. }));
        write_frame(&mut client_stream, &ClientMessage::Ready).unwrap();
        read_frame::<HostMessage>(&mut client_stream).unwrap()
    });

    let mut seat = RemoteCliSeat::new(PlayerId::new(0), host_stream).unwrap();
    seat.send_summary(&catan_runtime::run_stats::GameSummary::default());

    assert!(matches!(
        client.join().unwrap(),
        HostMessage::GameSummary { .. }
    ));
}
```

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test -p catan-remote runtime_adapter
```

Expected: all runtime adapter tests pass, including the new summary regression.

- [ ] **Step 6: Commit**

```bash
git add catan-remote/src/runtime_adapter.rs catan-remote/src/bin/catan-remote.rs
git commit -m "fix: send summaries to remote players"
```

## Task 2: Make Runtime Config Bot-Only

**Files:**
- Modify: `catan-runtime/src/config.rs`
- Modify: `catan-runtime/src/host.rs`
- Modify: `catan-runtime/src/bin/catan-bench.rs`
- Modify: `catan-runtime/src/main.rs`

- [ ] **Step 1: Remove runtime `SeatConfig`**

In `catan-runtime/src/config.rs`, change:

```rust
pub struct MatchConfig {
    pub players: Vec<SeatConfig>,
```

to:

```rust
pub struct MatchConfig {
    pub players: Vec<BotConfig>,
```

Delete `SeatConfig`, `TaggedConfig`, and the manual `Deserialize for SeatConfig`.

Keep `BotConfig` as:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BotConfig {
    Lazy,
    Greedy,
    Random,
}
```

- [ ] **Step 2: Replace remote/TUI observer config with runtime-local observer config**

Runtime may observe, but it should not know TUI/remote observers. Replace `ObserverConfig` with:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObserverConfig {
    RunSummary,
}
```

Do not wire `RunSummary` into visible output yet unless needed by a config in this task. It is a local runtime observer marker for future non-remote reporting and keeps the config boundary honest.

- [ ] **Step 3: Update runtime config tests**

Remove `parses_snapshot_observer_config`.

Add:

```rust
#[test]
fn runtime_rejects_remote_player_config() {
    let err = serde_json::from_str::<MatchConfig>(
        r#"{ "players": [{ "kind": "remote" }] }"#,
    )
    .unwrap_err();

    assert!(err.to_string().contains("unknown variant"));
}
```

Add:

```rust
#[test]
fn runtime_parses_bot_players_directly() {
    let config: MatchConfig = serde_json::from_str(
        r#"{
          "players": [{ "kind": "lazy" }, { "kind": "greedy" }, { "kind": "random" }]
        }"#,
    )
    .unwrap();

    assert_eq!(config.players.len(), 3);
}
```

- [ ] **Step 4: Update host seat building**

In `catan-runtime/src/host.rs`, change imports from `SeatConfig` to `BotConfig`.

Change:

```rust
pub fn build_seats(players: &[SeatConfig]) -> Result<Vec<Box<dyn Seat>>, String>
```

to:

```rust
pub fn build_seats(players: &[BotConfig]) -> Result<Vec<Box<dyn Seat>>, String>
```

Use:

```rust
match player {
    BotConfig::Lazy => Ok(bot_seat(Box::new(LazyAgent::new(player_id)) as Box<dyn BotPolicy>)),
    BotConfig::Greedy => Ok(bot_seat(Box::new(GreedyAgent::new(player_id)) as Box<dyn BotPolicy>)),
    BotConfig::Random => Ok(bot_seat(Box::new(RandomAgent::new(player_id)) as Box<dyn BotPolicy>)),
}
```

Change `build_bot_seat` to:

```rust
pub fn build_bot_seat(player: &BotConfig, player_id: PlayerId) -> Box<dyn Seat> {
    match player {
        BotConfig::Lazy => bot_seat(Box::new(LazyAgent::new(player_id)) as Box<dyn BotPolicy>),
        BotConfig::Greedy => bot_seat(Box::new(GreedyAgent::new(player_id)) as Box<dyn BotPolicy>),
        BotConfig::Random => bot_seat(Box::new(RandomAgent::new(player_id)) as Box<dyn BotPolicy>),
    }
}
```

- [ ] **Step 5: Split initial engine helper from runtime config**

Keep this runtime helper:

```rust
pub fn build_initial_engine(
    config: &MatchConfig,
    player_count: usize,
    options: RunOptions,
) -> Result<GameEngine, String> {
    build_initial_engine_from_parts(&config.initial, &config.field, player_count, options)
}
```

Add:

```rust
pub fn build_initial_engine_from_parts(
    initial: &InitialStateConfig,
    field: &FieldConfig,
    player_count: usize,
    options: RunOptions,
) -> Result<catan_core::gameplay::game::engine::GameEngine, String> {
    match initial {
        InitialStateConfig::Fresh => {
            let init = build_initial_state(field, player_count)?;
            Ok(catan_core::gameplay::game::engine::GameEngine::from_init(init, options))
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
            Ok(catan_core::gameplay::game::engine::GameEngine::from_snapshot(
                snapshot,
                loaded.board,
                options,
            ))
        }
    }
}
```

- [ ] **Step 6: Update benchmark**

In `catan-runtime/src/bin/catan-bench.rs`, use `BotConfig` only:

```rust
use catan_runtime::config::{self, BotConfig, FieldConfig, MatchConfig};
```

Remove the remote validation block:

```rust
if config.players.iter().any(|player| matches!(player, SeatConfig::Remote)) { ... }
```

Change `build_agents(players: &[SeatConfig], seed: u64)` to `build_agents(players: &[BotConfig], seed: u64)` and match on `BotConfig`.

- [ ] **Step 7: Run runtime tests**

Run:

```bash
cargo test -p catan-runtime
cargo test -p catan-runtime --bin catan-bench
```

Expected: runtime tests and benchmark tests pass.

- [ ] **Step 8: Commit**

```bash
git add catan-runtime/src/config.rs catan-runtime/src/host.rs catan-runtime/src/bin/catan-bench.rs catan-runtime/src/main.rs
git commit -m "refactor: make runtime config bot only"
```

## Task 3: Add Remote-Owned Mixed Match Config

**Files:**
- Create: `catan-remote/src/config.rs`
- Modify: `catan-remote/src/lib.rs`
- Modify: `catan-remote/src/bin/catan-remote.rs`

- [ ] **Step 1: Create remote config module**

Create `catan-remote/src/config.rs`:

```rust
use std::{fs, path::{Path, PathBuf}};

use catan_runtime::config::{
    BotConfig, DiceConfig, FieldConfig, InitialStateConfig, LimitsConfig, LoggingConfig,
    PersistenceConfig,
};
use serde::{Deserialize, Deserializer, de};

#[derive(Debug, Deserialize)]
pub struct RemoteMatchConfig {
    pub players: Vec<SeatConfig>,
    #[serde(default)]
    pub observers: Vec<RemoteObserverConfig>,
    #[serde(default)]
    pub initial: InitialStateConfig,
    #[serde(default)]
    pub field: FieldConfig,
    #[serde(default)]
    pub dice: DiceConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub persistence: PersistenceConfig,
}

#[derive(Debug, Clone)]
pub enum SeatConfig {
    Remote,
    Bot(BotConfig),
}

#[derive(Debug, Clone)]
pub enum RemoteObserverConfig {
    TuiSpectator,
    TuiPlayer { player_id: usize },
    TuiOmniscient,
    TuiSnapshot,
}

#[derive(Deserialize)]
struct TaggedConfig {
    kind: String,
    #[serde(default)]
    player_id: Option<usize>,
    #[serde(default)]
    bot: Option<BotConfig>,
}
```

- [ ] **Step 2: Add deserialization compatibility**

In `catan-remote/src/config.rs`, add the current manual deserializers from runtime:

```rust
impl<'de> Deserialize<'de> for SeatConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tagged = TaggedConfig::deserialize(deserializer)?;
        match tagged.kind.as_str() {
            "remote" | "cli" => Ok(Self::Remote),
            "lazy" => Ok(Self::Bot(BotConfig::Lazy)),
            "greedy" => Ok(Self::Bot(BotConfig::Greedy)),
            "random" => Ok(Self::Bot(BotConfig::Random)),
            "bot" => tagged
                .bot
                .map(Self::Bot)
                .ok_or_else(|| de::Error::missing_field("bot")),
            other => Err(de::Error::unknown_variant(
                other,
                &["remote", "cli", "lazy", "greedy", "random", "bot"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for RemoteObserverConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tagged = TaggedConfig::deserialize(deserializer)?;
        match tagged.kind.as_str() {
            "cli_spectator" | "tui_spectator" => Ok(Self::TuiSpectator),
            "cli_player" | "tui_player" => {
                let player_id = tagged
                    .player_id
                    .ok_or_else(|| de::Error::missing_field("player_id"))?;
                Ok(Self::TuiPlayer { player_id })
            }
            "cli_omniscient" | "tui_omniscient" => Ok(Self::TuiOmniscient),
            "snapshot_observer" | "tui_snapshot" => Ok(Self::TuiSnapshot),
            other => Err(de::Error::unknown_variant(
                other,
                &[
                    "cli_spectator",
                    "tui_spectator",
                    "cli_player",
                    "tui_player",
                    "cli_omniscient",
                    "tui_omniscient",
                    "snapshot_observer",
                    "tui_snapshot",
                ],
            )),
        }
    }
}
```

- [ ] **Step 3: Add load and path resolution**

In `catan-remote/src/config.rs`, add:

```rust
pub fn load_config(path: &Path) -> Result<RemoteMatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    let mut config: RemoteMatchConfig = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))?;
    resolve_paths(&mut config, path.parent().unwrap_or_else(|| Path::new(".")));
    Ok(config)
}

pub fn resolve_paths(config: &mut RemoteMatchConfig, base: &Path) {
    if let FieldConfig::LayoutRef { path } = &mut config.field
        && path.is_relative()
    {
        *path = base.join(&path);
    }
    if let InitialStateConfig::Snapshot { path } = &mut config.initial
        && path.is_relative()
    {
        *path = base.join(&path);
    }
}
```

- [ ] **Step 4: Export the module**

In `catan-remote/src/lib.rs`, add:

```rust
pub mod config;
```

- [ ] **Step 5: Update remote binary imports and helpers**

In `catan-remote/src/bin/catan-remote.rs`, replace runtime config imports with:

```rust
use catan_remote::config::{self, RemoteMatchConfig, RemoteObserverConfig, SeatConfig};
use catan_runtime::config::DiceConfig;
```

Change `run_host` to:

```rust
let config = config::load_config(&config_path)?;
run_unix_host(config, &socket_path)
```

Change signatures from `MatchConfig` to `RemoteMatchConfig`.

Use `host::build_initial_engine_from_parts(&config.initial, &config.field, config.players.len(), options)?`.

For bot seats:

```rust
SeatConfig::Bot(bot) => Ok(host::build_bot_seat(bot, player_id)),
SeatConfig::Remote => { ... }
```

Update observer role matching to `RemoteObserverConfig`.

- [ ] **Step 6: Add remote config tests**

In `catan-remote/src/config.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_remote_and_bot_seats() {
        let config: RemoteMatchConfig = serde_json::from_str(
            r#"{
              "players": [
                { "kind": "remote" },
                { "kind": "lazy" }
              ],
              "observers": [{ "kind": "tui_snapshot" }]
            }"#,
        )
        .unwrap();

        assert!(matches!(config.players[0], SeatConfig::Remote));
        assert!(matches!(config.players[1], SeatConfig::Bot(BotConfig::Lazy)));
        assert!(matches!(config.observers[0], RemoteObserverConfig::TuiSnapshot));
    }
}
```

- [ ] **Step 7: Run remote tests**

Run:

```bash
cargo test -p catan-remote
```

Expected: all remote tests pass.

- [ ] **Step 8: Commit**

```bash
git add catan-remote/src/config.rs catan-remote/src/lib.rs catan-remote/src/bin/catan-remote.rs
git commit -m "feat: move remote config to remote crate"
```

## Task 4: Split Config Files by Crate Responsibility

**Files:**
- Move: `catan-runtime/data/configurations/cli_single.json` to `catan-remote/data/configurations/cli_single.json`
- Move: `catan-runtime/data/configurations/cli_1v1.json` to `catan-remote/data/configurations/cli_1v1.json`
- Move: `catan-runtime/data/configurations/observer_debug.json` to `catan-remote/data/configurations/observer_debug.json`
- Modify: pure bot configs under `catan-runtime/data/configurations/`

- [ ] **Step 1: Create remote configuration directory**

Run:

```bash
mkdir -p catan-remote/data/configurations
```

- [ ] **Step 2: Move remote configs**

Run:

```bash
git mv catan-runtime/data/configurations/cli_single.json catan-remote/data/configurations/cli_single.json
git mv catan-runtime/data/configurations/cli_1v1.json catan-remote/data/configurations/cli_1v1.json
git mv catan-runtime/data/configurations/observer_debug.json catan-remote/data/configurations/observer_debug.json
```

- [ ] **Step 3: Update remote layout paths**

In each moved remote config, replace:

```json
"path": "../layouts/standard-4p.layout.json"
```

with:

```json
"path": "../../../catan-runtime/data/layouts/standard-4p.layout.json"
```

- [ ] **Step 4: Keep `cli_single` simple**

Change `catan-remote/data/configurations/cli_single.json` to one remote player plus bots and no extra observer:

```json
{
  "players": [
    { "kind": "remote" },
    { "kind": "lazy" },
    { "kind": "greedy" },
    { "kind": "random" }
  ],
  "observers": [],
  "field": {
    "kind": "layout_ref",
    "path": "../../../catan-runtime/data/layouts/standard-4p.layout.json"
  },
  "dice": { "kind": "random" },
  "limits": { "max_turns": 500 }
}
```

This makes the simple launcher require one TUI client. Keep snapshot/observer examples in `observer_debug.json`.

- [ ] **Step 5: Remove empty observer lists from runtime bot configs**

In pure runtime bot configs, remove:

```json
"observers": [],
```

because runtime-local observers should be opt-in and remote/TUI observers no longer exist in runtime config.

- [ ] **Step 6: Add parse smoke tests through existing binaries**

Run:

```bash
cargo run -p catan-runtime --bin catan-runtime -- catan-runtime/data/configurations/bots3.json
```

Expected: game runs to a result without config parse errors.

Run:

```bash
cargo test -p catan-remote config
```

Expected: remote config tests pass.

- [ ] **Step 7: Commit**

```bash
git add catan-runtime/data/configurations catan-remote/data/configurations
git commit -m "chore: split runtime and remote configs"
```

## Task 5: Add Simple Launch Scripts

**Files:**
- Create: `scripts/run-bots.sh`
- Create: `scripts/run-cli-single.sh`

- [ ] **Step 1: Create bot launcher**

Create `scripts/run-bots.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

CONFIG="catan-runtime/data/configurations/bots3.json"
LOG_FILTER="${RUST_LOG:-info}"
RELEASE=0

usage() {
  cat <<'USAGE'
usage: scripts/run-bots.sh [--config PATH] [--log FILTER] [--release]

Runs a pure bot game through catan-runtime.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config)
      CONFIG="${2:?--config requires a path}"
      shift 2
      ;;
    --log)
      LOG_FILTER="${2:?--log requires a filter}"
      shift 2
      ;;
    --release)
      RELEASE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

CARGO_ARGS=(run -p catan-runtime --bin catan-runtime)
if [[ "$RELEASE" -eq 1 ]]; then
  CARGO_ARGS=(run --release -p catan-runtime --bin catan-runtime)
fi

RUST_LOG="$LOG_FILTER" cargo "${CARGO_ARGS[@]}" -- "$CONFIG"
```

- [ ] **Step 2: Create cli single launcher**

Create `scripts/run-cli-single.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

CONFIG="catan-remote/data/configurations/cli_single.json"
SOCKET="target/catan-remote/cli-single.sock"
LOG_FILTER="${RUST_LOG:-info}"
RELEASE=0

usage() {
  cat <<'USAGE'
usage: scripts/run-cli-single.sh [--config PATH] [--socket PATH] [--log FILTER] [--release]

Starts a catan-remote host in the background, then opens one foreground TUI
client for the remote player seat.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config)
      CONFIG="${2:?--config requires a path}"
      shift 2
      ;;
    --socket)
      SOCKET="${2:?--socket requires a path}"
      shift 2
      ;;
    --log)
      LOG_FILTER="${2:?--log requires a filter}"
      shift 2
      ;;
    --release)
      RELEASE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "$(dirname "$SOCKET")"

CARGO_ARGS=(run -p catan-remote --bin catan-remote)
if [[ "$RELEASE" -eq 1 ]]; then
  CARGO_ARGS=(run --release -p catan-remote --bin catan-remote)
fi

cleanup() {
  if [[ -n "${HOST_PID:-}" ]]; then
    kill "$HOST_PID" >/dev/null 2>&1 || true
    wait "$HOST_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

RUST_LOG="$LOG_FILTER" cargo "${CARGO_ARGS[@]}" -- \
  host --config "$CONFIG" --listen "unix://$SOCKET" &
HOST_PID=$!

for _ in {1..100}; do
  [[ -S "$SOCKET" ]] && break
  sleep 0.05
done

if [[ ! -S "$SOCKET" ]]; then
  echo "host did not create socket: $SOCKET" >&2
  exit 1
fi

RUST_LOG="$LOG_FILTER" cargo "${CARGO_ARGS[@]}" -- \
  tui --connect "unix://$SOCKET" --role player
```

- [ ] **Step 3: Make scripts executable**

Run:

```bash
chmod +x scripts/run-bots.sh scripts/run-cli-single.sh
```

- [ ] **Step 4: Smoke test help output**

Run:

```bash
scripts/run-bots.sh --help
scripts/run-cli-single.sh --help
```

Expected: both print usage and exit successfully.

- [ ] **Step 5: Commit**

```bash
git add scripts/run-bots.sh scripts/run-cli-single.sh
git commit -m "chore: add common launch scripts"
```

## Task 6: Update Documentation for the New System

**Files:**
- Modify: `README.md`
- Modify: `catan-runtime/readme.md`
- Create: `catan-remote/readme.md`
- Create: `docs/configuration.md`
- Modify: `docs/performance-baseline.md` if paths changed by earlier tasks

- [ ] **Step 1: Update root README quick starts**

In `README.md`, replace the old local runtime command section with:

```markdown
# pure bot game
scripts/run-bots.sh

# one local TUI player against bots
scripts/run-cli-single.sh

# bot benchmark
cargo run --release --bin catan-bench -- \
  --games 1000 \
  --no-log
```

Keep training/server sections as-is unless they are stale.

- [ ] **Step 2: Rewrite runtime README**

Replace `catan-runtime/readme.md` with runtime-specific docs:

```markdown
# catan-runtime

`catan-runtime` runs in-process bot matches and simulation-oriented tooling.
It does not create remote or TUI players. Remote seats and TUI observers live in
`catan-remote`.

## Run a bot match

```sh
scripts/run-bots.sh
scripts/run-bots.sh --config catan-runtime/data/configurations/greedy_brawl.json
scripts/run-bots.sh --log warn,catan_runtime=debug
```

## Runtime config

Runtime `players` are bot configs only:

```json
{
  "players": [
    { "kind": "lazy" },
    { "kind": "greedy" },
    { "kind": "random" }
  ]
}
```

`remote`, `cli`, `tui_*`, and `snapshot_observer` are not valid runtime config
options. Use `catan-remote/data/configurations/` for those flows.

## Benchmarks

```sh
cargo run --release --bin catan-bench -- \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --no-log
```
```

- [ ] **Step 3: Add remote README**

Create `catan-remote/readme.md`:

```markdown
# catan-remote

`catan-remote` hosts games with remote player seats and remote TUI observers.
It reuses runtime bot setup, persistence, and shared match options, but owns all
remote-specific config.

## One TUI player against bots

```sh
scripts/run-cli-single.sh
```

Equivalent manual commands:

```sh
cargo run -p catan-remote --bin catan-remote -- \
  host --config catan-remote/data/configurations/cli_single.json \
  --listen unix://target/catan-remote/cli-single.sock

cargo run -p catan-remote --bin catan-remote -- \
  tui --connect unix://target/catan-remote/cli-single.sock \
  --role player
```

## Remote config

Remote configs can mix remote seats and bots:

```json
{
  "players": [
    { "kind": "remote" },
    { "kind": "lazy" },
    { "kind": "greedy" },
    { "kind": "random" }
  ],
  "observers": []
}
```

TUI observers are configured here, not in runtime:

```json
{ "kind": "tui_omniscient" }
{ "kind": "tui_player", "player_id": 0 }
{ "kind": "tui_snapshot" }
```

The host sends an observer-derived final game summary to connected TUI players
and observers when the match ends.

## Logging

Use `--log` through scripts or set `RUST_LOG` manually:

```sh
scripts/run-cli-single.sh --log warn,catan_remote=debug,catan_runtime=debug
```
```

- [ ] **Step 4: Add shared configuration doc**

Create `docs/configuration.md`:

```markdown
# Configuration

There are two match config families.

## Runtime configs

Runtime configs live under `catan-runtime/data/configurations/` and are for
pure bot matches. Their `players` array accepts only:

- `{ "kind": "lazy" }`
- `{ "kind": "greedy" }`
- `{ "kind": "random" }`

## Remote configs

Remote configs live under `catan-remote/data/configurations/` and are for games
with remote TUI clients. Their `players` array accepts:

- `{ "kind": "remote" }`
- bot entries accepted by runtime

Remote `observers` accepts TUI observer entries such as `tui_omniscient`,
`tui_player`, and `tui_snapshot`.

## Shared fields

Both config families support:

- `initial`
- `field`
- `dice`
- `limits`
- `logging`
- `persistence`

Relative paths are resolved from the config file directory.
```

- [ ] **Step 5: Update performance docs if needed**

If pure bot config paths changed, update `docs/performance-baseline.md`. Do not mention remote configs there unless a benchmark actually uses them.

- [ ] **Step 6: Run doc path checks**

Run:

```bash
rg "catan-runtime/data/configurations/(cli_single|cli_1v1|observer_debug)" README.md catan-runtime catan-remote docs scripts
rg "cargo run --bin catan-runtime" README.md catan-runtime catan-remote docs scripts
```

Expected: no stale remote config paths under runtime, and no docs telling users to launch TUI flows through `catan-runtime`.

- [ ] **Step 7: Commit**

```bash
git add README.md catan-runtime/readme.md catan-remote/readme.md docs/configuration.md docs/performance-baseline.md
git commit -m "docs: explain runtime and remote configs"
```

## Task 7: Final Verification

**Files:**
- No required source changes.

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test -p catan-core game
cargo test -p catan-runtime
cargo test -p catan-runtime --bin catan-bench
cargo test -p catan-remote
cargo test -p catan-tui
```

Expected: all pass.

- [ ] **Step 2: Run script help checks**

Run:

```bash
scripts/run-bots.sh --help
scripts/run-cli-single.sh --help
```

Expected: both print usage successfully.

- [ ] **Step 3: Run a pure bot smoke test**

Run:

```bash
scripts/run-bots.sh --config catan-runtime/data/configurations/bots3.json --log warn
```

Expected: match completes without config parse errors.

- [ ] **Step 4: Run a remote build check**

Run:

```bash
cargo build -p catan-remote --bin catan-remote
```

Expected: remote binary builds.

- [ ] **Step 5: Search for stale boundaries**

Run:

```bash
rg "SeatConfig|RemoteObserverConfig|TuiSnapshot|snapshot_observer|cli_omniscient" catan-runtime
rg "catan-runtime/data/configurations/(cli_single|cli_1v1|observer_debug)" .
```

Expected: first command has no runtime remote/TUI config references; second command has no stale path references outside old plan docs if those are intentionally historical.

- [ ] **Step 6: Commit any verification fixes**

If verification reveals only small path/doc fixes:

```bash
git add .
git commit -m "fix: clean stale config references"
```

## Self-Review

- Scope coverage: Includes the current missing post-game summary delivery to remote players, runtime bot-only config, remote-owned mixed config, config file split, launch scripts, and docs/readmes.
- Boundary check: Runtime no longer recognizes remote seats or TUI observer config after Task 2. Remote owns those concepts after Task 3.
- Developer experience check: Users get two simple scripts, root quick starts, runtime docs, remote docs, and a shared configuration reference.
- Verification check: Plan includes focused crate tests, script help checks, bot smoke test, remote build check, and stale-reference searches.
