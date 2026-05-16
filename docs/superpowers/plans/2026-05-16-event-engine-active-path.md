# Event Engine Active Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the live engine toward the reducer-driven active path by making committed transactions project outputs and update an `EngineLifecycle` mirror, then migrate the `Start` command onto that path.

**Architecture:** `GameEngine` keeps the existing imperative fields during this batch, but gains an `EngineLifecycle` mirror that is updated only through `reducer::reduce`. `projector` becomes the single conversion point from committed event transactions to runtime outputs, including compatibility outputs such as `DecisionOpened`. The `Start` command becomes the first live command to use `decider -> reducer -> projector`.

**Tech Stack:** Rust, `smallvec`, serde, existing `catan-core` game modules, cargo tests.

---

## File Structure

- Modify `catan-core/src/gameplay/game/projector.rs`: project transactions to both event records and compatibility `GameOutput` variants.
- Modify `catan-core/src/gameplay/game/engine.rs`: add `EngineLifecycle`, committed transaction application, and migrate `start` for initialized games.
- Modify `catan-core/src/gameplay/game/reducer.rs`: keep lifecycle phase consistent for decision events.
- Modify `catan-core/src/gameplay/game/lifecycle.rs`: expose active state needed by the engine sync step.
- Modify `catan-core/src/gameplay/game/engine/tests.rs`: add tests for transaction projection and live start path.
- Modify `docs/architecture/event-engine-active-path.md`: update the active-path note after this batch.

## Task 1: Project Compatibility Outputs From Transactions

**Files:**
- Modify: `catan-core/src/gameplay/game/projector.rs`
- Test: `catan-core/src/gameplay/game/projector.rs`

- [ ] **Step 1: Add failing projector test**

Add this test to `projector.rs`:

```rust
#[test]
fn transaction_projection_includes_compatibility_decision_output() {
    use crate::gameplay::game::decision::{
        DecisionId, DecisionKind, DecisionLifetime, OpenDecision,
    };

    let mut transaction = EventTransaction::new(12, EventCause::Start);
    transaction.events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(3),
        player_id: 1,
        kind: DecisionKind::InitPlacement,
        lifetime: DecisionLifetime::OneShot,
    }));

    let outputs = project_transaction(&transaction);

    assert!(matches!(outputs.as_slice(), [
        GameOutput::Event(record),
        GameOutput::DecisionOpened(decision),
    ] if record.tx_id == 12 && decision.id == DecisionId(3)));
}
```

- [ ] **Step 2: Verify red**

Run:

```bash
cargo test -q -p catan-core transaction_projection_includes_compatibility_decision_output
```

Expected: FAIL because `project_transaction` only emits `GameOutput::Event`.

- [ ] **Step 3: Implement compatibility projection**

Update `project_transaction` so each event always projects to `GameOutput::Event`, and also projects:

```rust
GameEvent::DecisionOpened(decision) => GameOutput::DecisionOpened(decision.clone())
GameEvent::DecisionClosed { decision_id } => GameOutput::DecisionClosed { decision_id: *decision_id }
GameEvent::CommandRejected { player_id, decision_id, reason, .. } => GameOutput::CommandRejected {
    player_id: *player_id,
    decision_id: *decision_id,
    reason: reason.clone(),
}
```

- [ ] **Step 4: Verify green**

Run:

```bash
cargo test -q -p catan-core transaction_projection_includes_compatibility_decision_output
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add catan-core/src/gameplay/game/projector.rs
git commit -m "refactor: project transaction compatibility outputs"
```

## Task 2: Add Reducer-Updated Lifecycle Mirror To GameEngine

**Files:**
- Modify: `catan-core/src/gameplay/game/engine.rs`
- Modify: `catan-core/src/gameplay/game/lifecycle.rs`
- Test: `catan-core/src/gameplay/game/engine/tests.rs`

- [ ] **Step 1: Add failing lifecycle mirror test**

Add this test to `engine/tests.rs`:

```rust
#[test]
fn start_updates_reducer_lifecycle_mirror() {
    let (engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);

    let active = engine
        .lifecycle()
        .as_active()
        .expect("started engine should have active lifecycle");

    assert!(matches!(active.phase, GamePhase::InitialPlacement));
    assert!(active.pending.get(decision.id).is_some());
    assert_eq!(active.next_decision_id, decision.id.0 + 1);
}
```

- [ ] **Step 2: Verify red**

Run:

```bash
cargo test -q -p catan-core start_updates_reducer_lifecycle_mirror
```

Expected: FAIL because `GameEngine` has no public lifecycle mirror accessor and start does not update lifecycle.

- [ ] **Step 3: Add lifecycle field and sync method**

Add `lifecycle: EngineLifecycle` to `GameEngine`.

Initialize it in `new_with_options`:

```rust
lifecycle: EngineLifecycle::active(game.clone()),
```

Initialize it in `from_snapshot` from the loaded state:

```rust
lifecycle: EngineLifecycle::from_snapshot_parts(
    game.clone(),
    snapshot.phase,
    snapshot.pending.clone(),
    snapshot.next_decision_id,
    snapshot.trade_sessions.clone(),
    snapshot.stats,
    snapshot.invalid_actions,
    snapshot.pending_discards.clone(),
    snapshot.result.clone(),
),
```

Add:

```rust
pub fn lifecycle(&self) -> &EngineLifecycle {
    &self.lifecycle
}
```

Add a private `sync_from_lifecycle` method that clones active lifecycle state back into the legacy fields. For `Finished`, sync `game`, `index`, `result`, and set phase to `GamePhase::Ended`.

- [ ] **Step 4: Add lifecycle constructor**

Add this associated function to `EngineLifecycle`:

```rust
pub fn from_snapshot_parts(
    game: GameState,
    phase: GamePhase,
    pending: PendingDecisions,
    next_decision_id: u64,
    trade_sessions: TradeSessions,
    stats: GameRunStats,
    invalid_actions: u64,
    pending_discards: PendingDiscards,
    result: Option<GameResult>,
) -> Self
```

If `result` is `Some`, return `EngineLifecycle::Finished`; otherwise return `EngineLifecycle::Active`.

- [ ] **Step 5: Verify green**

Run:

```bash
cargo test -q -p catan-core start_updates_reducer_lifecycle_mirror
```

Expected: PASS after Task 3 also migrates start.

## Task 3: Migrate Initialized Start To Decider/Reducer/Projector

**Files:**
- Modify: `catan-core/src/gameplay/game/engine.rs`
- Modify: `catan-core/src/gameplay/game/reducer.rs`
- Test: `catan-core/src/gameplay/game/engine/tests.rs`

- [ ] **Step 1: Add failing transaction-id test**

Add this test to `engine/tests.rs`:

```rust
#[test]
fn start_outputs_share_one_transaction_id() {
    let (_engine, outputs) = started_engine();
    let tx_ids = outputs
        .iter()
        .filter_map(|output| match output {
            GameOutput::Event(record) => Some(record.tx_id),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(tx_ids.len(), 1);
    assert_eq!(tx_ids.first().copied(), Some(1));
}
```

- [ ] **Step 2: Verify red**

Run:

```bash
cargo test -q -p catan-core start_outputs_share_one_transaction_id
```

Expected: FAIL if legacy start emits separate transaction ids or does not use transaction projection consistently.

- [ ] **Step 3: Implement initialized start transaction path**

In `GameEngine::start`, when `self.init.is_some()`:

```rust
let tx_id = self.begin_transaction(EventCause::Start);
let mut transaction = EventTransaction::new(tx_id, EventCause::Start);
transaction.events = decider::decide(&self.lifecycle, GameInput::Start);
for event in &transaction.events {
    reducer::reduce(&mut self.lifecycle, event).expect("start events should reduce");
    self.record_event(event);
}
self.sync_from_lifecycle();
for output in projector::project_transaction(&transaction) {
    sink.push(output);
}
return GameStatus::Waiting;
```

Leave the `init.is_none()` branch on the legacy `start_turn` path for this batch.

- [ ] **Step 4: Keep reducer phase consistent**

In `reducer::reduce`, update `DecisionOpened` handling:

```rust
if matches!(decision.kind, DecisionKind::InitPlacement) {
    active.phase = GamePhase::InitialPlacement;
}
```

- [ ] **Step 5: Verify green**

Run:

```bash
cargo test -q -p catan-core start_outputs_share_one_transaction_id start_updates_reducer_lifecycle_mirror
```

Expected: both tests PASS.

- [ ] **Step 6: Run full tests and commit**

Run:

```bash
cargo fmt
cargo test -q
git add catan-core/src/gameplay/game/engine.rs catan-core/src/gameplay/game/reducer.rs catan-core/src/gameplay/game/lifecycle.rs catan-core/src/gameplay/game/engine/tests.rs
git commit -m "feat: route game start through reducer"
```

## Task 4: Update Active Path Documentation

**Files:**
- Modify: `docs/architecture/event-engine-active-path.md`

- [ ] **Step 1: Update active path note**

Change `Active path` to state:

```markdown
`SyncGameHost -> GameEngine::start -> EngineLifecycle -> decider -> reducer -> projector -> runtime observers`

The initialized `Start` command is reducer-driven. Other command groups still use legacy imperative helpers while emitting committed event records.
```

Keep `Legacy path` and list:

```markdown
Submit command groups still mutate through legacy `GameEngine` helpers. Remaining groups are initial placement, turn lifecycle, regular actions, dev cards, robber/discard flow, and player trades.
```

- [ ] **Step 2: Verify docs and commit**

Run:

```bash
git diff -- docs/architecture/event-engine-active-path.md
git add docs/architecture/event-engine-active-path.md
git commit -m "docs: track reducer start active path"
```

## Task 5: Final Verification For This Batch

**Files:**
- No code edits unless verification reveals a defect.

- [ ] **Step 1: Run full verification**

Run:

```bash
cargo test -q
cargo run -q -p catan-runtime --bin catan-bench -- --games 1 --no-log --json-summary
git status --short
```

Expected:
- All tests pass.
- Benchmark prints one-game JSON summary.
- Git status is clean.

