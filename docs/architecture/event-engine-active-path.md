# Event Engine Active Path

## Active path

`SyncGameHost -> GameEngine -> EngineCore -> decider -> reducer -> projector -> runtime observers`

`GameEngine` now owns a single `EngineCore` source of truth instead of mirrored legacy fields plus a
lifecycle copy. Initialized `Start`, initial placement, regular `EndMove`, valid bank-trade
commands, and non-winning regular builds are reducer-driven. `GameOutput::Event` carries
transaction metadata and event visibility. Persistence writes committed event records rather than
serialized output envelopes.

## Target path

`SyncGameHost -> GameEngine -> EngineCore -> decider -> reducer -> GameOutput projection`

## Legacy path

Submit command groups not listed in the active path still route through legacy `GameEngine` command
helpers while emitting committed event records. These helpers mutate the single active `EngineCore`,
so the old duplicated state mirror is gone, but command decision logic has not yet moved fully into
`decider`.

Remaining groups are dice/harvest/seven turn lifecycle, winning build finish projection, dev-card
purchase/use, robber/discard flow, full rejection handling for migrated commands, and player
trades.

## Retired path

CLI-owned snapshot writing has been retired. Snapshots are host-owned.

Separate state-driving `GameEnded` and `GameInterrupted` events have been retired. Terminal state
is represented by `GameFinished`.

The `GameEngine` duplicate lifecycle mirror has been retired. `EngineCore::Active` and
`EngineCore::Finished` are the current lifecycle facts.

`game::action` is retired as an implementation module and remains only as a compatibility re-export
shim for old external callers. Canonical command payloads live under `game::command`.
