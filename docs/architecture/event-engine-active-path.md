# Event Engine Active Path

## Active path

`SyncGameHost -> GameEngine -> EngineCore -> decider -> reducer -> projector -> runtime observers`

`GameEngine` now owns a single `EngineCore` source of truth instead of mirrored legacy fields plus a
lifecycle copy. Initialized `Start` and submitted player commands are reducer-driven through
`decider -> reducer`. `GameOutput::Event` carries transaction metadata and event visibility.
Persistence writes committed event records rather than serialized output envelopes.

## Target path

`SyncGameHost -> GameEngine -> EngineCore -> decider -> reducer -> GameOutput projection`

## Legacy path

No submitted player command routes through the legacy imperative helper path.

Remaining cleanup is mechanical: delete obsolete helper functions from `engine.rs`, remove
compatibility aliases, and replace old test-only sink helpers with transaction helpers.

## Retired path

CLI-owned snapshot writing has been retired. Snapshots are host-owned.

Separate state-driving `GameEnded` and `GameInterrupted` events have been retired. Terminal state
is represented by `GameFinished`.

The `GameEngine` duplicate lifecycle mirror has been retired. `EngineCore::Active` and
`EngineCore::Finished` are the current lifecycle facts.

`game::action` is retired as an implementation module and remains only as a compatibility re-export
shim for old external callers. Canonical command payloads live under `game::command`.
