use crate::gameplay::{
    game::{
        event::GameEvent,
        index::GameIndex,
        lifecycle::{EngineCore, FinishedEngine},
        run::GameResult,
    },
    primitives::build::{Establishment, EstablishmentType},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    ExpectedActiveLifecycle,
    InvalidInitialPlacement,
    InvalidResourceTransfer,
    InvalidBuild,
}

pub fn reduce(lifecycle: &mut EngineCore, event: &GameEvent) -> Result<(), ReplayError> {
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
            if matches!(
                decision.kind,
                crate::gameplay::game::decision::DecisionKind::InitPlacement
            ) {
                active.phase = crate::gameplay::game::phase::GamePhase::InitialPlacement;
            }
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
        GameEvent::TurnStarted { player_id, .. } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .players
                .get_mut(*player_id)
                .dev_cards_reset_queue();
            active.stats.turns_started += 1;
        }
        GameEvent::TurnEnded { .. } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active.game.turn.next();
            active.stats.turns_ended += 1;
        }
        GameEvent::Built { player_id, build } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .build(*player_id, *build)
                .map_err(|_| ReplayError::InvalidBuild)?;
            active
                .index
                .refresh_after_build(&active.game, *player_id, *build);
            active.stats.builds += 1;
        }
        GameEvent::Traded { player_id, trade } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .trade_with_bank(*player_id, *trade)
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            active.stats.bank_trades += 1;
        }
        GameEvent::PlayerDiscarded {
            player_id,
            resources,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .transfer_to_bank(*resources, *player_id)
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            active.stats.player_discards += 1;
        }
        GameEvent::RobberMoved { hex, .. } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active.game.board_state.robber_pos = *hex;
            active.stats.robber_moves += 1;
        }
        GameEvent::GameFinished { result, .. } => finish(lifecycle, result.clone())?,
        _ => {}
    }
    Ok(())
}

fn finish(lifecycle: &mut EngineCore, result: GameResult) -> Result<(), ReplayError> {
    let active = lifecycle
        .take_active()
        .ok_or(ReplayError::ExpectedActiveLifecycle)?;
    *lifecycle = EngineCore::Finished(FinishedEngine {
        game: active.game,
        index: active.index,
        result,
        stats: active.stats,
    });
    Ok(())
}
