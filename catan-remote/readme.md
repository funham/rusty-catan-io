# catan-remote

`catan-remote` hosts games with remote player seats and remote TUI observers.
It reuses runtime bot setup, persistence, and shared match options, but owns all
remote-specific config.

## One TUI player against bots

```sh
scripts/run-cli-single.sh
```

Equivalent manual commands:

```sh
cargo run -p catan-remote --bin catan-remote -- \
  host --config catan-remote/data/configurations/cli_single.json \
  --listen unix://target/catan-remote/cli-single.sock

cargo run -p catan-remote --bin catan-remote -- \
  tui --connect unix://target/catan-remote/cli-single.sock \
  --role player
```

## Remote config

Remote configs can mix remote seats and bots:

```json
{
  "players": [
    { "kind": "remote" },
    { "kind": "lazy" },
    { "kind": "greedy" },
    { "kind": "random" }
  ],
  "observers": []
}
```

TUI observers are configured here, not in runtime:

```json
{ "kind": "tui_omniscient" }
{ "kind": "tui_player", "player_id": 0 }
{ "kind": "tui_snapshot" }
```

The host sends an observer-derived final game summary to connected TUI players
and observers when the match ends.

## Logging

Use `--log` through scripts or set `RUST_LOG` manually:

```sh
scripts/run-cli-single.sh --log warn,catan_remote=debug,catan_runtime=debug
```
