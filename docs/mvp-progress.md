# MVP Progress Notes

Branch: `codex/mvp-hardening`

## Implemented

- Knight dev cards now move the robber, require a valid robbed target when one exists, steal one random resource, and move the card from active to used.
- Invalid dev-card decisions no longer advance the turn flow; the controller re-requests the same decision point.
- Robber blocks resource production on its hex.
- River tiles no longer panic in harvesting or CLI field rendering.
- Remote CLI child has a basic Ratatui alternate-screen command UI.
- Greedy bot priority is covered by tests: city, settlement, dev card, road.

## Useful Commands

```bash
cargo test --workspace
cargo run -p catan-runtime -- catan-runtime/data/configurations/lazy_vs_greedy.json
cargo run -p catan-runtime -- catan-runtime/data/configurations/cli_single.json
```

## CLI Commands

```text
roll
end
buy dev
build road h1 h2
build settlement h1 h2 h3
build city h1 h2 h3
bank-trade give take common
use knight hex [player|none]
use yop res1 res2
use monopoly res
use roadbuild h1 h2 h3 h4
```

## Verification So Far

- `cargo test --workspace` passes.
- `lazy_vs_greedy` headless scenario exits cleanly.

## Remaining Work

- Add cursor/keyboard board selection on top of the Ratatui command UI.
- Improve remote CLI display with the existing field renderer rather than only textual board summaries.
- Replace panic-based remote agent failures with recoverable controller/runtime errors.
- Add controller-level tests for invalid remote/player decisions where practical.
- Revisit player-to-player trades, which are still explicitly not implemented.
