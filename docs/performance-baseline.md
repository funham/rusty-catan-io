# Greedy Brawl Performance Baseline

Target scenario:

```sh
catan-runtime/data/configurations/greedy_brawl.json
```

Use release builds only for timing and profiling.

## One-Off Release Run

```sh
cargo build --release -p catan-runtime --bin catan-runtime
RUST_LOG=off target/release/catan-runtime catan-runtime/data/configurations/greedy_brawl.json
```

## Batched Benchmark Harness

The benchmark runner executes many games in one process with deterministic dice
and dev-card deck shuffling. It currently supports observer-free configs with
in-process `lazy` and `greedy` agents.

```sh
cargo build --release -p catan-runtime --bin catan-bench
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 30 \
  --seed 0 \
  --seed-stride 1 \
  --no-log
```

For machine-readable output:

```sh
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 30 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log
```

For legal-action candidate counters, rebuild with the dev-only counter feature:

```sh
cargo build --release -p catan-runtime --bin catan-bench --features bench-counters
target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 30 \
  --seed 0 \
  --seed-stride 1 \
  --json-summary \
  --no-log
```

## Profiling Builds

Keep optimization enabled and add debug symbols:

```sh
RUSTFLAGS="-C debuginfo=1" cargo build --release -p catan-runtime --bin catan-bench
```

On macOS, use Instruments Time Profiler as the primary profiler:

```sh
RUST_LOG=off target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 100 \
  --seed 0 \
  --no-log
```

On Linux/x86_64, prefer host-native `perf` or `cargo flamegraph`:

```sh
RUSTFLAGS="-C debuginfo=1 -C force-frame-pointers=yes" \
  cargo build --release -p catan-runtime --bin catan-bench

perf record -g -- target/release/catan-bench \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 100 \
  --seed 0 \
  --no-log
```
