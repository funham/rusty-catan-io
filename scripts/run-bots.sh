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
