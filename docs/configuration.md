# Configuration

There are two match config families.

## Runtime configs

Runtime configs live under `catan-runtime/data/configurations/` and are for
pure bot matches. Their `players` array accepts only:

- `{ "kind": "lazy" }`
- `{ "kind": "greedy" }`
- `{ "kind": "random" }`

## Remote configs

Remote configs live under `catan-remote/data/configurations/` and are for games
with remote TUI clients. Their `players` array accepts:

- `{ "kind": "remote" }`
- bot entries accepted by runtime

Remote `observers` accepts TUI observer entries such as `tui_omniscient`,
`tui_player`, and `tui_snapshot`.

## Shared fields

Both config families support:

- `initial`
- `field`
- `dice`
- `limits`
- `logging`
- `persistence`

Relative paths are resolved from the config file directory.
