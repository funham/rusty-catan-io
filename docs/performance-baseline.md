# Greedy Brawl Performance Baseline

Target scenario:

```sh
catan-runtime/data/configurations/greedy_brawl.json
```

Use release builds only for timing and profiling.

## Stable Hyperfine Pipeline

The stable benchmark pipeline builds the deterministic benchmark binary with
`bench-counters`, writes one JSON summary, and times repeated release runs with
`hyperfine`.

Install the required cargo plugin locally:

```sh
cargo install hyperfine --locked
```

Run the same command used by CI:

```sh
BENCH_GAMES=30 BENCH_RUNS=10 BENCH_WARMUP=2 scripts/bench-stable.sh
```

Artifacts are written under `target/bench-artifacts/`:

- `catan-bench-summary.json`
- `hyperfine.json`
- `hyperfine.md`

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
cargo install flamegraph --locked

PROFILE_GAMES=100 scripts/profile-flamegraph.sh
```

The flamegraph SVG is written to
`target/bench-artifacts/catan-bench-flamegraph.svg`. On Linux runners this
requires access to `perf`; the GitHub Actions profiling workflow relaxes the
runner's perf settings before invoking the script.
