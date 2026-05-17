# Event Engine Active Path

## Active path

`SyncGameHost -> GameEngine -> EngineCore -> decider -> reducer -> projector -> runtime observers`

`GameEngine` now owns a single `EngineCore` source of truth instead of mirrored legacy fields plus a
lifecycle copy. The initialized `Start` command is reducer-driven. `GameOutput::Event` carries
transaction metadata and event visibility. Persistence writes committed event records rather than
serialized output envelopes.

## Target path

`SyncGameHost -> GameEngine -> EngineCore -> decider -> reducer -> GameOutput projection`

## Legacy path

Submit command groups still route through legacy `GameEngine` command helpers while emitting
committed event records. These helpers mutate the single active `EngineCore`, so the old duplicated
state mirror is gone, but command decision logic has not yet moved fully into `decider`.

Remaining groups are initial placement, turn lifecycle, regular actions, dev cards,
robber/discard flow, rejection handling, and player trades.

## Retired path

CLI-owned snapshot writing has been retired. Snapshots are host-owned.

Separate state-driving `GameEnded` and `GameInterrupted` events have been retired. Terminal state
is represented by `GameFinished`.

The `GameEngine` duplicate lifecycle mirror has been retired. `EngineCore::Active` and
`EngineCore::Finished` are the current lifecycle facts.

`game::action` is retired as an implementation module and remains only as a compatibility re-export
shim for old external callers. Canonical command payloads live under `game::command`.
