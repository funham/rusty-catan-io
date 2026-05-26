# Borrowed Runtime Observers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace host-owned runtime observers with caller-owned borrowed observers so observer state can be inspected directly after a run without `Rc<RefCell<_>>` handles.

**Architecture:** `SimulationHost` and `SyncGameHost` keep ownership of engine/orchestration state only. Callers pass `&mut [&mut dyn OutputObserver]` into observed run methods, and observer-free convenience methods construct an empty observer slice internally. `RunStatsObserver` owns `GameSummary` directly and exposes read-only accessors after the run.

**Tech Stack:** Rust 2024, `catan-runtime`, `catan-core` game output/projector APIs, trait-object slices with explicit mutable reborrowing.

---

## Scope

This plan migrates observers only. It intentionally does not move `SimulationHost` bots or `SyncGameHost` seats to borrowed ownership in the same change. That keeps the migration small, removes the current `RunStatsObserver` internal mutability, and creates the pattern that can be applied to seats/bots in a separate plan.

## File Structure

- Modify `catan-runtime/src/run_stats.rs`: remove `Rc<RefCell<_>>` and `RunStatsHandle`; make `RunStatsObserver` directly own `GameSummary`.
- Modify `catan-runtime/src/simulation.rs`: remove the owned observer vector and `add_observer`; add borrowed-observer run/delivery methods.
- Modify `catan-runtime/src/sync_host.rs`: remove the owned observer vector and `add_observer`; add borrowed-observer run/delivery methods while preserving no-observer convenience methods.
- Modify `catan-runtime/src/host.rs`: keep persistence observer in the caller scope and pass a borrowed observer slice to `SyncGameHost`.
- Modify `catan-runtime/src/bin/catan-bench.rs`: keep `RunStatsObserver` in the benchmark scope and pass it by mutable borrow to `SimulationHost`.
- Update tests in `catan-runtime/src/run_stats.rs`, `catan-runtime/src/simulation.rs`, and `catan-runtime/src/sync_host.rs`.

## Task 1: Make `RunStatsObserver` Own Its Summary Directly

**Files:**
- Modify: `catan-runtime/src/run_stats.rs`

- [ ] **Step 1: Remove shared-state imports**

At the top of `catan-runtime/src/run_stats.rs`, replace:

```rust
use std::{cell::RefCell, rc::Rc};
```

with no `std` import for this module. If no other `std` items are needed, delete the line entirely.

- [ ] **Step 2: Replace handle-based observer state**

Remove the entire `RunStatsHandle` type and replace the current observer definition:

```rust
#[derive(Debug, Clone)]
pub struct RunStatsObserver {
    inner: Rc<RefCell<GameSummary>>,
    collector: RunStatsCollector,
}
```

with:

```rust
#[derive(Debug, Default, Clone)]
pub struct RunStatsObserver {
    summary: GameSummary,
    collector: RunStatsCollector,
}
```

- [ ] **Step 3: Replace `new` and add direct accessors**

Replace the existing `impl RunStatsObserver` constructor with:

```rust
impl RunStatsObserver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn summary(&self) -> &GameSummary {
        &self.summary
    }

    pub fn summary_owned(&self) -> GameSummary {
        self.summary.clone()
    }

    pub fn stats(&self) -> GameRunStats {
        self.summary.run
    }

    fn record_event(&mut self, event: &GameEvent, frame: &ObserverFrame<'_>) {
        self.collector.record_event(event);
        self.summary.run = self.collector.stats();

        match event {
            GameEvent::DiceRolled { value, .. } => self.summary.dice.record(*value),
            GameEvent::ResourcesDistributed { by_player } => {
                for (_, resources) in by_player {
                    self.summary.resources.distributed += *resources;
                }
            }
            GameEvent::PlayerDiscarded { resources, .. } => {
                self.summary.resources.discarded += *resources;
            }
            GameEvent::ResourceStolen { resource, .. } => {
                self.summary.resources.stolen[*resource] += 1;
            }
            GameEvent::Built { build, .. } => match build {
                Build::Road(_) => self.summary.builds.roads += 1,
                Build::Establishment(establishment) => match establishment.stage {
                    EstablishmentType::Settlement => self.summary.builds.settlements += 1,
                    EstablishmentType::City => self.summary.builds.cities += 1,
                },
            },
            GameEvent::DevCardUsed { usage, .. } => {
                self.summary.dev_cards_used[usage.card_kind()] += 1;
            }
            GameEvent::GameEnded { result } => {
                self.summary.result = Some(result.clone());
                self.summary.final_view = Some(GameProjection::from_observer(
                    ObserverNotificationContext::Omniscient {
                        public: frame.factory.public_view(VisibilityPolicy::Omniscient),
                        full: frame.factory.omniscient_view(),
                    },
                    false,
                ));
            }
            _ => {}
        }
    }
}
```

This keeps the same event logic but removes runtime borrow checks and shared ownership.

- [ ] **Step 4: Update the observer unit test**

In `stats_observer_records_dice_and_final_view`, replace:

```rust
let (mut observer, handle) = RunStatsObserver::new();
```

with:

```rust
let mut observer = RunStatsObserver::new();
```

and replace:

```rust
let summary = handle.summary();
```

with:

```rust
let summary = observer.summary();
```

Then replace the consuming unwrap assertion:

```rust
assert!(summary.final_view.unwrap().snapshot_state.is_none());
```

with the borrowed form:

```rust
assert!(summary
    .final_view
    .as_ref()
    .expect("final view should be captured")
    .snapshot_state
    .is_none());
```

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test -p catan-runtime run_stats
```

Expected: the `run_stats` tests pass after all handle references in this file are removed.

- [ ] **Step 6: Commit**

```bash
git add catan-runtime/src/run_stats.rs
git commit -m "refactor: store run stats directly in observer"
```

## Task 2: Migrate `SimulationHost` to Borrowed Observers

**Files:**
- Modify: `catan-runtime/src/simulation.rs`
- Modify: `catan-runtime/src/bin/catan-bench.rs`

- [ ] **Step 1: Remove observer ownership from `SimulationHost`**

In `catan-runtime/src/simulation.rs`, remove the `observers` field:

```rust
observers: Vec<Box<dyn OutputObserver>>,
```

Remove this initialization from `SimulationHost::new`:

```rust
observers: Vec::new(),
```

Delete the `add_observer` method:

```rust
pub fn add_observer(&mut self, observer: Box<dyn OutputObserver>) {
    self.observers.push(observer);
}
```

- [ ] **Step 2: Add observer-free and observed run entry points**

Replace the existing `run` method signature:

```rust
pub fn run(&mut self) -> GameResult {
```

with these two methods:

```rust
pub fn run(&mut self) -> GameResult {
    let mut observers: [&mut dyn OutputObserver; 0] = [];
    self.run_observed(&mut observers)
}

pub fn run_observed(&mut self, observers: &mut [&mut dyn OutputObserver]) -> GameResult {
```

Move the existing run body into `run_observed`.

- [ ] **Step 3: Thread observers through simulation delivery**

Inside `run_observed`, replace:

```rust
self.project_and_deliver(&transaction, &mut queue);
```

with:

```rust
self.project_and_deliver(&transaction, &mut queue, observers);
```

and replace the submit path call with the same observer argument:

```rust
self.project_and_deliver(&transaction, &mut queue, observers);
```

Change `project_and_deliver` to:

```rust
fn project_and_deliver(
    &mut self,
    transaction: &catan_core::gameplay::game::event::EventTransaction,
    queue: &mut VecDeque<GameOutput>,
    observers: &mut [&mut dyn OutputObserver],
) {
    for output in projector::project_transaction(transaction) {
        self.deliver_output(&output, observers);
        queue.push_back(output);
    }
}
```

Change `deliver_output` to:

```rust
fn deliver_output(&mut self, output: &GameOutput, observers: &mut [&mut dyn OutputObserver]) {
    let factory = ContextFactory {
        state: self.engine.table(),
        index: self.engine.index(),
        visibility: &self.visibility,
        trade_sessions: self.engine.trade_sessions(),
    };
    for observer in observers.iter_mut() {
        observer.on_output(ObserverFrame {
            output,
            factory: &factory,
            engine: &self.engine,
        });
    }
}
```

The loop constructs a fresh `ObserverFrame` for each observer because `ObserverFrame` is moved into `on_output`.

- [ ] **Step 4: Update the simulation test**

In `lazy_bots_reach_turn_limit`, replace:

```rust
let (stats_observer, stats_handle) = crate::run_stats::RunStatsObserver::new();
host.add_observer(Box::new(stats_observer));

assert!(matches!(host.run(), GameResult::LimitReached { turns: 3 }));
let stats = stats_handle.stats();
```

with:

```rust
let mut stats_observer = crate::run_stats::RunStatsObserver::new();
let mut observers: [&mut dyn OutputObserver; 1] = [&mut stats_observer];

assert!(matches!(
    host.run_observed(&mut observers),
    GameResult::LimitReached { turns: 3 }
));
let stats = stats_observer.stats();
```

- [ ] **Step 5: Update benchmark run stats wiring**

In `catan-runtime/src/bin/catan-bench.rs`, make `agents` mutable:

```rust
let agents = build_agents(&config.players, seed);
```

can stay unchanged because this task does not migrate bots. Replace the observer wiring:

```rust
let (stats_observer, stats_handle) = RunStatsObserver::new();
host.add_observer(Box::new(stats_observer));
let result = host.run();

Ok(GameOutcome {
    result,
    stats: stats_handle.stats(),
})
```

with:

```rust
let mut stats_observer = RunStatsObserver::new();
let mut observers: [&mut dyn catan_runtime::sync_host::OutputObserver; 1] = [&mut stats_observer];
let result = host.run_observed(&mut observers);

Ok(GameOutcome {
    result,
    stats: stats_observer.stats(),
})
```

If the fully-qualified trait type is noisy, add this import near the existing runtime imports:

```rust
use catan_runtime::sync_host::OutputObserver;
```

and use:

```rust
let mut observers: [&mut dyn OutputObserver; 1] = [&mut stats_observer];
```

- [ ] **Step 6: Run focused simulation and benchmark compilation tests**

Run:

```bash
cargo test -p catan-runtime simulation
cargo test -p catan-runtime --bin catan-bench
```

Expected: the simulation test passes and `catan-bench` compiles with borrowed stats observer wiring.

- [ ] **Step 7: Commit**

```bash
git add catan-runtime/src/simulation.rs catan-runtime/src/bin/catan-bench.rs
git commit -m "refactor: borrow simulation observers during runs"
```

## Task 3: Migrate `SyncGameHost` to Borrowed Observers

**Files:**
- Modify: `catan-runtime/src/sync_host.rs`
- Modify: `catan-runtime/src/host.rs`

- [ ] **Step 1: Remove observer ownership from `SyncGameHost`**

In `catan-runtime/src/sync_host.rs`, remove the `observers` field:

```rust
observers: Vec<Box<dyn OutputObserver>>,
```

Remove this initialization from `from_engine`:

```rust
observers: Vec::new(),
```

Delete the `add_observer` method:

```rust
pub fn add_observer(&mut self, observer: Box<dyn OutputObserver>) {
    self.observers.push(observer);
}
```

- [ ] **Step 2: Add observer-free and observed run methods**

Replace the existing `run_until_waiting` and `run_to_result` methods with:

```rust
pub fn run_until_waiting(&mut self) -> Option<GameResult> {
    let mut observers: [&mut dyn OutputObserver; 0] = [];
    self.run_until_waiting_observed(&mut observers)
}

pub fn run_until_waiting_observed(
    &mut self,
    observers: &mut [&mut dyn OutputObserver],
) -> Option<GameResult> {
    loop {
        if let Some(input) = self.inputs.pop_front() {
            let transaction = self
                .engine
                .submit(input.response)
                .expect("engine submit should reduce");
            self.prepend_outputs(projector::project_transaction(&transaction));
            self.enqueue_rejected_pending_decisions(&transaction);
            continue;
        }
        if let Some(output) = self.outputs.pop_front() {
            self.deliver_output(&output, observers);
            continue;
        }
        if let Some(result) = self.engine.result().cloned() {
            return Some(result);
        }
        return None;
    }
}

pub fn run_to_result(&mut self) -> GameResult {
    let mut observers: [&mut dyn OutputObserver; 0] = [];
    self.run_to_result_observed(&mut observers)
}

pub fn run_to_result_observed(
    &mut self,
    observers: &mut [&mut dyn OutputObserver],
) -> GameResult {
    self.run_until_waiting_observed(observers)
        .unwrap_or_else(|| GameResult::Interrupted {
            reason: "host is waiting for external input".to_owned(),
        })
}
```

This keeps existing observer-free callers compiling while making observed runs explicit.

- [ ] **Step 3: Change `deliver_output` to accept borrowed observers**

Replace:

```rust
fn deliver_output(&mut self, output: &GameOutput) {
```

with:

```rust
fn deliver_output(&mut self, output: &GameOutput, observers: &mut [&mut dyn OutputObserver]) {
```

Replace the observer loop:

```rust
for observer in &mut self.observers {
    observer.on_output(ObserverFrame {
        output,
        factory: &factory,
        engine: &self.engine,
    });
}
```

with:

```rust
for observer in observers.iter_mut() {
    observer.on_output(ObserverFrame {
        output,
        factory: &factory,
        engine: &self.engine,
    });
}
```

Keep the seat delivery loop unchanged in this task.

- [ ] **Step 4: Update persistence wiring in `run_match`**

In `catan-runtime/src/host.rs`, replace:

```rust
let mut host = SyncGameHost::from_engine(engine, seats);
if !config.observers.is_empty() {
    log::debug!("runtime-local observers are not wired to visible output yet");
}
if let Some(observer) = PersistenceObserver::from_config(&config.persistence)
    .map_err(|err| format!("failed to initialize persistence: {err}"))?
{
    host.add_observer(Box::new(observer));
}
host.start();
let result = host.run_to_result();
```

with:

```rust
let mut host = SyncGameHost::from_engine(engine, seats);
if !config.observers.is_empty() {
    log::debug!("runtime-local observers are not wired to visible output yet");
}
let mut persistence_observer = PersistenceObserver::from_config(&config.persistence)
    .map_err(|err| format!("failed to initialize persistence: {err}"))?;
let mut observers: Vec<&mut dyn crate::sync_host::OutputObserver> = Vec::new();
if let Some(observer) = persistence_observer.as_mut() {
    observers.push(observer);
}
host.start();
let result = host.run_to_result_observed(&mut observers);
```

This keeps the persistence observer alive in caller scope for the full run.

- [ ] **Step 5: Run focused sync host tests to expose call-site failures**

Run:

```bash
cargo test -p catan-runtime sync_host
```

Expected on the first run: compile errors only at tests that still call `add_observer` or use `RunStatsObserver::new()` as a tuple. Proceed to Task 4 to update those tests.

- [ ] **Step 6: Commit after tests are updated in Task 4**

Do not commit this task alone if the crate does not compile. Commit together with Task 4 after all sync host tests are migrated.

## Task 4: Update Sync Host Observer Tests to Use Direct Borrowing

**Files:**
- Modify: `catan-runtime/src/sync_host.rs`

- [ ] **Step 1: Update `bot_seat_responds_immediately`**

Replace:

```rust
let (stats_observer, stats_handle) = crate::run_stats::RunStatsObserver::new();
host.add_observer(Box::new(stats_observer));

host.start();
assert!(matches!(
    host.run_to_result(),
    GameResult::LimitReached { turns: 1 }
));
let stats = stats_handle.stats();
```

with:

```rust
let mut stats_observer = crate::run_stats::RunStatsObserver::new();
let mut observers: [&mut dyn OutputObserver; 1] = [&mut stats_observer];

host.start();
assert!(matches!(
    host.run_to_result_observed(&mut observers),
    GameResult::LimitReached { turns: 1 }
));
let stats = stats_observer.stats();
```

- [ ] **Step 2: Simplify `CountingObserver`**

Replace:

```rust
struct CountingObserver {
    outputs: std::rc::Rc<std::cell::Cell<usize>>,
}

impl OutputObserver for CountingObserver {
    fn on_output(&mut self, _frame: ObserverFrame<'_>) {
        self.outputs.set(self.outputs.get() + 1);
    }
}
```

with:

```rust
#[derive(Default)]
struct CountingObserver {
    outputs: usize,
}

impl OutputObserver for CountingObserver {
    fn on_output(&mut self, _frame: ObserverFrame<'_>) {
        self.outputs += 1;
    }
}
```

In `output_observer_receives_engine_outputs`, replace:

```rust
let output_count = std::rc::Rc::new(std::cell::Cell::new(0));
let observer = Box::new(CountingObserver {
    outputs: output_count.clone(),
});
```

with:

```rust
let mut observer = CountingObserver::default();
let mut observers: [&mut dyn OutputObserver; 1] = [&mut observer];
```

Replace:

```rust
host.add_observer(observer);
host.start();
let _ = host.run_until_waiting();

assert!(output_count.get() > 0);
```

with:

```rust
host.start();
let _ = host.run_until_waiting_observed(&mut observers);

assert!(observer.outputs > 0);
```

- [ ] **Step 3: Simplify `EventRecordingObserver`**

Replace:

```rust
struct EventRecordingObserver {
    events: std::rc::Rc<std::cell::RefCell<Vec<GameEvent>>>,
}

impl OutputObserver for EventRecordingObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        if let GameOutput::Event(record) = frame.output {
            self.events.borrow_mut().push(record.event.clone());
        }
    }
}
```

with:

```rust
#[derive(Default)]
struct EventRecordingObserver {
    events: Vec<GameEvent>,
}

impl OutputObserver for EventRecordingObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        if let GameOutput::Event(record) = frame.output {
            self.events.push(record.event.clone());
        }
    }
}
```

- [ ] **Step 4: Update event-recording test wiring**

In `greedy_bots_emit_player_trade_events`, replace the `Rc<RefCell<_>>` setup and observer ownership:

```rust
let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
let observer = Box::new(EventRecordingObserver {
    events: events.clone(),
});
```

with:

```rust
let mut observer = EventRecordingObserver::default();
let mut observers: [&mut dyn OutputObserver; 1] = [&mut observer];
```

Replace:

```rust
host.add_observer(observer);
host.start();
let _ = host.run_until_waiting();

let events = events.borrow();
```

with:

```rust
host.start();
let _ = host.run_until_waiting_observed(&mut observers);

let events = &observer.events;
```

In `terminal_outputs_are_delivered_before_host_returns_result`, make the same replacement and call:

```rust
let _ = host.run_to_result_observed(&mut observers);
```

Then replace:

```rust
assert!(events.borrow().iter().any(|event| matches!(
```

with:

```rust
assert!(observer.events.iter().any(|event| matches!(
```

- [ ] **Step 5: Run focused sync host tests**

Run:

```bash
cargo test -p catan-runtime sync_host
```

Expected: all `sync_host` tests pass.

- [ ] **Step 6: Commit Tasks 3 and 4 together**

```bash
git add catan-runtime/src/sync_host.rs catan-runtime/src/host.rs
git commit -m "refactor: borrow sync host observers during runs"
```

## Task 5: Remove Remaining Handle and Owned-Observer References

**Files:**
- Modify any file reported by the search commands below.

- [ ] **Step 1: Search for removed APIs**

Run:

```bash
rg -n "RunStatsHandle|RunStatsObserver::new\\(\\).*handle|add_observer|run_observed|run_to_result_observed|run_until_waiting_observed" catan-runtime
```

Expected:
- No `RunStatsHandle`.
- No `add_observer`.
- `run_observed`, `run_to_result_observed`, and `run_until_waiting_observed` appear only in method definitions and call sites that intentionally pass borrowed observers.

- [ ] **Step 2: Search for leftover observer `Rc<RefCell<_>>` test scaffolding**

Run:

```bash
rg -n "Rc<.*RefCell|RefCell<.*Vec<GameEvent>|Cell<usize>|stats_handle|handle\\.summary|handle\\.stats" catan-runtime/src
```

Expected:
- No stats handle references.
- No observer-only `Rc<RefCell<_>>` scaffolding remains.
- Existing unrelated seat tests may still use `Rc<Cell<_>>`; leave those unchanged because this plan does not migrate seats.

- [ ] **Step 3: Run the runtime crate test suite**

Run:

```bash
cargo test -p catan-runtime
```

Expected: all `catan-runtime` tests pass.

- [ ] **Step 4: Commit cleanup**

If Step 1 or Step 2 required additional cleanup, commit it:

```bash
git add catan-runtime
git commit -m "chore: remove obsolete observer handle references"
```

If no files changed, skip this commit.

## Task 6: Full Verification

**Files:**
- No planned source edits.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt --all -- --check
```

Expected: no formatting diffs.

If formatting fails, run:

```bash
cargo fmt --all
```

then rerun:

```bash
cargo fmt --all -- --check
```

- [ ] **Step 2: Run workspace tests**

Run:

```bash
cargo test --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 3: Run clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no clippy warnings.

- [ ] **Step 4: Final API search**

Run:

```bash
rg -n "add_observer|RunStatsHandle|Rc<RefCell<GameSummary>>|stats_handle" catan-runtime
```

Expected: no matches.

- [ ] **Step 5: Commit verification fixes**

If formatting or clippy required source changes, commit them:

```bash
git add .
git commit -m "chore: verify borrowed observer migration"
```

If no files changed, skip this commit.

## Follow-Up Plan: Borrow Seats and Bots

After this observer migration lands, create a separate plan for borrowed players:

- Change `SimulationHost` from owning `Vec<Box<dyn BotPolicy>>` to accepting `&mut [&mut dyn BotPolicy]` in `run_observed`.
- Change `SyncGameHost` from owning `Vec<Box<dyn Seat>>` to accepting `&mut [&mut dyn Seat]` in `run_until_waiting_observed`.
- Keep observer and seat borrows as separate arguments to avoid combined-participant aliasing.
- Add adapter helpers at config boundaries where JSON-created bots/seats still naturally live as `Vec<Box<dyn Trait>>`.

Keeping this as a separate migration makes borrow errors easier to reason about and prevents observer cleanup from being tangled with player ownership changes.

## Self-Review

- Spec coverage: the plan removes observer ownership from both runtime hosts, removes `RunStatsObserver` internal mutability, updates benchmark and persistence wiring, and verifies removed APIs.
- Placeholder scan: no placeholder markers or unspecified implementation steps remain.
- Type consistency: observed methods use `&mut [&mut dyn OutputObserver]`; observer-free methods preserve existing `run`, `run_until_waiting`, and `run_to_result` call sites by constructing an empty mutable observer slice internally.
