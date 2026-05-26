# Observer-Derived Game Summary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move run/game summary collection out of core and host internals into runtime observers, then send a rich post-game summary to remote TUI clients.

**Architecture:** `catan-core` emits `GameEvent::GameEnded { result }` as the authoritative game-lifecycle event. `catan-runtime` owns reusable observer-derived statistics and final-view capture. `catan-remote` uses that observer handle after the game ends and sends a dedicated `HostMessage::GameSummary` to TUI clients.

**Tech Stack:** Rust 2024, serde/serde_json, catan-core event/projector APIs, catan-runtime `OutputObserver`, catan-remote framed Unix socket protocol, Ratatui TUI rendering.

---

## File Structure

- Modify `catan-core/src/gameplay/game/io/event.rs`: replace `GameFinished { result, stats }` with `GameEnded { result }`.
- Modify `catan-core/src/gameplay/game/engine/decider.rs`: stop computing `GameEndStats` in core and emit `GameEnded`.
- Modify `catan-core/src/gameplay/game/engine/reducer.rs`: match the renamed event.
- Modify `catan-core/src/gameplay/game/io/projector.rs`: no structural change expected beyond renamed event tests.
- Modify `catan-runtime/src/run_stats.rs`: add observer-owned summary collection, dice histogram, final-view capture, and a shared handle.
- Modify `catan-runtime/src/sync_host.rs`: remove direct stats collection and rely on observers.
- Modify `catan-runtime/src/simulation.rs`: support output observers and remove direct stats collection.
- Modify `catan-runtime/src/bin/catan-bench.rs`: attach `RunStatsObserver` and read stats from its handle.
- Modify `catan-remote/src/protocol.rs`: add `HostMessage::GameSummary`.
- Modify `catan-remote/src/runtime_adapter.rs`: let remote CLI observers receive post-game summary frames.
- Modify `catan-remote/src/bin/catan-remote.rs`: attach stats observer and send summary after `run_to_result`.
- Modify `catan-remote/src/tui_adapter.rs`: display `GameSummary` as the graceful final state.
- Modify `catan-tui/src/tui.rs` and `catan-tui/src/panels.rs`: render final summary from observer-derived data instead of core `GameEndStats`.
- Update tests in touched modules.

## Task 1: Rename Core Game-End Event

**Files:**
- Modify: `catan-core/src/gameplay/game/io/event.rs`
- Modify: `catan-core/src/gameplay/game/engine/decider.rs`
- Modify: `catan-core/src/gameplay/game/engine/reducer.rs`
- Modify: `catan-core/src/gameplay/game/engine/tests.rs`
- Modify: `catan-runtime/src/run_stats.rs`
- Modify: `catan-runtime/src/sync_host.rs`
- Modify: `catan-remote/src/tui_adapter.rs`
- Modify: `catan-tui/src/tui.rs`
- Modify: `catan-tui/src/journal.rs`

- [ ] **Step 1: Change the core event type**

In `catan-core/src/gameplay/game/io/event.rs`, remove `GameEndPlayerStats`, `GameEndStats`, and the import of `GAME_END_STATS_INLINE`. Replace the event variant with:

```rust
GameEnded {
    result: GameResult,
},
```

- [ ] **Step 2: Update core decider emission**

In `catan-core/src/gameplay/game/engine/decider.rs`, remove `game_end_stats` and every `stats: Some(...)` / `stats: None` field from game-end event construction.

Use this shape everywhere:

```rust
events.push(GameEvent::GameEnded { result });
```

For call sites that currently inline the result, use:

```rust
events.push(GameEvent::GameEnded {
    result: GameResult::Win(player_id),
});
```

- [ ] **Step 3: Update reducer and tests**

In `catan-core/src/gameplay/game/engine/reducer.rs`, replace:

```rust
GameEvent::GameFinished { result, .. } => finish(lifecycle, result),
```

with:

```rust
GameEvent::GameEnded { result } => finish(lifecycle, result),
```

Update ignore arms similarly:

```rust
GameEvent::GameEnded { .. } => {}
```

- [ ] **Step 4: Run focused core tests**

Run:

```bash
cargo test -p catan-core game
```

Expected: compile errors only for remaining `GameFinished` references on first run; after updates, all `catan-core` game tests pass.

- [ ] **Step 5: Commit**

```bash
git add catan-core/src/gameplay/game/io/event.rs \
  catan-core/src/gameplay/game/engine/decider.rs \
  catan-core/src/gameplay/game/engine/reducer.rs \
  catan-core/src/gameplay/game/engine/tests.rs
git commit -m "refactor: simplify game ended event"
```

## Task 2: Add Observer-Derived Runtime Summary Types

**Files:**
- Modify: `catan-runtime/src/run_stats.rs`

- [ ] **Step 1: Add summary data types**

Extend `catan-runtime/src/run_stats.rs` with these public types near `GameRunStats`:

```rust
use std::{cell::RefCell, rc::Rc};

use catan_core::gameplay::{
    game::{
        event::GameEvent,
        output::GameOutput,
        projection::GameProjection,
        run::GameResult,
    },
    primitives::{
        build::Build,
        dev_card::UsableDevCardSet,
        player::PlayerId,
        resource::ResourceSet,
    },
};
use catan_core::math::dice::DiceRoll;
use serde::{Deserialize, Serialize};

use crate::sync_host::{ObserverFrame, OutputObserver};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiceRollHistogram {
    pub counts: [u64; DiceRoll::COUNT],
}

impl Default for DiceRollHistogram {
    fn default() -> Self {
        Self {
            counts: [0; DiceRoll::COUNT],
        }
    }
}

impl DiceRollHistogram {
    pub fn record(&mut self, roll: DiceRoll) {
        self.counts[(roll.get() - DiceRoll::MIN_VALUE) as usize] += 1;
    }

    pub fn count(&self, roll: DiceRoll) -> u64 {
        self.counts[(roll.get() - DiceRoll::MIN_VALUE) as usize]
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFlowStats {
    pub distributed: ResourceSet,
    pub discarded: ResourceSet,
    pub stolen: ResourceSet,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildStats {
    pub roads: u64,
    pub settlements: u64,
    pub cities: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSummary {
    pub result: Option<GameResult>,
    pub run: GameRunStats,
    pub dice: DiceRollHistogram,
    pub resources: ResourceFlowStats,
    pub builds: BuildStats,
    pub dev_cards_used: UsableDevCardSet,
    pub final_view: Option<GameProjection>,
}
```

- [ ] **Step 2: Add handle and observer**

Add this below `RunStatsCollector`:

```rust
#[derive(Debug, Default, Clone)]
pub struct RunStatsHandle {
    inner: Rc<RefCell<GameSummary>>,
}

impl RunStatsHandle {
    pub fn summary(&self) -> GameSummary {
        self.inner.borrow().clone()
    }

    pub fn stats(&self) -> GameRunStats {
        self.inner.borrow().run
    }
}

#[derive(Debug, Clone)]
pub struct RunStatsObserver {
    inner: Rc<RefCell<GameSummary>>,
    collector: RunStatsCollector,
}

impl RunStatsObserver {
    pub fn new() -> (Self, RunStatsHandle) {
        let inner = Rc::new(RefCell::new(GameSummary::default()));
        (
            Self {
                inner: inner.clone(),
                collector: RunStatsCollector::default(),
            },
            RunStatsHandle { inner },
        )
    }
}
```

- [ ] **Step 3: Implement event collection**

Add helper methods and `OutputObserver` implementation:

```rust
impl RunStatsObserver {
    fn record_event(&mut self, event: &GameEvent, frame: &ObserverFrame<'_>) {
        self.collector.record_event(event);

        let mut summary = self.inner.borrow_mut();
        summary.run = self.collector.stats();

        match event {
            GameEvent::DiceRolled { value, .. } => summary.dice.record(*value),
            GameEvent::ResourcesDistributed { by_player } => {
                for (_, resources) in by_player {
                    summary.resources.distributed += *resources;
                }
            }
            GameEvent::PlayerDiscarded { resources, .. } => {
                summary.resources.discarded += *resources;
            }
            GameEvent::ResourceStolen { resource, .. } => {
                summary.resources.stolen[*resource] += 1;
            }
            GameEvent::Built { build, .. } => match build {
                Build::Road(_) => summary.builds.roads += 1,
                Build::Settlement(_) => summary.builds.settlements += 1,
                Build::City(_) => summary.builds.cities += 1,
            },
            GameEvent::DevCardUsed { usage, .. } => {
                summary.dev_cards_used[usage.card_kind()] += 1;
            }
            GameEvent::GameEnded { result } => {
                summary.result = Some(result.clone());
                summary.final_view = Some(GameProjection::from_observer(
                    catan_core::gameplay::game::event::ObserverNotificationContext::Omniscient {
                        public: frame.factory.public_view(
                            catan_core::gameplay::game::view::VisibilityPolicy::Omniscient,
                        ),
                        full: frame.factory.omniscient_view(),
                    },
                    false,
                ));
            }
            _ => {}
        }
    }
}

impl OutputObserver for RunStatsObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        let GameOutput::Event(record) = frame.output else {
            return;
        };
        self.record_event(&record.event, &frame);
    }
}
```

`DevCardUsage::card_kind()` already exists in `catan-core/src/gameplay/primitives/dev_card.rs`; use it instead of adding a duplicate helper.

- [ ] **Step 4: Update collector match arm**

In `RunStatsCollector::record_event`, replace:

```rust
GameEvent::GameFinished { result, .. } => match result {
```

with:

```rust
GameEvent::GameEnded { result } => match result {
```

- [ ] **Step 5: Add focused tests**

Add tests to `catan-runtime/src/run_stats.rs`:

```rust
#[test]
fn stats_observer_records_dice_and_final_view() {
    use catan_core::dice_roll;
    use catan_core::gameplay::{
        game::{
            engine::GameEngine,
            event::{EventVisibility, GameEvent},
            output::{GameEventRecord, GameOutput},
            run::GameResult,
            state::SetupGameState,
            view::{ContextFactory, VisibilityConfig},
        },
        primitives::player::PlayerId,
    };

    let init = SetupGameState::default();
    let engine = GameEngine::from_init(init, Default::default());
    let visibility = VisibilityConfig::default();
    let factory = ContextFactory {
        state: engine.table(),
        index: engine.index(),
        visibility: &visibility,
        trade_sessions: engine.trade_sessions(),
    };
    let (mut observer, handle) = RunStatsObserver::new();

    observer.on_output(ObserverFrame {
        output: &GameOutput::event(GameEventRecord {
            event: GameEvent::DiceRolled {
                player_id: PlayerId::new(0),
                value: dice_roll!(8),
            },
            visibility: EventVisibility::Public,
        }),
        factory: &factory,
        engine: &engine,
    });
    observer.on_output(ObserverFrame {
        output: &GameOutput::event(GameEventRecord {
            event: GameEvent::GameEnded {
                result: GameResult::Win(PlayerId::new(0)),
            },
            visibility: EventVisibility::Public,
        }),
        factory: &factory,
        engine: &engine,
    });

    let summary = handle.summary();
    assert_eq!(summary.dice.count(dice_roll!(8)), 1);
    assert!(matches!(summary.result, Some(GameResult::Win(_))));
    assert!(summary.final_view.is_some());
    assert!(summary.final_view.unwrap().snapshot_state.is_none());
}
```

- [ ] **Step 6: Run focused runtime stats tests**

Run:

```bash
cargo test -p catan-runtime run_stats
```

Expected: all `run_stats` tests pass.

- [ ] **Step 7: Commit**

```bash
git add catan-runtime/src/run_stats.rs catan-core/src/gameplay/primitives/dev_card.rs
git commit -m "feat: collect run stats with observer"
```

## Task 3: Make SyncGameHost Observer-Only for Stats

**Files:**
- Modify: `catan-runtime/src/sync_host.rs`

- [ ] **Step 1: Remove direct stats field**

In `SyncGameHost`, delete:

```rust
stats: RunStatsCollector,
```

Delete initialization:

```rust
stats: RunStatsCollector::default(),
```

Delete direct record calls:

```rust
self.stats.record_transaction(&transaction);
```

Delete:

```rust
pub fn run_stats(&self) -> GameRunStats {
    self.stats.stats()
}
```

- [ ] **Step 2: Update tests to attach observer**

Where tests call `host.run_stats()`, replace with:

```rust
let (stats_observer, stats_handle) = crate::run_stats::RunStatsObserver::new();
host.add_observer(Box::new(stats_observer));
```

Then assert:

```rust
let stats = stats_handle.stats();
assert_eq!(stats.game_started, 1);
assert_eq!(stats.games_interrupted, 1);
```

- [ ] **Step 3: Update terminal output test**

Replace remaining `GameEvent::GameFinished { ... }` matches with:

```rust
GameEvent::GameEnded {
    result: GameResult::LimitReached { turns: 0 },
}
```

- [ ] **Step 4: Run sync host tests**

Run:

```bash
cargo test -p catan-runtime sync_host
```

Expected: all sync host tests pass.

- [ ] **Step 5: Commit**

```bash
git add catan-runtime/src/sync_host.rs
git commit -m "refactor: observe sync host stats externally"
```

## Task 4: Make SimulationHost Support Observers

**Files:**
- Modify: `catan-runtime/src/simulation.rs`

- [ ] **Step 1: Add observer storage**

Change `SimulationHost` fields:

```rust
pub struct SimulationHost {
    engine: GameEngine,
    bots: Vec<Box<dyn BotPolicy>>,
    visibility: VisibilityConfig,
    observers: Vec<Box<dyn OutputObserver>>,
}
```

Add imports:

```rust
use crate::sync_host::{ObserverFrame, OutputObserver};
```

- [ ] **Step 2: Initialize and expose observers**

In `new`, initialize:

```rust
observers: Vec::new(),
```

Add:

```rust
pub fn add_observer(&mut self, observer: Box<dyn OutputObserver>) {
    self.observers.push(observer);
}
```

- [ ] **Step 3: Deliver projected outputs to observers**

Add helper:

```rust
fn project_and_deliver(
    &mut self,
    transaction: &catan_core::gameplay::game::event::EventTransaction,
    queue: &mut VecDeque<GameOutput>,
) {
    for output in projector::project_transaction(transaction) {
        self.deliver_output(&output);
        queue.push_back(output);
    }
}

fn deliver_output(&mut self, output: &GameOutput) {
    let factory = ContextFactory {
        state: self.engine.table(),
        index: self.engine.index(),
        visibility: &self.visibility,
        trade_sessions: self.engine.trade_sessions(),
    };
    for observer in &mut self.observers {
        observer.on_output(ObserverFrame {
            output,
            factory: &factory,
            engine: &self.engine,
        });
    }
}
```

Replace both `queue.extend(projector::project_transaction(&transaction));` calls with:

```rust
self.project_and_deliver(&transaction, &mut queue);
```

- [ ] **Step 4: Remove old stats method**

Delete direct stats field and:

```rust
pub fn run_stats(&self) -> GameRunStats {
    self.stats.stats()
}
```

- [ ] **Step 5: Update simulation test**

In `lazy_bots_reach_turn_limit`, attach observer:

```rust
let (stats_observer, stats_handle) = crate::run_stats::RunStatsObserver::new();
host.add_observer(Box::new(stats_observer));
```

Then assert:

```rust
let stats = stats_handle.stats();
assert_eq!(stats.game_started, 1);
assert_eq!(stats.turns_started, 3);
assert_eq!(stats.games_interrupted, 1);
```

- [ ] **Step 6: Run simulation tests**

Run:

```bash
cargo test -p catan-runtime simulation
```

Expected: all simulation tests pass.

- [ ] **Step 7: Commit**

```bash
git add catan-runtime/src/simulation.rs
git commit -m "refactor: add simulation observers"
```

## Task 5: Update Benchmark Stats Collection

**Files:**
- Modify: `catan-runtime/src/bin/catan-bench.rs`

- [ ] **Step 1: Attach stats observer in `run_one_game`**

Replace:

```rust
let result = host.run();

Ok(GameOutcome {
    result,
    stats: host.run_stats(),
})
```

with:

```rust
let (stats_observer, stats_handle) = catan_runtime::run_stats::RunStatsObserver::new();
host.add_observer(Box::new(stats_observer));
let result = host.run();

Ok(GameOutcome {
    result,
    stats: stats_handle.stats(),
})
```

- [ ] **Step 2: Update renamed event references if any**

Run:

```bash
rg "GameFinished|GameEnded" catan-runtime/src/bin/catan-bench.rs
```

Expected: no `GameFinished` references remain.

- [ ] **Step 3: Run benchmark tests**

Run:

```bash
cargo test -p catan-runtime --bin catan-bench
```

Expected: benchmark tests pass with unchanged deterministic stats.

- [ ] **Step 4: Commit**

```bash
git add catan-runtime/src/bin/catan-bench.rs
git commit -m "refactor: collect bench stats through observer"
```

## Task 6: Add Remote GameSummary Protocol

**Files:**
- Modify: `catan-remote/src/protocol.rs`
- Modify: `catan-remote/src/runtime_adapter.rs`

- [ ] **Step 1: Add protocol variant**

In `catan-remote/src/protocol.rs`, import:

```rust
use catan_runtime::run_stats::GameSummary;
```

Add to `HostMessage`:

```rust
GameSummary {
    summary: GameSummary,
},
```

- [ ] **Step 2: Forward summary to remote observer streams**

In `catan-remote/src/runtime_adapter.rs`, add to `RemoteCliOutputObserver`:

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

If object-safe post-game dispatch is needed from `catan-remote/src/bin/catan-remote.rs`, introduce a remote-only wrapper enum instead of adding this method to the runtime `OutputObserver` trait. Keep `catan-runtime` independent from remote protocol.

- [ ] **Step 3: Add protocol serialization test**

Add a test in `catan-remote/src/protocol.rs`:

```rust
#[test]
fn game_summary_message_round_trips() {
    let message = HostMessage::GameSummary {
        summary: GameSummary::default(),
    };
    let encoded = serde_json::to_string(&message).unwrap();
    let decoded: HostMessage = serde_json::from_str(&encoded).unwrap();
    assert!(matches!(decoded, HostMessage::GameSummary { .. }));
}
```

- [ ] **Step 4: Run remote protocol tests**

Run:

```bash
cargo test -p catan-remote protocol
```

Expected: protocol tests pass.

- [ ] **Step 5: Commit**

```bash
git add catan-remote/src/protocol.rs catan-remote/src/runtime_adapter.rs
git commit -m "feat: add remote game summary frame"
```

## Task 7: Send Summary After Remote Match Completion

**Files:**
- Modify: `catan-remote/src/bin/catan-remote.rs`

- [ ] **Step 1: Keep remote observers addressable**

Change remote observer construction so the host can send post-game frames after `run_to_result`. Replace `build_remote_observers` with a helper that returns concrete remote observers and registers boxed adapters.

Use this local holder:

```rust
struct RemoteObserverSlot {
    observer: std::rc::Rc<std::cell::RefCell<RemoteCliOutputObserver>>,
}

impl OutputObserver for RemoteObserverSlot {
    fn on_output(&mut self, frame: catan_runtime::sync_host::ObserverFrame<'_>) {
        self.observer.borrow_mut().on_output(frame);
    }
}
```

Return both:

```rust
struct BuiltRemoteObservers {
    host_observers: Vec<Box<dyn OutputObserver>>,
    summary_targets: Vec<std::rc::Rc<std::cell::RefCell<RemoteCliOutputObserver>>>,
}
```

- [ ] **Step 2: Attach stats observer before game starts**

In `run_unix_host`, add:

```rust
let (stats_observer, stats_handle) = catan_runtime::run_stats::RunStatsObserver::new();
game_host.add_observer(Box::new(stats_observer));
```

Register it before remote observers so it observes every output.

- [ ] **Step 3: Send summary after result**

After:

```rust
let result = game_host.run_to_result();
```

add:

```rust
let mut summary = stats_handle.summary();
if summary.result.is_none() {
    summary.result = Some(result.clone());
}
for observer in remote_observers.summary_targets {
    observer.borrow_mut().send_summary(&summary);
}
```

- [ ] **Step 4: Run remote binary tests/build**

Run:

```bash
cargo test -p catan-remote
cargo build -p catan-remote --bin catan-remote
```

Expected: tests pass and binary builds.

- [ ] **Step 5: Commit**

```bash
git add catan-remote/src/bin/catan-remote.rs
git commit -m "feat: send remote game summaries"
```

## Task 8: Render Observer-Derived Final Screen in TUI

**Files:**
- Modify: `catan-tui/src/panels.rs`
- Modify: `catan-tui/src/tui.rs`
- Modify: `catan-remote/src/tui_adapter.rs`

- [ ] **Step 1: Add a TUI-owned display DTO**

In `catan-tui/src/tui.rs` or a small new module `catan-tui/src/summary.rs`, define a display-only DTO that depends only on `catan-core` and primitive values:

```rust
#[derive(Debug, Clone)]
pub struct FinalGameSummaryView {
    pub result: String,
    pub turns_started: u64,
    pub dice_counts: Vec<(u8, u64)>,
    pub resources_distributed: u16,
    pub resources_discarded: u16,
    pub resources_stolen: u16,
    pub roads_built: u64,
    pub settlements_built: u64,
    pub cities_built: u64,
    pub knights_used: u16,
    pub year_of_plenty_used: u16,
    pub road_build_used: u16,
    pub monopoly_used: u16,
}
```

Do not add a `catan-runtime` dependency to `catan-tui`.

- [ ] **Step 2: Add final summary panel renderer**

In `catan-tui/src/panels.rs`, add:

```rust
pub fn game_summary_lines(summary: &FinalGameSummaryView) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Game ended",
            Style::default().fg(Color::Green),
        )),
        Line::from(format!("result: {}", summary.result)),
        Line::from(format!("turns: {}", summary.turns_started)),
        Line::from(""),
        Line::from("dice"),
    ];

    for (roll, count) in &summary.dice_counts {
        lines.push(Line::from(format!("{roll:>2}: {count}")));
    }

    lines.extend([
        Line::from(""),
        Line::from(format!(
            "resources distributed: {}",
            summary.resources_distributed
        )),
        Line::from(format!("resources discarded: {}", summary.resources_discarded)),
        Line::from(format!("resources stolen: {}", summary.resources_stolen)),
        Line::from(format!(
            "builds: roads {} settlements {} cities {}",
            summary.roads_built, summary.settlements_built, summary.cities_built
        )),
        Line::from(format!(
            "dev cards used: knights {} yp {} rb {} monopoly {}",
            summary.knights_used,
            summary.year_of_plenty_used,
            summary.road_build_used,
            summary.monopoly_used
        )),
        Line::from(Span::styled(
            "[press esc to quit]",
            Style::default().fg(Color::Yellow),
        )),
    ]);

    lines
}
```

- [ ] **Step 3: Add TUI method**

In `catan-tui/src/tui.rs`, add:

```rust
pub fn show_game_summary(
    &mut self,
    summary: &FinalGameSummaryView,
    final_view: Option<&GameProjection>,
) -> io::Result<()> {
    self.message = "game ended".to_owned();
    self.overlay.selected = None;
    self.overlay.status = SelectionStatus::Neutral;
    self.overlay.preview.clear();
    self.public_override = Some(game_summary_lines(summary));
    self.personal_override = None;
    self.interactive_override = None;
    self.show_command_help = false;

    loop {
        self.draw(final_view, "[press esc to quit]", "")?;
        if let CrosstermEvent::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Esc
        {
            self.public_override = None;
            return Ok(());
        }
    }
}
```

- [ ] **Step 4: Map runtime summary in remote adapter**

In `catan-remote/src/tui_adapter.rs`, add:

```rust
fn final_summary_view(summary: &catan_runtime::run_stats::GameSummary) -> FinalGameSummaryView {
    FinalGameSummaryView {
        result: summary
            .result
            .as_ref()
            .map(|result| format!("{result:?}"))
            .unwrap_or_else(|| "unknown".to_owned()),
        turns_started: summary.run.turns_started,
        dice_counts: catan_core::math::dice::DiceRoll::iter()
            .map(|roll| (roll.get(), summary.dice.count(roll)))
            .collect(),
        resources_distributed: summary.resources.distributed.total(),
        resources_discarded: summary.resources.discarded.total(),
        resources_stolen: summary.resources.stolen.total(),
        roads_built: summary.builds.roads,
        settlements_built: summary.builds.settlements,
        cities_built: summary.builds.cities,
        knights_used: summary.dev_cards_used.knight,
        year_of_plenty_used: summary.dev_cards_used.year_of_plenty,
        road_build_used: summary.dev_cards_used.road_build,
        monopoly_used: summary.dev_cards_used.monopoly,
    }
}
```

- [ ] **Step 5: Consume protocol frame**

In `catan-remote/src/tui_adapter.rs`, handle `HostMessage::GameSummary` in player and observer loops:

```rust
HostMessage::GameSummary { summary } => {
    let final_view = summary.final_view.as_ref();
    let display = final_summary_view(&summary);
    ui.show_game_summary(&display, final_view)
        .map_err(|err| format!("failed to draw game summary: {err}"))?;
    return Ok(());
}
```

- [ ] **Step 6: Stop special-casing core stats**

In `process_host_event`, remove the old `GameFinished { stats: Some(_) }` branch. The final overlay now comes from `HostMessage::GameSummary`.

- [ ] **Step 7: Run TUI and remote tests**

Run:

```bash
cargo test -p catan-tui
cargo test -p catan-remote
```

Expected: both crates pass.

- [ ] **Step 8: Commit**

```bash
git add catan-tui/src/panels.rs catan-tui/src/tui.rs catan-remote/src/tui_adapter.rs
git commit -m "feat: render observer game summaries"
```

## Task 9: Finish Event Rename Across Workspace

**Files:**
- Modify every remaining file reported by `rg`.

- [ ] **Step 1: Search for stale event names and core stats**

Run:

```bash
rg "GameFinished|GameEndStats|GameEndPlayerStats|stats: Some|stats: None" catan-core catan-runtime catan-remote catan-tui
```

Expected before cleanup: only references already identified in this plan.

- [ ] **Step 2: Replace stale references**

Replace stale matches with:

```rust
GameEvent::GameEnded { result }
```

or summary rendering paths, depending on context.

- [ ] **Step 3: Run workspace tests**

Run:

```bash
cargo test --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 4: Commit**

```bash
git add catan-core catan-runtime catan-remote catan-tui
git commit -m "refactor: finish game ended migration"
```

## Task 10: Manual Remote Smoke Test

**Files:**
- No required source changes.

- [ ] **Step 1: Build remote binary**

Run:

```bash
cargo build -p catan-remote --bin catan-remote
```

Expected: build succeeds.

- [ ] **Step 2: Run a short remote game**

Use a config with one remote player and low max turns. In terminal A:

```bash
RUST_LOG=info cargo run -p catan-remote --bin catan-remote -- \
  host --config catan-remote/data/configurations/cli_single.json \
  --listen unix://target/catan-remote/cli-single.sock
```

In terminal B:

```bash
RUST_LOG=info cargo run -p catan-remote --bin catan-remote -- \
  tui --connect unix://target/catan-remote/cli-single.sock \
  --role player
```

Expected: when the game ends, the TUI displays the observer-derived final summary instead of reporting an unexpected EOF.

- [ ] **Step 3: Commit smoke-test docs if commands changed**

If config paths are moved during the later runtime/remote config split, update the command paths and commit the docs with that later split.

## Self-Review

- Spec coverage: Covers core event simplification, observer-derived stats, final full-view capture, remote summary transport, TUI display, benchmark migration, and tests.
- Placeholder scan: No placeholder markers or undefined future tasks remain.
- Type consistency: The plan uses `GameEnded`, `GameSummary`, `RunStatsObserver`, `RunStatsHandle`, and `DevCardUsage::card_kind()` consistently.
- Scope note: This plan intentionally does not perform the `SeatConfig`/remote config split or launcher scripts. Those should be a separate plan/commit series after summary migration, because this plan changes event semantics and host observation behavior.
