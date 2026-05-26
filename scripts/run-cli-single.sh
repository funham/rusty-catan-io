#!/usr/bin/env bash
set -euo pipefail

CONFIG="catan-remote/data/configurations/cli_single.json"
SOCKET="target/catan-remote/cli-single.sock"
LOG_FILTER="${RUST_LOG:-info}"
RELEASE=0

usage() {
  cat <<'USAGE'
usage: scripts/run-cli-single.sh [--config PATH] [--socket PATH] [--log FILTER] [--release]

Starts a catan-remote host in this terminal for logs, then opens separate
Terminal.app windows for the player TUI and snapshot observer TUI.
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
rm -f "$SOCKET"

CARGO_BUILD_ARGS=(build -p catan-remote --bin catan-remote)
TARGET_PROFILE="debug"
if [[ "$RELEASE" -eq 1 ]]; then
  CARGO_BUILD_ARGS=(build --release -p catan-remote --bin catan-remote)
  TARGET_PROFILE="release"
fi

cleanup() {
  if [[ -n "${HOST_PID:-}" ]]; then
    kill "$HOST_PID" >/dev/null 2>&1 || true
    wait "$HOST_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

shell_command_for_tui() {
  local role="$1"
  printf 'cd %q && RUST_LOG=%q exec %q tui --connect %q --role %q' \
    "$PWD" \
    "$LOG_FILTER" \
    "$BIN" \
    "unix://$SOCKET" \
    "$role"
}

open_tui_terminal() {
  local role="$1"
  local label="$2"
  local command

  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "opening $label in a new terminal requires macOS Terminal.app" >&2
    exit 1
  fi
  if ! command -v osascript >/dev/null 2>&1; then
    echo "opening $label in a new terminal requires osascript" >&2
    exit 1
  fi

  command="$(shell_command_for_tui "$role")"
  osascript - "$command" <<'APPLESCRIPT'
on run argv
  tell application "Terminal"
    activate
    do script (item 1 of argv)
  end tell
end run
APPLESCRIPT
}

cargo "${CARGO_BUILD_ARGS[@]}"
BIN="${CARGO_TARGET_DIR:-target}/$TARGET_PROFILE/catan-remote"

RUST_LOG="$LOG_FILTER" "$BIN" \
  host --config "$CONFIG" --listen "unix://$SOCKET" &
HOST_PID=$!

for _ in {1..100}; do
  [[ -S "$SOCKET" ]] && break
  if ! kill -0 "$HOST_PID" >/dev/null 2>&1; then
    wait "$HOST_PID"
  fi
  sleep 0.05
done

if [[ ! -S "$SOCKET" ]]; then
  echo "host did not create socket: $SOCKET" >&2
  exit 1
fi

open_tui_terminal "player" "player TUI"
# The remote host assigns roles by accept order: player seats first, observers
# second. Give Terminal.app a moment to start the player client before opening
# the snapshot observer client.
sleep 0.5
open_tui_terminal "snapshot-observer" "snapshot observer TUI"

echo "host logs are streaming in this terminal"
echo "player and snapshot observer TUIs opened in separate Terminal.app windows"
wait "$HOST_PID"
