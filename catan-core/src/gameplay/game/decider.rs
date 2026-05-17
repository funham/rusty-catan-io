use crate::gameplay::game::{
    decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
    event::{EventBatch, GameEvent},
    input::{GameInput, PlayerCommand},
    lifecycle::EngineCore,
    output::CommandRejectionReason,
    phase::GamePhase,
    run::GameResult,
};
use crate::gameplay::primitives::{Tile, build::Establishment, resource::ResourceCollection};

#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionContext {
    pub max_turns: Option<u64>,
}

pub fn decide(lifecycle: &EngineCore, input: GameInput) -> EventBatch {
    decide_with_context(lifecycle, input, DecisionContext::default())
}

pub fn decide_with_context(
    lifecycle: &EngineCore,
    input: GameInput,
    context: DecisionContext,
) -> EventBatch {
    match input {
        GameInput::Start => decide_start(lifecycle),
        GameInput::Submit {
            player_id,
            decision_id,
            command,
        } => decide_submit(lifecycle, player_id, decision_id, command, context),
    }
}

fn decide_start(lifecycle: &EngineCore) -> EventBatch {
    let mut events = EventBatch::new();
    let Some(active) = lifecycle.as_active() else {
        return events;
    };
    if active.phase != GamePhase::NotStarted {
        return events;
    }

    events.push(GameEvent::GameStarted);
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id: active.game.turn.get_turn_index(),
        kind: DecisionKind::InitPlacement,
        lifetime: DecisionLifetime::OneShot,
    }));
    events
}

fn decide_submit(
    lifecycle: &EngineCore,
    player_id: crate::gameplay::primitives::player::PlayerId,
    decision_id: DecisionId,
    command: PlayerCommand,
    context: DecisionContext,
) -> EventBatch {
    let mut events = EventBatch::new();
    let Some(active) = lifecycle.as_active() else {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision_id),
            reason: CommandRejectionReason::GameEnded,
            counts_toward_limit: false,
        });
        return events;
    };

    let Some(decision) = active.pending.get(decision_id).cloned() else {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision_id),
            reason: CommandRejectionReason::StaleDecision,
            counts_toward_limit: false,
        });
        return events;
    };

    if decision.player_id != player_id {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision_id),
            reason: CommandRejectionReason::WrongPlayer {
                expected: decision.player_id,
            },
            counts_toward_limit: false,
        });
        return events;
    }

    match (decision.kind, command) {
        (DecisionKind::InitPlacement, PlayerCommand::InitialPlacement(command)) => {
            decide_initial_placement(active, decision, command, context, &mut events);
        }
        _ => {}
    }

    events
}

fn decide_initial_placement(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    command: crate::gameplay::game::command::InitialPlacementCommand,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let (settlement, road) = command.as_builds();

    if let Some(init) = &active.init {
        let mut candidate = init.clone();
        if let Err(err) = candidate.builds.try_init_place(player_id, road, settlement) {
            events.push(GameEvent::CommandRejected {
                player_id,
                decision_id: Some(decision.id),
                reason: CommandRejectionReason::IllegalCommand(format!(
                    "invalid initial placement: {err:?}"
                )),
                counts_toward_limit: true,
            });
            return;
        }

        events.push(GameEvent::DecisionClosed {
            decision_id: decision.id,
        });
        events.push(GameEvent::InitialPlacementBuilt {
            player_id,
            settlement: settlement.vtx,
            road,
        });
        if init.turn.get_rounds_played() == 1 {
            let resources = initial_resources(init, settlement);
            if resources != ResourceCollection::ZERO {
                events.push(GameEvent::InitialResourcesGranted {
                    player_id,
                    resources,
                });
            }
        }

        candidate.turn.next();
        if candidate.turn.get_rounds_played() < 2 {
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id: candidate.turn.get_turn_index(),
                kind: DecisionKind::InitPlacement,
                lifetime: DecisionLifetime::OneShot,
            }));
        } else {
            let regular_turn = candidate.turn.into_regular();
            let turn_no = regular_turn.get_turns_played();
            if let Some(max_turns) = context.max_turns
                && turn_no >= max_turns
            {
                events.push(GameEvent::GameFinished {
                    result: GameResult::LimitReached { turns: turn_no },
                    stats: None,
                });
                return;
            }
            events.push(GameEvent::TurnStarted {
                player_id: regular_turn.get_turn_index(),
                turn_no,
            });
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id: active.game.turn.get_turn_index(),
                kind: DecisionKind::InitCommand,
                lifetime: DecisionLifetime::OneShot,
            }));
        }
        return;
    }

    let mut builds = active.game.builds.clone();
    if let Err(err) = builds.try_init_place(player_id, road, settlement) {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision.id),
            reason: CommandRejectionReason::IllegalCommand(format!(
                "invalid initial placement: {err:?}"
            )),
            counts_toward_limit: true,
        });
        return;
    }
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::InitialPlacementBuilt {
        player_id,
        settlement: settlement.vtx,
        road,
    });
}

fn initial_resources(
    init: &crate::gameplay::game::init::GameInitializationState,
    settlement: Establishment,
) -> ResourceCollection {
    let mut resources = ResourceCollection::ZERO;
    for hex in settlement
        .vtx
        .as_set()
        .into_iter()
        .filter(|hex| hex.norm() <= init.board.arrangement.radius() as usize)
    {
        if let Tile::Resource { resource, .. } = init.board.arrangement[hex] {
            resources += &resource.into();
        }
    }
    resources
}
