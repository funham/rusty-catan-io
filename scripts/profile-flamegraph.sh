#!/usr/bin/env bash
set -euo pipefail

BENCH_CONFIG="${BENCH_CONFIG:-catan-runtime/data/configurations/greedy_brawl.json}"
BENCH_SEED="${BENCH_SEED:-0}"
BENCH_SEED_STRIDE="${BENCH_SEED_STRIDE:-1}"
PROFILE_GAMES="${PROFILE_GAMES:-100}"
ARTIFACT_DIR="${ARTIFACT_DIR:-target/bench-artifacts}"
FLAMEGRAPH_OUTPUT="${FLAMEGRAPH_OUTPUT:-$ARTIFACT_DIR/catan-bench-flamegraph.svg}"

if ! cargo flamegraph --help >/dev/null 2>&1; then
    echo "cargo-flamegraph is required. Install it with: cargo install flamegraph --locked" >&2
    exit 127
fi

mkdir -p "$ARTIFACT_DIR"

export RUSTFLAGS="${RUSTFLAGS:-} -C debuginfo=1 -C force-frame-pointers=yes"

CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph \
    --release \
    --package catan-runtime \
    --bin catan-bench \
    --features bench-counters \
    --output "$FLAMEGRAPH_OUTPUT" \
    -- \
    --config "$BENCH_CONFIG" \
    --games "$PROFILE_GAMES" \
    --seed "$BENCH_SEED" \
    --seed-stride "$BENCH_SEED_STRIDE" \
    --no-log
