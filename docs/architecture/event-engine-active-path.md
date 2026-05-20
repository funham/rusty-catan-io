# Event Engine Active Path

## Active path

`SyncGameHost -> GameEngine -> EngineState -> decider -> reducer -> projector -> runtime observers`

`GameEngine` now owns a single `EngineState` source of truth instead of mirrored legacy fields plus a
lifecycle copy. Initialized `Start` and submitted player commands are reducer-driven through
`decider -> reducer`. `GameOutput::Event` carries event visibility.
Persistence writes committed event records rather than serialized output envelopes.

## Target path

`SyncGameHost -> GameEngine -> EngineState -> decider -> reducer -> GameOutput projection`

## Legacy path

Empty. Submitted player commands no longer route through the legacy imperative helper path, and
`GameEngine` no longer exposes sink-based apply/start APIs.

## Retired path

CLI-owned snapshot writing has been retired. Snapshots are host-owned.

Separate state-driving `GameEnded` and `GameInterrupted` events have been retired. Terminal state
is represented by `GameFinished`.

The `GameEngine` duplicate lifecycle mirror has been retired. `EngineState` variants are the
current lifecycle facts.

`game::action` is retired. Canonical command payloads live under `game::command`.

`OutputSink`, `VecOutputSink`, `GamePhase::Ended`, `Deref` access to `ActiveEngine`, and
`ActiveEngine::init` have been retired.
