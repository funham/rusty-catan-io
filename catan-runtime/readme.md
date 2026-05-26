# catan-runtime

`catan-runtime` runs in-process bot matches and simulation-oriented tooling.
It does not create remote or TUI players. Remote seats and TUI observers live in
`catan-remote`.

## Run a bot match

```sh
scripts/run-bots.sh
scripts/run-bots.sh --config catan-runtime/data/configurations/greedy_brawl.json
scripts/run-bots.sh --log warn,catan_runtime=debug
```

## Runtime config

Runtime `players` are bot configs only:

```json
{
  "players": [
    { "kind": "lazy" },
    { "kind": "greedy" },
    { "kind": "random" }
  ]
}
```

Remote seats and TUI or snapshot observer entries are not valid runtime config
options. Use `catan-remote/data/configurations/` for those flows.

## Benchmarks

```sh
cargo run --release --bin catan-bench -- \
  --config catan-runtime/data/configurations/greedy_brawl.json \
  --games 1000 \
  --no-log
```
