# catan-runtime CLI Agent

`catan-runtime` runs local games and can attach a Ratatui-based CLI agent for human play. The CLI opens in an alternate-screen terminal UI with the board on the left, public game state on the right, personal cards below it, and a command line at the bottom.

## Running a CLI Game

Use one of the runtime configurations that includes a CLI player:

```sh
cargo run --bin catan-runtime
```

If a different configuration is needed, pass a JSON file under `catan-runtime/data/configurations/`:

```sh
cargo run --bin catan-runtime -- catan-runtime/data/configurations/observer_debug.json
```

## Logging

Both the host runtime and the spawned CLI child use `env_logger`, so log verbosity is controlled with `RUST_LOG`.

The CLI child runs in its own terminal window, but its logs are sent back to the host over a dedicated log socket. The host then re-emits those child log lines with the `catan_cli_child` target and includes the child role plus the original child target in the message:

```text
[player-0][catan_runtime::cli_child::session] selected road
```

To see trace logs from the CLI child and debug logs from `catan-runtime`, run:

```sh
RUST_LOG=warn,catan_runtime=debug,catan_runtime::cli_child=trace,catan_cli_child=trace \
  cargo run --bin catan-runtime
```

For a specific configuration:

```sh
RUST_LOG=warn,catan_runtime=debug,catan_runtime::cli_child=trace,catan_cli_child=trace \
  cargo run --bin catan-runtime -- \
  catan-runtime/data/configurations/observer_debug.json
```

Useful variants:

```sh
# Only CLI child trace logs.
RUST_LOG=warn,catan_runtime::cli_child=trace,catan_cli_child=trace \
  cargo run --bin catan-runtime

# Runtime debug logs plus all warnings from dependencies.
RUST_LOG=warn,catan_runtime=debug cargo run --bin catan-runtime

# Very verbose: trace everything in this crate, including the child before forwarding.
RUST_LOG=catan_runtime=trace,catan_cli_child=trace cargo run --bin catan-runtime
```

The child log filter has two required parts:

- `catan_runtime::cli_child=trace` lets the child process emit its own trace records.
- `catan_cli_child=trace` lets the host print the forwarded child records.

If either half is missing, child trace logs will not appear in the host terminal.

Look for forwarded child logs in the terminal where the host command was run, not in the child TUI terminal. `catan-runtime` currently emits few host-side debug records, so `catan_runtime=debug` may be active without printing many runtime debug lines on a normal game path. To include debug records from core game-rule helpers too, add `catan_core=debug`.

Runtime logs are also written to timestamped files by default under `target/catan-logs/`. The runtime prints the exact file path at startup:

```text
writing runtime logs to target/catan-logs/rusty-catan-YYYY-MM-DDTHH-MM-SSZ.log
```

Because the child logs are forwarded into the host logger, they appear in both stderr and that log file when runtime logging is enabled.

## Screen Layout

- **Field**: board, robber, roads, settlements, cities, and selection previews.
- **Public**: robber index, bank resources, awards, other players, and command reminders.
- **Personal**: your resource cards and development cards.
- **Command**: typed commands and modal selector status.

Resource cards are shown as small boxes. The number inside a resource card is your card count. Development cards show their abbreviation; usable development cards show `used`, `active`, and `queued` counts next to the card. Victory Point cards show a single count.

## Basic Turn Commands

- `roll` or `r`: roll dice.
- `end` or `e`: end your turn.
- `buy dev` or `bd`: buy a development card.
- `bank-trade` or `bt`: open the interactive bank-trade menu.

Fully typed bank trades are still supported:

```text
bank-trade brick ore G4
bank-trade wood wheat G3
bank-trade sheep brick S2
```

`G4` is a 4:1 generic bank trade, `G3` is a 3:1 universal-port trade, and `S2` is a 2:1 specific-port trade.

## Building

Typed build commands still work:

```text
build road 0 1
build settlement 0 1 2
build city 0 1 2
```

Interactive build shortcuts:

- `build road` or `br`: cycle legal road placements.
- `build settlement` or `bs`: cycle legal settlement placements.
- `build city` or `bc`: cycle settlements that can be upgraded.

In selection mode, use arrow keys or Tab to cycle options, Enter to confirm, and Esc to cancel.

## Development Cards

Typed development-card commands still work:

```text
use knight 4 none
use knight 4 1
use monopoly ore
use yop brick wheat
use roadbuild 0 1 2 3
```

Interactive development-card shortcuts:

- `kn` or `use knight`: choose the robber hex, then choose a player to rob if needed.
- `m` or `use monopoly`: pick a resource.
- `yp` or `use yop`: pick two resources.
- `rb` or `use roadbuild`: choose two road placements consecutively.

## Discarding Cards

When the host asks you to discard cards after a 7, either type five numbers:

```text
0 1 0 2 0
```

or type:

```text
discard
```

Interactive discard mode shows your resource cards, the required total, and a selected discard count under every resource deck. Use Left/Right to choose a resource, Up/Down to change that resource's discard count, Enter to submit, and Esc to cancel. The UI prevents counts below zero or above the cards you hold, and it only submits when the selected total matches the required total.

## Robber

When a 7 is rolled, the CLI asks for a robber hex. Choose it with arrow keys and Enter. If there are multiple legal players to rob on that hex, a player menu appears; use Up/Down and Enter. Knight cards use the same robber and player selection flow.

## Game Ended Screen

When the game ends, the Public panel switches to a bordered final scoreboard while the Field, Personal, and Command panels remain visible. It shows the winner, turns played, each player's total VP, base VP, award VP, piece counts, longest-road length, and knights used. Press Esc to close the CLI window and terminate the child process.
