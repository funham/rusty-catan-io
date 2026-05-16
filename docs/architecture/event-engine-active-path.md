# Event Engine Active Path

## Active path

`SyncGameHost -> GameEngine::start -> EngineLifecycle -> decider -> reducer -> projector -> runtime observers`

The initialized `Start` command is reducer-driven. `GameOutput::Event` carries transaction metadata
and event visibility. Persistence writes committed event records rather than serialized output
envelopes.

## Target path

`SyncGameHost -> GameEngine -> EngineLifecycle -> decider -> reducer -> GameOutput projection`

## Legacy path

Submit command groups still mutate through legacy `GameEngine` helpers while emitting committed
event records. Remaining groups are initial placement, turn lifecycle, regular actions, dev cards,
robber/discard flow, and player trades.

## Retired path

CLI-owned snapshot writing has been retired. Snapshots are host-owned.

Separate state-driving `GameEnded` and `GameInterrupted` events have been retired. Terminal state
is represented by `GameFinished`.
