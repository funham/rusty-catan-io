use crate::gameplay::{
    game::{
        event::GameEvent,
        index::GameIndex,
        lifecycle::{EngineLifecycle, FinishedGame},
        run::GameResult,
    },
    primitives::build::{Establishment, EstablishmentType},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    ExpectedActiveLifecycle,
    InvalidInitialPlacement,
    InvalidResourceTransfer,
}

pub fn reduce(lifecycle: &mut EngineLifecycle, event: &GameEvent) -> Result<(), ReplayError> {
    match event {
        GameEvent::GameStarted => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active.stats.game_started += 1;
        }
        GameEvent::DecisionOpened(decision) => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active.next_decision_id = active.next_decision_id.max(decision.id.0 + 1);
            active.pending.push(decision.clone());
            if !matches!(
                decision.kind,
                crate::gameplay::game::decision::DecisionKind::InitPlacement
            ) {
                active.stats.decision_requests += 1;
            }
        }
        GameEvent::DecisionClosed { decision_id } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active.pending.close(*decision_id);
        }
        GameEvent::InitialPlacementBuilt {
            player_id,
            settlement,
            road,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .builds
                .try_init_place(
                    *player_id,
                    *road,
                    Establishment {
                        vtx: *settlement,
                        stage: EstablishmentType::Settlement,
                    },
                )
                .map_err(|_| ReplayError::InvalidInitialPlacement)?;
            active.index = GameIndex::rebuild(&active.game);
        }
        GameEvent::ResourcesDistributed { by_player } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            for (player_id, resources) in by_player {
                let _ = active.game.transfer_from_bank(*resources, *player_id);
            }
            active.stats.resources_distributed += 1;
        }
        GameEvent::ResourceStolen {
            player_id,
            robbed_id,
            resource,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .players_resource_transfer(*robbed_id, *player_id, (*resource).into())
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
        }
        GameEvent::GameFinished { result } => finish(lifecycle, result.clone())?,
        _ => {}
    }
    Ok(())
}

fn finish(lifecycle: &mut EngineLifecycle, result: GameResult) -> Result<(), ReplayError> {
    let active = lifecycle
        .take_active()
        .ok_or(ReplayError::ExpectedActiveLifecycle)?;
    *lifecycle = EngineLifecycle::Finished(FinishedGame {
        game: active.game,
        index: active.index,
        result,
        stats: active.stats,
    });
    Ok(())
}
