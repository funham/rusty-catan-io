# catan-runtime CLI Agent

`catan-runtime` runs local games and can attach a Ratatui-based CLI agent for human play. The CLI opens in an alternate-screen terminal UI with the board on the left, public game state on the right, personal cards below it, and a command line at the bottom.

## Running a CLI Game

Use one of the runtime configurations that includes a CLI player:

```sh
cargo run -p catan-runtime
```

If a different configuration is needed, pass a JSON file under `catan-runtime/data/configurations/`:

```sh
cargo run -p catan-runtime -- catan-runtime/data/configurations/observer_debug.json
```

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

## Dropping Cards

When the host asks you to drop cards after a 7, either type five numbers:

```text
0 1 0 2 0
```

or type:

```text
drop
```

Interactive drop mode shows your resource cards, the required total, and a selected drop count under every resource deck. Use Left/Right to choose a resource, Up/Down to change that resource's drop count, Enter to submit, and Esc to cancel. The UI prevents counts below zero or above the cards you hold, and it only submits when the selected total matches the required total.

## Robber

When a 7 is rolled, the CLI asks for a robber hex. Choose it with arrow keys and Enter. If there are multiple legal players to rob on that hex, a player menu appears; use Up/Down and Enter. Knight cards use the same robber and player selection flow.
