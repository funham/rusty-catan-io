# Core Hot Loop Allocation Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drive heap allocation in the main simulation loop toward zero and reduce repeated core legal-query work exposed by trading and greedy-style build scoring.

**Architecture:** Add measurement first, then remove obvious core heap allocations from projection, then move board topology and build-legality caches into `GameIndex`/`FieldIndex`. Keep rule enforcement centralized in core state/legal logic; bots should benefit through cheaper core queries without bot-specific algorithms.

**Tech Stack:** Rust 1.95, `smallvec`, existing `SmallSet`, `catan-core`, `catan-runtime`, `catan-bench`, `hyperfine`, macOS `sample`.

---

## Current Evidence

The current hot scenario is:

```sh
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --no-log
```

Observed profile evidence:

- `road_extension_candidates_with_extra_roads` is the dominant core function in sampled stacks.
- Allocator frames (`_xzm_xzone_malloc_tiny`, `_malloc_zone_malloc`, `_realloc`, `_xzm_free`, `_free`) appear under `Vec::from_iter`, `RawVec::finish_grow`, `project_transaction`, and greedy/legal collection paths.
- `GameIndex` currently caches awards, ports, and a duplicate `all_builds`, but not the core data used by legal build searches.
- `FieldIndex` stores board paths as a `Vec<Path>` but not as a membership set or incident-path lookup.
- `GameOutput::Event(Box<GameEventRecord>)` guarantees a heap allocation per event output.
- `project_transaction` returns `Vec<GameOutput>` in the runtime loop.

The plan below treats heap allocation in core hot paths as a correctness/performance constraint, not a micro-optimization.

## File Structure

Files to modify:

- `Cargo.toml`
  - Add workspace feature plumbing only if needed by package features.
- `catan-runtime/Cargo.toml`
  - Add `bench-allocs` feature for benchmark allocation accounting.
- `catan-runtime/src/bin/catan-bench.rs`
  - Add allocation counters, reset/snapshot around the simulation loop, and emit metrics in JSON/human summaries.
- `catan-core/src/gameplay/game/io/output.rs`
  - Remove `Box<GameEventRecord>` from `GameOutput::Event`.
  - Add an inline output batch alias.
- `catan-core/src/gameplay/game/io/projector.rs`
  - Replace `Vec<GameOutput>` transaction projection with `SmallVec` or visitor-based projection.
- `catan-runtime/src/simulation.rs`
  - Consume projected outputs without heap-allocating a `Vec`.
- `catan-core/src/gameplay/field/index.rs`
  - Add immutable board topology caches: path membership and incident paths.
- `catan-core/src/gameplay/field/state.rs`
  - Expose cached topology data through `BoardLayout`.
- `catan-core/src/gameplay/game/index.rs`
  - Add mutable build-legality cache fields and incremental refresh.
- `catan-core/src/gameplay/game/view.rs`
  - Expose the relevant index-backed query surface to legal functions.
- `catan-core/src/gameplay/game/legal.rs`
  - Route build count/list queries through index-backed cached primitives.
  - Remove boxed iterators from hot legal APIs.
- `catan-core/src/gameplay/primitives/build.rs`
  - Keep validation APIs as source of truth, but add allocation-free candidate helpers that accept precomputed topology/index data.
- `catan-bots/src/trade.rs`
  - Replace temporary `Vec<PlayerTrade>` with an inline iterator or `SmallVec`.
- `catan-bots/src/greedy.rs`
  - Consume iterator/smallvec legal APIs without changing strategy semantics.
- `docs/performance-baseline.md`
  - Document allocation counters and the benchmark command.

Files to create:

- `catan-core/src/gameplay/game/legal/cache.rs`
  - Optional if `legal.rs` grows too large during implementation. Defines small cached legal-summary types if introduced.

Do not split files preemptively. Create `legal/cache.rs` only if `legal.rs` becomes harder to read after Task 5.

---

## Task 1: Add Allocation Measurement to `catan-bench`

**Files:**

- Modify: `catan-runtime/Cargo.toml`
- Modify: `catan-runtime/src/bin/catan-bench.rs`
- Modify: `docs/performance-baseline.md`

### Purpose

Add objective heap allocation counters for the benchmark loop. This prevents relying on sampled allocator frames alone and gives every later task a pass/fail metric.

### Implementation Steps

- [ ] **Step 1: Add the benchmark feature**

In `catan-runtime/Cargo.toml`, change:

```toml
[features]
bench-counters = ["catan-core/bench-counters"]
```

to:

```toml
[features]
bench-counters = ["catan-core/bench-counters"]
bench-allocs = []
```

- [ ] **Step 2: Add allocation counter types**

In `catan-runtime/src/bin/catan-bench.rs`, near the imports, add:

```rust
#[cfg(feature = "bench-allocs")]
mod alloc_counters {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[derive(Debug, Clone, Copy, serde::Serialize)]
    pub struct AllocationSummary {
        pub alloc_calls: u64,
        pub dealloc_calls: u64,
        pub realloc_calls: u64,
        pub alloc_bytes: u64,
        pub dealloc_bytes: u64,
        pub realloc_new_bytes: u64,
    }

    pub struct CountingAllocator;

    static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static REALLOC_NEW_BYTES: AtomicU64 = AtomicU64::new(0);

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            REALLOC_NEW_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    pub fn reset() {
        ALLOC_CALLS.store(0, Ordering::Relaxed);
        DEALLOC_CALLS.store(0, Ordering::Relaxed);
        REALLOC_CALLS.store(0, Ordering::Relaxed);
        ALLOC_BYTES.store(0, Ordering::Relaxed);
        DEALLOC_BYTES.store(0, Ordering::Relaxed);
        REALLOC_NEW_BYTES.store(0, Ordering::Relaxed);
    }

    pub fn snapshot() -> AllocationSummary {
        AllocationSummary {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
            realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            dealloc_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
            realloc_new_bytes: REALLOC_NEW_BYTES.load(Ordering::Relaxed),
        }
    }
}

#[cfg(feature = "bench-allocs")]
#[global_allocator]
static GLOBAL_ALLOCATOR: alloc_counters::CountingAllocator = alloc_counters::CountingAllocator;
```

- [ ] **Step 3: Add summary fields**

In `Environment`, add:

```rust
bench_allocs_enabled: bool,
```

In `Summary`, add:

```rust
allocations: Option<AllocationSummary>,
```

Use the cfg-safe type alias near the summary structs:

```rust
#[cfg(feature = "bench-allocs")]
use alloc_counters::AllocationSummary;

#[cfg(not(feature = "bench-allocs"))]
type AllocationSummary = ();
```

This makes `Option<AllocationSummary>` compile in both feature modes.

- [ ] **Step 4: Reset counters after setup and before the game loop**

In `run()`, after config loading/validation and bench legal counter reset, add:

```rust
#[cfg(feature = "bench-allocs")]
alloc_counters::reset();
```

This intentionally excludes argument parsing, config parsing, logger setup, and build startup from the allocation metric.

- [ ] **Step 5: Snapshot counters when building the summary**

In `build_summary`, set:

```rust
allocations: allocation_summary(),
```

Add helper:

```rust
fn allocation_summary() -> Option<AllocationSummary> {
    #[cfg(feature = "bench-allocs")]
    {
        Some(alloc_counters::snapshot())
    }

    #[cfg(not(feature = "bench-allocs"))]
    {
        None
    }
}
```

Set environment field:

```rust
bench_allocs_enabled: cfg!(feature = "bench-allocs"),
```

- [ ] **Step 6: Print human allocation summary**

In `print_human_summary`, after legal counters output, add:

```rust
if let Some(allocations) = &summary.allocations {
    println!(
        "allocations: alloc_calls={} realloc_calls={} dealloc_calls={} alloc_bytes={} realloc_new_bytes={} dealloc_bytes={}",
        allocations.alloc_calls,
        allocations.realloc_calls,
        allocations.dealloc_calls,
        allocations.alloc_bytes,
        allocations.realloc_new_bytes,
        allocations.dealloc_bytes
    );
} else {
    println!("allocations: disabled; rebuild with --features bench-allocs");
}
```

- [ ] **Step 7: Verify build and JSON output**

Run:

```sh
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 5 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log | jq '.environment.bench_allocs_enabled, .allocations'
```

Expected:

```text
true
{
  "alloc_calls": <nonzero integer>,
  "dealloc_calls": <nonzero integer>,
  "realloc_calls": <integer>,
  "alloc_bytes": <nonzero integer>,
  "dealloc_bytes": <integer>,
  "realloc_new_bytes": <integer>
}
```

- [ ] **Step 8: Document the allocation benchmark**

In `docs/performance-baseline.md`, add:

```markdown
For allocation accounting in the benchmark loop:

```sh
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log | jq '.allocations'
```

The counter resets after argument/config setup and before the simulated game loop.
```

- [ ] **Step 9: Commit**

```sh
git add catan-runtime/Cargo.toml catan-runtime/src/bin/catan-bench.rs docs/performance-baseline.md
git commit -m "perf: add bench allocation counters"
```

---

## Task 2: Remove Heap Allocation From Runtime Output Projection

**Files:**

- Modify: `catan-core/src/gameplay/game/io/output.rs`
- Modify: `catan-core/src/gameplay/game/io/projector.rs`
- Modify: `catan-runtime/src/simulation.rs`
- Modify: tests in `catan-core/src/gameplay/game/io/projector.rs`

### Purpose

Remove guaranteed heap allocations from every projected event:

- `GameOutput::Event(Box<GameEventRecord>)`
- `project_transaction() -> Vec<GameOutput>`

### Implementation Steps

- [ ] **Step 1: Change `GameOutput::Event` to store inline records**

In `catan-core/src/gameplay/game/io/output.rs`, change:

```rust
Event(Box<GameEventRecord>),
```

to:

```rust
Event(GameEventRecord),
```

Change:

```rust
pub fn event(record: GameEventRecord) -> Self {
    Self::Event(Box::new(record))
}
```

to:

```rust
pub fn event(record: GameEventRecord) -> Self {
    Self::Event(record)
}
```

- [ ] **Step 2: Add inline output batch alias**

In `output.rs`, add:

```rust
use smallvec::SmallVec;

use crate::gameplay::constants::capacities::EVENT_BATCH_INLINE;

pub type GameOutputBatch = SmallVec<[GameOutput; EVENT_BATCH_INLINE]>;
```

If the import path for capacities is rejected, use:

```rust
use crate::constants::capacities::EVENT_BATCH_INLINE;
```

based on the local module path from the compiler error.

- [ ] **Step 3: Replace vector projection**

In `projector.rs`, change imports:

```rust
output::{GameEventRecord, GameOutput},
```

to:

```rust
output::{GameEventRecord, GameOutput, GameOutputBatch},
```

Replace:

```rust
pub fn project_transaction(transaction: &EventTransaction) -> Vec<GameOutput> {
    let mut outputs = Vec::new();
    for event in &transaction.events {
        outputs.push(project_event(event.clone()));
        match event {
            GameEvent::DecisionOpened(decision) => {
                outputs.push(GameOutput::DecisionOpened(
                    DecisionRequest::from_open_decision(decision),
                ));
            }
            GameEvent::DecisionClosed { decision_id } => {
                outputs.push(GameOutput::DecisionClosed {
                    decision_id: *decision_id,
                });
            }
            GameEvent::CommandRejected { token, reason, .. } => {
                outputs.push(GameOutput::CommandRejected {
                    token: *token,
                    reason: reason.clone(),
                });
            }
            _ => {}
        }
    }
    outputs
}
```

with:

```rust
pub fn project_transaction(transaction: &EventTransaction) -> GameOutputBatch {
    let mut outputs = GameOutputBatch::new();
    project_transaction_into(transaction, |output| outputs.push(output));
    outputs
}

pub fn project_transaction_into(
    transaction: &EventTransaction,
    mut emit: impl FnMut(GameOutput),
) {
    for event in &transaction.events {
        emit(project_event(event.clone()));
        match event {
            GameEvent::DecisionOpened(decision) => {
                emit(GameOutput::DecisionOpened(
                    DecisionRequest::from_open_decision(decision),
                ));
            }
            GameEvent::DecisionClosed { decision_id } => {
                emit(GameOutput::DecisionClosed {
                    decision_id: *decision_id,
                });
            }
            GameEvent::CommandRejected { token, reason, .. } => {
                emit(GameOutput::CommandRejected {
                    token: *token,
                    reason: reason.clone(),
                });
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 4: Update simulation to use visitor projection**

In `catan-runtime/src/simulation.rs`, replace:

```rust
for output in projector::project_transaction(transaction) {
    self.deliver_output(&output, observers);
    queue.push_back(output);
}
```

with:

```rust
projector::project_transaction_into(transaction, |output| {
    self.deliver_output(&output, observers);
    queue.push_back(output);
});
```

- [ ] **Step 5: Update tests for inline event records**

In projector/output tests, any pattern like:

```rust
let [GameOutput::Event(record)] = outputs.as_slice() else {
    panic!("transaction should project to one event output");
};
```

still works because `record` is now `&GameEventRecord`. No dereference change should be needed. If a test expected `Box<GameEventRecord>`, change the assertion to read fields directly from `record`.

- [ ] **Step 6: Verify tests and allocation metric**

Run:

```sh
cargo test -p catan-core projector output
cargo test -p catan-runtime --bin catan-bench
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 100 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log | jq '.allocations'
```

Expected:

- Tests pass.
- Allocation counts decrease from the Task 1 baseline.
- No deterministic summary assertions change solely because of projection storage.

- [ ] **Step 7: Commit**

```sh
git add catan-core/src/gameplay/game/io/output.rs catan-core/src/gameplay/game/io/projector.rs catan-runtime/src/simulation.rs
git commit -m "perf: inline projected game outputs"
```

---

## Task 3: Add Immutable Board Topology Caches to `FieldIndex`

**Files:**

- Modify: `catan-core/src/gameplay/field/index.rs`
- Modify: `catan-core/src/gameplay/field/state.rs`
- Test: add tests in `catan-core/src/gameplay/field/index.rs`

### Purpose

Stop rebuilding board path membership and incident path candidates inside legal road searches. The board topology is immutable after setup.

### Implementation Steps

- [ ] **Step 1: Import `PathSet` and `SmallSet`**

In `field/index.rs`, update imports to include:

```rust
use crate::{
    common::SmallSet,
    gameplay::{
        field::{BoardArrangement, HexesByNum},
        primitives::{PortKind, Tile, build::PathSet},
    },
    math::dice::TileNum,
    topology::{Hex, Intersection, Path},
};
```

If `build::PathSet` is not exported at that path, use:

```rust
use crate::gameplay::primitives::build::PathSet;
```

- [ ] **Step 2: Extend `FieldIndex`**

Change:

```rust
pub struct FieldIndex {
    pub desert_pos: Hex,
    pub hex_by_num: HexesByNum,
    pub ports_intersection: BTreeMap<Intersection, PortKind>,
    intersections: Vec<Intersection>,
    paths: Vec<Path>,
}
```

to:

```rust
pub struct FieldIndex {
    pub desert_pos: Hex,
    pub hex_by_num: HexesByNum,
    pub ports_intersection: BTreeMap<Intersection, PortKind>,
    intersections: Vec<Intersection>,
    paths: Vec<Path>,
    path_set: PathSet,
    incident_paths: Vec<(Intersection, SmallSet<Path, 3>)>,
}
```

- [ ] **Step 3: Build the caches in `FieldIndex::new`**

In `FieldIndex::new`, replace:

```rust
let intersections = board.intersections().into_iter().collect();
let paths = board.path_set().into_iter().collect();
```

with:

```rust
let intersections: Vec<Intersection> = board.intersections().into_iter().collect();
let path_set: PathSet = board.path_set().into_iter().collect();
let paths: Vec<Path> = path_set.iter().copied().collect();
let incident_paths = Self::incident_paths(&intersections, &path_set);
```

Include fields in `Self`:

```rust
path_set,
incident_paths,
```

- [ ] **Step 4: Add incident path builder and accessors**

In `impl FieldIndex`, add:

```rust
pub fn path_set(&self) -> &PathSet {
    &self.path_set
}

pub fn incident_paths(&self, intersection: Intersection) -> SmallSet<Path, 3> {
    self.incident_paths
        .binary_search_by_key(&intersection, |(pos, _)| *pos)
        .map(|index| self.incident_paths[index].1.clone())
        .unwrap_or_default()
}

fn incident_paths(
    intersections: &[Intersection],
    board_paths: &PathSet,
) -> Vec<(Intersection, SmallSet<Path, 3>)> {
    let mut result = Vec::with_capacity(intersections.len());
    for &intersection in intersections {
        let paths = intersection
            .paths_arr()
            .into_iter()
            .filter(|path| board_paths.contains(path))
            .collect::<SmallSet<Path, 3>>();
        result.push((intersection, paths));
    }
    result.sort_unstable_by_key(|(intersection, _)| *intersection);
    result
}
```

This allocates during board construction only, not in the main game loop.

- [ ] **Step 5: Expose through `BoardLayout`**

In `field/state.rs`, add:

```rust
pub fn path_set(&self) -> &crate::gameplay::primitives::build::PathSet {
    self.index.path_set()
}

pub fn incident_paths(&self, intersection: Intersection) -> SmallSet<Path, 3> {
    self.index.incident_paths(intersection)
}
```

Keep `paths(&self) -> &[Path]` for existing callers.

- [ ] **Step 6: Add tests**

In `field/index.rs` tests, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::field::ser::standard_4p_arrangement;

    #[test]
    fn path_set_matches_paths_slice() {
        let index = FieldIndex::new(&standard_4p_arrangement());
        let from_slice = index.paths().iter().copied().collect::<PathSet>();

        assert_eq!(index.path_set(), &from_slice);
    }

    #[test]
    fn incident_paths_are_valid_board_paths_touching_intersection() {
        let index = FieldIndex::new(&standard_4p_arrangement());

        for &intersection in index.intersections() {
            let incident = index.incident_paths(intersection);
            assert!(incident.len() <= 3);
            for path in incident {
                assert!(index.path_set().contains(&path));
                assert!(path.intersections().contains(&intersection));
            }
        }
    }
}
```

If `field/index.rs` already has a `tests` module, merge these tests into it.

- [ ] **Step 7: Verify**

Run:

```sh
cargo test -p catan-core gameplay::field::index
cargo test -p catan-core
```

Expected: tests pass.

- [ ] **Step 8: Commit**

```sh
git add catan-core/src/gameplay/field/index.rs catan-core/src/gameplay/field/state.rs
git commit -m "perf: cache board topology indexes"
```

---

## Task 4: Add Build-Legality Cache Data to `GameIndex`

**Files:**

- Modify: `catan-core/src/gameplay/game/index.rs`
- Modify: `catan-core/src/gameplay/game/query.rs`
- Tests: extend `catan-core/src/gameplay/game/index.rs`

### Purpose

Cache mutable state-derived data needed by legal build queries:

- occupied roads
- occupied establishments
- deadzone intersections
- opponent blockers per player
- road frontiers per player
- legal road candidates per player

This keeps rules in core and gives all agents/controllers the same fast query surface.

### Implementation Steps

- [ ] **Step 1: Add imports**

In `game/index.rs`, update build imports:

```rust
build::{Build, Establishment, EstablishmentType, PathSet, Road},
```

Also import:

```rust
use smallvec::SmallVec;
```

- [ ] **Step 2: Add type aliases**

Near the top of `game/index.rs`, add:

```rust
type PlayerIndexed<T> = SmallVec<[T; 8]>;
type IntersectionSet = SmallSet<Intersection, 64>;
type OpponentBlockers = SmallSet<Intersection, 32>;
```

- [ ] **Step 3: Extend `GameIndex`**

Change `GameIndex` to:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameIndex {
    pub all_builds: Vec<BuildCollection>,
    pub longest_road_lengths: Vec<u16>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
    pub ports_acquired: Vec<SmallSet<PortKind, PLAYER_PORTS_INLINE>>,
    pub occupied_roads: PathSet,
    pub occupied_establishments: IntersectionSet,
    pub deadzone_intersections: IntersectionSet,
    pub opponent_establishments: PlayerIndexed<OpponentBlockers>,
    pub road_frontiers: PlayerIndexed<IntersectionSet>,
    pub legal_road_candidates: PlayerIndexed<PathSet>,
}
```

Do not remove `all_builds` in this task; remove it later only after verifying no external dependency.

- [ ] **Step 4: Build cache fields in `rebuild_table`**

In `rebuild_table`, after `longest_road_owner`, add:

```rust
let occupied_roads = Self::occupied_roads(state);
let occupied_establishments = Self::occupied_establishments(state);
let deadzone_intersections = Self::deadzone_intersections(&occupied_establishments);
let opponent_establishments = Self::opponent_establishments_by_player(state);
let road_frontiers = Self::road_frontiers(state, &opponent_establishments);
let legal_road_candidates =
    Self::legal_road_candidates(state, &occupied_roads, &road_frontiers);
```

Set these fields in `Self`.

- [ ] **Step 5: Add cache builders**

In `impl GameIndex`, add:

```rust
fn occupied_roads(state: &TableState) -> PathSet {
    state
        .builds
        .players()
        .iter()
        .flat_map(|player| player.roads.edges().iter().copied())
        .collect()
}

fn occupied_establishments(state: &TableState) -> IntersectionSet {
    state
        .builds
        .players()
        .iter()
        .flat_map(|player| player.establishments.iter().map(|est| est.vtx))
        .collect()
}

fn deadzone_intersections(occupied: &IntersectionSet) -> IntersectionSet {
    let mut deadzone = IntersectionSet::new();
    for &intersection in occupied {
        deadzone.insert(intersection);
        for neighbor in intersection.neighbors_arr() {
            deadzone.insert(neighbor);
        }
    }
    deadzone
}

fn opponent_establishments_by_player(state: &TableState) -> PlayerIndexed<OpponentBlockers> {
    player_ids(state.players.count())
        .map(|player_id| Self::opponent_establishments(state, player_id))
        .collect()
}

fn road_frontiers(
    state: &TableState,
    opponent_establishments: &[OpponentBlockers],
) -> PlayerIndexed<IntersectionSet> {
    player_ids(state.players.count())
        .map(|player_id| {
            let mut frontier = IntersectionSet::new();
            for road in state.builds[player_id].roads.edges() {
                for intersection in road.intersections() {
                    if !opponent_establishments[player_id.index()].contains(&intersection) {
                        frontier.insert(intersection);
                    }
                }
            }
            frontier
        })
        .collect()
}

fn legal_road_candidates(
    state: &TableState,
    occupied_roads: &PathSet,
    road_frontiers: &[IntersectionSet],
) -> PlayerIndexed<PathSet> {
    player_ids(state.players.count())
        .map(|player_id| {
            if state.builds[player_id].roads_count() >= crate::gameplay::primitives::build::PlayerBuildData::ROAD_LIMIT {
                return PathSet::new();
            }

            let mut candidates = PathSet::new();
            for &intersection in &road_frontiers[player_id.index()] {
                for candidate in state.board.incident_paths(intersection) {
                    if !occupied_roads.contains(&candidate) {
                        candidates.insert(candidate);
                    }
                }
            }
            candidates
        })
        .collect()
}
```

- [ ] **Step 6: Recompute cache incrementally by rebuilding at first**

For this task, keep correctness simple. At the end of each mutating refresh method, rebuild the cache fields from state:

```rust
fn refresh_build_legality_cache(&mut self, state: &TableState) {
    self.occupied_roads = Self::occupied_roads(state);
    self.occupied_establishments = Self::occupied_establishments(state);
    self.deadzone_intersections = Self::deadzone_intersections(&self.occupied_establishments);
    self.opponent_establishments = Self::opponent_establishments_by_player(state);
    self.road_frontiers = Self::road_frontiers(state, &self.opponent_establishments);
    self.legal_road_candidates =
        Self::legal_road_candidates(state, &self.occupied_roads, &self.road_frontiers);
}
```

Call this at the end of:

```rust
refresh_after_build
refresh_after_roadbuild
```

Do not call it from `refresh_after_dev_card` unless `RoadBuild` was used; `refresh_after_roadbuild` already handles that.

This rebuild is acceptable as an intermediate step because builds are much rarer than legal queries. Later tasks can make this fully incremental if measurement says it matters.

- [ ] **Step 7: Add query accessors**

In `game/query.rs`, add:

```rust
use crate::gameplay::primitives::build::PathSet;
use crate::topology::Intersection;
```

Add methods:

```rust
pub fn legal_road_candidates(&self, player_id: PlayerId) -> &PathSet {
    &self.index.legal_road_candidates[player_id.index()]
}

pub fn occupied_roads(&self) -> &PathSet {
    &self.index.occupied_roads
}

pub fn deadzone_intersections(&self) -> &[Intersection] {
    self.index.deadzone_intersections.as_slice()
}

pub fn has_establishment_in_deadzone(&self, pos: Intersection) -> bool {
    self.index.deadzone_intersections.contains(&pos)
}
```

- [ ] **Step 8: Add tests comparing cached candidates to existing direct logic**

In `game/index.rs` tests, add:

```rust
#[test]
fn indexed_road_candidates_match_direct_generation_after_rebuild() {
    let mut state = SetupGameState::default().finish();
    let mut builds = empty_build_collections();
    let road = Road {
        path: path(h(0, 0), h(1, 0)),
    };
    builds[0].roads.push(road);
    state.builds = BoardBuildData::from_build_collections(builds);

    let index = GameIndex::rebuild(&state);
    let direct = state
        .builds
        .road_extension_candidates(P0, state.board.paths());

    assert_eq!(index.legal_road_candidates[P0.index()], direct);
}

#[test]
fn indexed_road_candidates_match_direct_generation_after_incremental_road() {
    let mut state = SetupGameState::default().finish();
    let mut index = GameIndex::rebuild(&state);
    let road = Road {
        path: path(h(0, 0), h(1, 0)),
    };
    let mut builds = empty_build_collections();
    builds[0].roads.push(road);
    state.builds = BoardBuildData::from_build_collections(builds);

    index.refresh_after_build(&state, P0, Build::Road(road));

    assert_matches_rebuild(&index, &state);
    assert_eq!(
        index.legal_road_candidates[P0.index()],
        state.builds.road_extension_candidates(P0, state.board.paths())
    );
}

#[test]
fn indexed_deadzone_contains_settlement_and_neighbors() {
    let mut state = SetupGameState::default().finish();
    let pos = path(h(0, 0), h(1, 0)).intersections()[0];
    let settlement = Establishment {
        vtx: pos,
        stage: EstablishmentType::Settlement,
    };
    let mut builds = empty_build_collections();
    builds[0].establishments.push(settlement);
    state.builds = BoardBuildData::from_build_collections(builds);

    let index = GameIndex::rebuild(&state);

    assert!(index.deadzone_intersections.contains(&pos));
    for neighbor in pos.neighbors_arr() {
        assert!(index.deadzone_intersections.contains(&neighbor));
    }
}
```

- [ ] **Step 9: Verify**

Run:

```sh
cargo test -p catan-core gameplay::game::index
cargo test -p catan-core gameplay::game::legal
cargo test -p catan-runtime --bin catan-bench
```

Expected: tests pass and deterministic benchmark tests remain unchanged.

- [ ] **Step 10: Commit**

```sh
git add catan-core/src/gameplay/game/index.rs catan-core/src/gameplay/game/query.rs
git commit -m "perf: cache build legality indexes"
```

---

## Task 5: Route Legal Build Counts Through Cached Core Indexes

**Files:**

- Modify: `catan-core/src/gameplay/game/view.rs`
- Modify: `catan-core/src/gameplay/game/legal.rs`
- Modify: `catan-core/src/gameplay/primitives/build.rs`
- Tests: existing legal tests plus new resource-count tests in `legal.rs`

### Purpose

Use cached legal opportunities for count queries and direct list queries. This is the highest-impact core change for trading and greedy-like resource scoring.

### Implementation Steps

- [ ] **Step 1: Expose `GameIndex` on public game views**

In `PublicGameView`, add:

```rust
pub index: &'a GameIndex,
```

In `ContextFactory::public_view`, set:

```rust
index: self.index,
```

This is a read-only core query surface. It must not leak mutable state.

- [ ] **Step 2: Add indexed road candidate accessor**

In `impl PublicGameView<'a>`, add:

```rust
pub fn legal_road_candidates_for(
    &self,
    player_id: PlayerId,
) -> &crate::gameplay::primitives::build::PathSet {
    &self.index.legal_road_candidates[player_id.index()]
}

pub fn has_establishment_in_deadzone(&self, pos: crate::topology::Intersection) -> bool {
    self.index.deadzone_intersections.contains(&pos)
}
```

- [ ] **Step 3: Route road count through cache**

In `legal_road_spots_count_with_resources`, replace:

```rust
context
    .public
    .builds
    .road_extension_candidates(player_id, context.public.board.paths())
    .into_iter()
    .inspect(|_| {
        #[cfg(feature = "bench-counters")]
        counters::road_candidate();
    })
    .count()
```

with:

```rust
let count = context.public.legal_road_candidates_for(player_id).len();
#[cfg(feature = "bench-counters")]
for _ in 0..count {
    counters::road_candidate();
}
count
```

- [ ] **Step 4: Route road list through cache**

In `legal_road_spots_iter`, replace the `road_extension_candidates(...).into_iter()` source with:

```rust
context
    .public
    .legal_road_candidates_for(player_id)
    .iter()
    .copied()
```

Because the return type is currently boxed, the full expression remains boxed for this task:

```rust
Box::new(
    context
        .public
        .legal_road_candidates_for(player_id)
        .iter()
        .copied()
        .inspect(move |_| {
            #[cfg(feature = "bench-counters")]
            counters::road_candidate();
        })
        .map(|pos| Build::Road(Road { path: pos })),
)
```

The boxed iterator is removed in Task 6.

- [ ] **Step 5: Add cached settlement count helper**

In `legal_settlement_spots_count_with_resources`, replace full-board `can_place_settlement` scans with a direct deadzone and road-frontier intersection scan:

```rust
let mut count = 0;
for &pos in context.public.index.road_frontiers[player_id.index()].as_slice() {
    #[cfg(feature = "bench-counters")]
    counters::settlement_candidate();
    if !context.public.has_establishment_in_deadzone(pos) {
        count += 1;
    }
}
count
```

This is equivalent for normal settlement placement after initial placement: a settlement must touch the player road network and be outside the deadzone.

- [ ] **Step 6: Keep settlement list behavior identical**

For now, keep `legal_settlement_spots_iter` using `can_place_settlement`. The count path is the one hit by trade scoring. Change list generation only after a test proves cached list and direct list match.

- [ ] **Step 7: Add tests for cached count equivalence**

In `legal.rs` tests, add:

```rust
#[test]
fn indexed_road_count_matches_direct_road_generation() {
    with_test_context(P0, |context| {
        let indexed = legal_road_spots_count(&context, P0);
        let direct = context
            .public
            .builds
            .road_extension_candidates(P0, context.public.board.paths())
            .len();

        assert_eq!(indexed, direct);
    });
}

#[test]
fn indexed_settlement_count_matches_direct_generation() {
    with_test_context(P0, |context| {
        let indexed = legal_settlement_spots_count(&context, P0);
        let direct = legal_settlement_spots(&context, P0).len();

        assert_eq!(indexed, direct);
    });
}
```

Use the existing helper name in `legal.rs`; if the helper is named differently, adapt these tests to the local helper that creates a `PlayerDecisionContext`.

- [ ] **Step 8: Verify deterministic stats and performance**

Run:

```sh
cargo test -p catan-core gameplay::game::legal
cargo test -p catan-runtime --bin catan-bench greedy_brawl_seed_zero_keeps_golden_summary
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log > target/bench-artifacts/indexed-legal-summary.json
jq '.rates, .allocations, .totals' target/bench-artifacts/indexed-legal-summary.json
```

Expected:

- Tests pass.
- Deterministic totals do not change.
- `legal_candidates` may remain comparable because counters still count candidates logically.
- Runtime and allocation counts should improve.

- [ ] **Step 9: Commit**

```sh
git add catan-core/src/gameplay/game/view.rs catan-core/src/gameplay/game/legal.rs catan-core/src/gameplay/primitives/build.rs
git commit -m "perf: use cached legal build counts"
```

---

## Task 6: Remove Boxed Iterators and `Vec` Legal APIs From Hot Paths

**Files:**

- Modify: `catan-core/src/gameplay/game/legal.rs`
- Modify: `catan-bots/src/greedy.rs`
- Modify: `catan-bots/src/random.rs`
- Modify: tests that collect legal iterators

### Purpose

Remove `Box<dyn Iterator>` heap allocation and avoid collecting legal actions into `Vec` where callers only need first/max/count/random choice.

### Implementation Steps

- [ ] **Step 1: Add callback visitor APIs for build classes**

In `legal.rs`, add:

```rust
pub fn for_each_legal_city_spot(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    mut visit: impl FnMut(Build),
) {
    if context.search.is_none() {
        log::debug!("legal city spots require search context");
        return;
    }
    if !context.private.resources.has_enough(&costs::CITY)
        || context.public.builds.by_player(player_id).cities_count() >= PlayerBuildData::CITY_LIMIT
    {
        return;
    }

    for est in context.public.builds.by_player(player_id).establishments.iter().copied() {
        if est.stage == EstablishmentType::Settlement {
            #[cfg(feature = "bench-counters")]
            counters::city_candidate();
            visit(Build::Establishment(Establishment {
                vtx: est.vtx,
                stage: EstablishmentType::City,
            }));
        }
    }
}

pub fn for_each_legal_road_spot(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    mut visit: impl FnMut(Build),
) {
    if !can_search_road_with_resources(context, player_id, context.private.resources) {
        return;
    }

    for &path in context.public.legal_road_candidates_for(player_id) {
        #[cfg(feature = "bench-counters")]
        counters::road_candidate();
        visit(Build::Road(Road { path }));
    }
}
```

Add settlement visitor only after Task 5 settlement list equivalence is proven:

```rust
pub fn for_each_legal_settlement_spot(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    mut visit: impl FnMut(Build),
) {
    if !can_search_settlement_with_resources(context, player_id, context.private.resources) {
        return;
    }

    for &pos in context.public.index.road_frontiers[player_id.index()].as_slice() {
        #[cfg(feature = "bench-counters")]
        counters::settlement_candidate();
        if !context.public.has_establishment_in_deadzone(pos) {
            visit(Build::Establishment(Establishment {
                vtx: pos,
                stage: EstablishmentType::Settlement,
            }));
        }
    }
}
```

- [ ] **Step 2: Reimplement `Vec` APIs as compatibility wrappers**

Keep public compatibility wrappers, but make them call visitors:

```rust
pub fn legal_city_spots(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
    let mut builds = Vec::new();
    for_each_legal_city_spot(context, player_id, |build| builds.push(build));
    builds
}

pub fn legal_road_spots(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
    let mut builds = Vec::new();
    for_each_legal_road_spot(context, player_id, |build| builds.push(build));
    builds
}

pub fn legal_settlement_spots(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Vec<Build> {
    let mut builds = Vec::new();
    for_each_legal_settlement_spot(context, player_id, |build| builds.push(build));
    builds
}
```

Do not delete old iterator functions until all callers are migrated. Mark them as compatibility wrappers if retained.

- [ ] **Step 3: Update greedy city selection**

In `greedy.rs`, replace:

```rust
legal::legal_city_spots(context, player_id)
    .into_iter()
    .next()
```

with:

```rust
let mut best = None;
legal::for_each_legal_city_spot(context, player_id, |build| {
    if best.is_none() {
        best = Some(build);
    }
});
best
```

- [ ] **Step 4: Update greedy settlement selection**

Replace:

```rust
legal::legal_settlement_spots(context, player_id)
    .into_iter()
    .max_by_key(|build| match build {
        Build::Establishment(establishment) => {
            settlement_production_score(context.public.board, establishment.vtx)
        }
        Build::Road(_) => 0,
    })
```

with:

```rust
let mut best: Option<(Build, u16)> = None;
legal::for_each_legal_settlement_spot(context, player_id, |build| {
    let score = match build {
        Build::Establishment(establishment) => {
            settlement_production_score(context.public.board, establishment.vtx)
        }
        Build::Road(_) => 0,
    };
    if best.as_ref().is_none_or(|(_, best_score)| score > *best_score) {
        best = Some((build, score));
    }
});
best.map(|(build, _)| build)
```

If `Option::is_none_or` is not available for the project toolchain despite Rust 1.95, replace with:

```rust
if best.as_ref().map_or(true, |(_, best_score)| score > *best_score) {
```

- [ ] **Step 5: Update greedy road selection**

Replace:

```rust
let roads = legal::legal_road_spots(context, player_id);
...
roads.into_iter().max_by_key(|build| { ... })
```

with:

```rust
let resources_after_road = context
    .private
    .resources
    .checked_sub(&constants::costs::ROAD)?;

let mut best: Option<(Build, usize)> = None;
legal::for_each_legal_road_spot(context, player_id, |build| {
    let Build::Road(road) = build else {
        return;
    };
    let score = legal::legal_settlement_spots_count_with_extra_road(
        context,
        player_id,
        road.path,
        &resources_after_road,
    );
    if best.as_ref().map_or(true, |(_, best_score)| score > *best_score) {
        best = Some((build, score));
    }
});
best.map(|(build, _)| build)
```

- [ ] **Step 6: Update random bot**

In `catan-bots/src/random.rs`, replace direct `legal_*_spots_iter` calls with either:

```rust
let mut options = smallvec::SmallVec::<[RegularCommand; 32]>::new();
legal::for_each_legal_road_spot(context, context.actor, |build| {
    options.push(RegularCommand::Build(build));
});
```

or keep compatibility wrappers if random selection code requires indexed access. Use `SmallVec`, not `Vec`.

- [ ] **Step 7: Verify**

Run:

```sh
cargo test -p catan-bots
cargo test -p catan-core gameplay::game::legal
cargo test -p catan-runtime --bin catan-bench greedy_brawl_seed_zero_keeps_golden_summary
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log | jq '.allocations, .rates'
```

Expected:

- Tests pass.
- Deterministic seed-zero stats remain unchanged.
- Allocation counts decrease.

- [ ] **Step 8: Commit**

```sh
git add catan-core/src/gameplay/game/legal.rs catan-bots/src/greedy.rs catan-bots/src/random.rs
git commit -m "perf: avoid boxed legal iterators"
```

---

## Task 7: Remove Bot Trade Temporary Vectors Without Changing Strategy

**Files:**

- Modify: `catan-bots/src/trade.rs`
- Modify: `catan-bots/src/greedy.rs`
- Test: `catan-bots` tests

### Purpose

Keep bot behavior identical while removing easy heap allocations introduced by trading.

### Implementation Steps

- [ ] **Step 1: Replace one-card candidate `Vec` with iterator**

In `catan-bots/src/trade.rs`, replace:

```rust
pub fn one_card_trade_candidates(context: &PlayerDecisionContext<'_>) -> Vec<PlayerTrade> {
    Resource::iter()
        .filter(|give| context.private.resources[*give] > 0)
        .flat_map(move |give| {
            Resource::iter()
                .filter(move |take| *take != give)
                .map(move |take| one_card_trade(give, take))
        })
        .collect()
}
```

with:

```rust
pub fn one_card_trade_candidates(
    context: &PlayerDecisionContext<'_>,
) -> impl Iterator<Item = PlayerTrade> + '_ {
    Resource::iter()
        .filter(|give| context.private.resources[*give] > 0)
        .flat_map(move |give| {
            Resource::iter()
                .filter(move |take| *take != give)
                .map(move |take| one_card_trade(give, take))
        })
}
```

- [ ] **Step 2: Update greedy player trade loop**

In `greedy.rs`, replace:

```rust
trade::one_card_trade_candidates(context)
    .into_iter()
    .filter_map(|trade| {
```

with:

```rust
trade::one_card_trade_candidates(context)
    .filter_map(|trade| {
```

- [ ] **Step 3: Replace committable offers `Vec` with iterator**

In `trade.rs`, replace:

```rust
pub fn committable_offers(
    context: &PlayerDecisionContext<'_>,
    session: &TradeSession,
) -> Vec<(TradeOfferId, PlayerId)> {
    session
        .offers
        .iter()
        .filter_map(move |offer| {
            let peer_id = if offer.proposer == session.proposer {
                session.accepted_peer_for_offer(offer.id)?
            } else {
                offer.proposer
            };
            offer_is_funded(context, session, offer.id, peer_id).then_some((offer.id, peer_id))
        })
        .collect()
}
```

with:

```rust
pub fn committable_offers<'a>(
    context: &'a PlayerDecisionContext<'_>,
    session: &'a TradeSession,
) -> impl Iterator<Item = (TradeOfferId, PlayerId)> + 'a {
    session.offers.iter().filter_map(move |offer| {
        let peer_id = if offer.proposer == session.proposer {
            session.accepted_peer_for_offer(offer.id)?
        } else {
            offer.proposer
        };
        offer_is_funded(context, session, offer.id, peer_id).then_some((offer.id, peer_id))
    })
}
```

- [ ] **Step 4: Update greedy owner action**

In `greedy.rs`, replace:

```rust
trade::committable_offers(&context, session)
    .into_iter()
    .filter_map(|(offer_id, _)| {
```

with:

```rust
trade::committable_offers(&context, session)
    .filter_map(|(offer_id, _)| {
```

- [ ] **Step 5: Use inline storage for attempted trades**

In `greedy.rs`, change the `GreedyAgent` field:

```rust
attempted_trades: Vec<(ResourceSet, ResourceSet)>,
```

to:

```rust
attempted_trades: smallvec::SmallVec<[(ResourceSet, ResourceSet); 16]>,
```

Change `Vec::new()` initializers to:

```rust
smallvec::SmallVec::new()
```

Change function parameter:

```rust
attempted_trades: &mut Vec<(ResourceSet, ResourceSet)>,
```

to:

```rust
attempted_trades: &mut smallvec::SmallVec<[(ResourceSet, ResourceSet); 16]>,
```

- [ ] **Step 6: Verify**

Run:

```sh
cargo test -p catan-bots
cargo test -p catan-runtime --bin catan-bench greedy_brawl_seed_zero_keeps_golden_summary
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log | jq '.allocations, .totals'
```

Expected:

- Tests pass.
- Seed-zero deterministic totals remain unchanged.
- Allocation counts decrease or remain no worse.

- [ ] **Step 7: Commit**

```sh
git add catan-bots/src/trade.rs catan-bots/src/greedy.rs
git commit -m "perf: avoid heap allocations in trade scoring"
```

---

## Task 8: Replace Dynamic Trade Session Storage With Inline Storage

**Files:**

- Modify: `catan-core/src/gameplay/game/trade.rs`
- Modify: projection/serde tests if necessary

### Purpose

Trade sessions are now part of the core game loop. Their offer/response storage should not allocate for normal 4-player games.

### Implementation Steps

- [ ] **Step 1: Inspect current trade storage**

Open `catan-core/src/gameplay/game/trade.rs` and find:

```rust
pub offers: Vec<TradeOffer>,
pub responses: Vec<Option<TradeResponseState>>,
```

- [ ] **Step 2: Add inline aliases**

At the top of `trade.rs`, add:

```rust
use smallvec::SmallVec;

type TradeOffers = SmallVec<[TradeOffer; 16]>;
type TradeResponses = SmallVec<[Option<TradeResponseState>; 8]>;
```

- [ ] **Step 3: Replace fields**

Change fields to:

```rust
pub offers: TradeOffers,
pub responses: TradeResponses,
```

- [ ] **Step 4: Replace vector constructors**

Change `Vec::new()` for these fields to:

```rust
SmallVec::new()
```

Change `vec![None; n]` to:

```rust
(0..n).map(|_| None).collect()
```

This collects into the `SmallVec` field type.

- [ ] **Step 5: Verify serde compatibility**

Because `smallvec` has serde support in this workspace, derive serialization should continue to work. Run:

```sh
cargo test -p catan-core trade
cargo test -p catan-runtime
```

Expected: tests pass.

- [ ] **Step 6: Commit**

```sh
git add catan-core/src/gameplay/game/trade.rs
git commit -m "perf: inline trade session storage"
```

---

## Task 9: Optimize Road Candidate Generation With Cached Index Inputs

**Files:**

- Modify: `catan-core/src/gameplay/primitives/build.rs`
- Modify: `catan-core/src/gameplay/game/index.rs`
- Tests: existing build/index tests

### Purpose

Eliminate repeated cache reconstruction inside `road_extension_candidates_with_extra_roads` and make the core helper accept precomputed immutable/mutable index inputs.

### Implementation Steps

- [ ] **Step 1: Add a helper that accepts precomputed inputs**

In `build.rs`, add near `road_extension_candidates_with_extra_roads`:

```rust
pub fn road_extension_candidates_from_frontier(
    frontier: &SmallSet<Intersection, 64>,
    occupied_roads: &PathSet,
    incident_paths: impl Fn(Intersection) -> SmallSet<Path, 3>,
) -> PathSet {
    let mut candidates = PathSet::new();
    for &intersection in frontier {
        for candidate in incident_paths(intersection) {
            if !occupied_roads.contains(&candidate) {
                candidates.insert(candidate);
            }
        }
    }
    candidates
}
```

If adding a free function inside the module is awkward, add it as an associated function on `BoardBuildData`.

- [ ] **Step 2: Use helper in `GameIndex::legal_road_candidates`**

In `game/index.rs`, replace the explicit candidate loop with:

```rust
crate::gameplay::primitives::build::road_extension_candidates_from_frontier(
    &road_frontiers[player_id.index()],
    occupied_roads,
    |intersection| state.board.incident_paths(intersection),
)
```

- [ ] **Step 3: Add an extra-road optimized path for road-building dev card**

Add a helper in `GameIndex`:

```rust
pub fn legal_road_candidates_with_extra_roads(
    &self,
    state: &TableState,
    player_id: PlayerId,
    extra_roads: &[Path],
) -> PathSet {
    if state.builds[player_id].roads_count() + extra_roads.len()
        >= crate::gameplay::primitives::build::PlayerBuildData::ROAD_LIMIT
    {
        return PathSet::new();
    }

    let extra_roads_set = extra_roads.iter().copied().collect::<PathSet>();
    let mut occupied = self.occupied_roads.union(&extra_roads_set);
    let mut frontier = self.road_frontiers[player_id.index()].clone();
    for road in extra_roads_set.iter() {
        for intersection in road.intersections() {
            if !self.opponent_establishments[player_id.index()].contains(&intersection) {
                frontier.insert(intersection);
            }
        }
    }

    crate::gameplay::primitives::build::road_extension_candidates_from_frontier(
        &frontier,
        &occupied,
        |intersection| state.board.incident_paths(intersection),
    )
}
```

If the compiler warns that `occupied` need not be mutable, remove `mut`.

- [ ] **Step 4: Route `legal_k_road_extensions` through index only if context is available**

`legal_k_road_extensions` currently accepts `BoardBuildData`, not context/index. Do not change the public API in this task. Add a new context-based helper:

```rust
pub fn legal_k_road_extensions_for_context<'a, const K: usize>(
    context: &'a PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> IndexedRoadExtensionIter<'a, K> {
    IndexedRoadExtensionIter::new(context, player_id)
}
```

Implement `IndexedRoadExtensionIter` by using `context.public.index.legal_road_candidates_with_extra_roads(...)` for each prefix. Keep the old iterator for tests and compatibility.

- [ ] **Step 5: Verify**

Run:

```sh
cargo test -p catan-core gameplay::primitives::build
cargo test -p catan-core gameplay::game::index
cargo test -p catan-core gameplay::game::legal
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log | jq '.allocations, .rates'
```

Expected: tests pass, deterministic totals unchanged, road candidate hotpath reduced in `sample`.

- [ ] **Step 6: Commit**

```sh
git add catan-core/src/gameplay/primitives/build.rs catan-core/src/gameplay/game/index.rs catan-core/src/gameplay/game/legal.rs
git commit -m "perf: reuse indexed road candidate data"
```

---

## Task 10: Profile, Compare, and Decide Whether to Remove `all_builds`

**Files:**

- Modify: `docs/performance-baseline.md`
- Optionally modify: `catan-core/src/gameplay/game/index.rs`

### Purpose

Run final verification and clean up stale index fields if they are still unused.

### Implementation Steps

- [ ] **Step 1: Check if `all_builds` is still only internally used**

Run:

```sh
rg -n "all_builds" catan-core/src catan-runtime/src catan-bots/src
```

Expected after previous tasks:

```text
catan-core/src/gameplay/game/index.rs:...
```

If no external runtime/legal/UI code uses it, remove `all_builds` from `GameIndex` and delete `insert_road`/`upsert_establishment` if they become unused. Keep tests focused on functional cached fields.

- [ ] **Step 2: Run full correctness suite**

Run:

```sh
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 3: Run stable benchmark with allocation counters**

Run:

```sh
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log > target/bench-artifacts/final-1000-summary.json
jq '.totals, .rates, .allocations' target/bench-artifacts/final-1000-summary.json
```

Expected:

- `.totals` matches the pre-optimization deterministic current behavior.
- `.allocations.alloc_calls` is materially lower than Task 1 baseline.
- `.allocations.realloc_calls` is near zero or explained by remaining unavoidable paths.

- [ ] **Step 4: Run `hyperfine`**

Run:

```sh
hyperfine \
  --warmup 3 \
  --runs 20 \
  --export-json target/bench-artifacts/final-hyperfine-100.json \
  'target/release/catan-bench --config catan-runtime/data/configurations/greedy_brawl.json --games 100 --seed 0 --seed-stride 1 --no-log'
jq '.results[0] | {mean,stddev,min,max,user,system,memory_usage_byte: .memory_usage_byte[0]}' \
  target/bench-artifacts/final-hyperfine-100.json
```

Expected: mean runtime lower than the pre-plan `~202ms` for 100 games on the same machine/build profile.

- [ ] **Step 5: Run macOS `sample`**

Run:

```sh
(
  target/release/catan-bench \
    --config catan-runtime/data/configurations/greedy_brawl.json \
    --games 5000 \
    --seed 0 \
    --seed-stride 1 \
    --no-log >/tmp/catan-bench-5000-final.out &
  pid=$!
  sleep 0.2
  sample "$pid" 5 -file target/bench-artifacts/final-sample-5000.txt
  wait "$pid"
)
rg -n "Sort by top of stack|road_extension_candidates|malloc|free|project_and_deliver|greedy_regular_action" \
  target/bench-artifacts/final-sample-5000.txt | tail -120
```

Expected:

- `road_extension_candidates_with_extra_roads` no longer dominates top-of-stack samples for ordinary one-road legal counts.
- allocator frames drop materially.
- any remaining allocator frames are traceable to known compatibility wrappers or projection/serialization paths.

- [ ] **Step 6: Update performance docs**

In `docs/performance-baseline.md`, add a short dated section generated from
the measured artifacts, not hand-written estimates:

```sh
jq '.summary | {
  simulations_per_sec,
  turns_per_sec,
  allocations,
  reallocations
}' target/bench-artifacts/final-1000-summary.json

jq '.results[0] | {
  command,
  mean,
  stddev,
  min,
  max
}' target/bench-artifacts/final-hyperfine-100.json
```

The documentation section must include:

- The exact benchmark command.
- The pre-change baseline values from the "Current Evidence" section.
- The final measured `simulations_per_sec`.
- The final measured `turns_per_sec`.
- The final measured allocation-call count.
- The final measured reallocation-call count.
- The final measured 100-game hyperfine mean and standard deviation.
- The remaining hottest core frame from the final `sample` output.
- The remaining allocator source, or "none visible in sampled hot path" if no allocator frame remains in the final hot stack.

- [ ] **Step 7: Commit**

```sh
git add docs/performance-baseline.md catan-core/src/gameplay/game/index.rs
git commit -m "docs: record core hot loop benchmark"
```

---

## Risk Notes

- Cached legal candidates must not become a parallel rule engine. `BoardBuildData::can_*` remains the source of truth for applying builds. Cached candidates are query accelerators and must be tested against direct validation.
- Settlement legality has two modes: initial placement and normal placement. Do not route initial placement through road-frontier caches.
- RoadBuild dev card needs hypothetical extra roads. Keep existing direct logic until the indexed extra-road path has equivalence tests.
- Bench allocation counters count all allocations after reset, including allocations caused by bots and runtime observers. This is intentional for simulation-loop accounting.
- `SmallSet<Path, 72>` should not spill on standard boards. Add tests around `.spilled()` for standard board path sets if a regression is suspected.
- `Vec` in projection structs is acceptable for remote/UI serialization paths, but not in simulation-loop projection and legal query paths.

## Validation Matrix

Run these after the full plan:

```sh
cargo test --workspace
cargo test -p catan-runtime --bin catan-bench greedy_brawl_seed_zero_keeps_golden_summary
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters,bench-allocs
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log
hyperfine --warmup 3 --runs 20 \
  'target/release/catan-bench --config catan-runtime/data/configurations/greedy_brawl.json --games 100 --seed 0 --seed-stride 1 --no-log'
```

Expected final state:

- Deterministic behavior unchanged unless an existing stale golden intentionally updates.
- Heap allocation calls in the benchmark loop are substantially lower than Task 1 baseline.
- 100-game hyperfine mean improves from the observed `~202ms` baseline on the same machine.
- `sample` no longer shows projection allocation and direct road candidate reconstruction as top hot spots.

## Self-Review

- Spec coverage: The plan covers strict heap control, `GameIndex`/`FieldIndex` core caching, inline structures, legal query APIs, projection allocation, benchmark measurement, and profiling.
- Placeholder scan: no task contains fill-in placeholders; final documentation values are generated from measured artifacts.
- Type consistency: `PathSet`, `SmallSet`, `GameIndex`, `FieldIndex`, `PlayerDecisionContext`, and `PublicGameView` names match existing code. New APIs are introduced before use.
