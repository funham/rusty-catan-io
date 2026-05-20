#!/usr/bin/env bash
set -euo pipefail

BENCH_CONFIG="${BENCH_CONFIG:-catan-runtime/data/configurations/greedy_brawl.json}"
BENCH_GAMES="${BENCH_GAMES:-30}"
BENCH_RUNS="${BENCH_RUNS:-10}"
BENCH_SEED="${BENCH_SEED:-0}"
BENCH_SEED_STRIDE="${BENCH_SEED_STRIDE:-1}"
BENCH_WARMUP="${BENCH_WARMUP:-2}"
ARTIFACT_DIR="${ARTIFACT_DIR:-target/bench-artifacts}"

if ! command -v hyperfine >/dev/null 2>&1; then
    echo "hyperfine is required. Install it with: cargo install hyperfine --locked" >&2
    exit 127
fi

mkdir -p "$ARTIFACT_DIR"

cargo build --release -p catan-runtime --bin catan-bench --features bench-counters

BENCH_COMMAND=(
    "target/release/catan-bench"
    "--config" "$BENCH_CONFIG"
    "--games" "$BENCH_GAMES"
    "--seed" "$BENCH_SEED"
    "--seed-stride" "$BENCH_SEED_STRIDE"
    "--no-log"
)

"${BENCH_COMMAND[@]}" --json-summary >"$ARTIFACT_DIR/catan-bench-summary.json"

hyperfine \
    --warmup "$BENCH_WARMUP" \
    --runs "$BENCH_RUNS" \
    --export-json "$ARTIFACT_DIR/hyperfine.json" \
    --export-markdown "$ARTIFACT_DIR/hyperfine.md" \
    "${BENCH_COMMAND[*]}"
