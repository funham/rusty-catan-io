# Event Engine Active Path

## Active path

`SyncGameHost -> GameEngine -> committed GameEventRecord outputs -> runtime observers`

`GameOutput::Event` now carries transaction metadata and event visibility. Persistence writes
committed event records rather than serialized output envelopes.

## Target path

`SyncGameHost -> GameEngine -> EngineLifecycle -> decider -> reducer -> GameOutput projection`

## Legacy path

`GameEngine` still mutates most game state directly through imperative helper methods. The
`EngineLifecycle`, `decider`, and `reducer` modules are present but not yet the only active command
path.

## Retired path

CLI-owned snapshot writing has been retired. Snapshots are host-owned.
