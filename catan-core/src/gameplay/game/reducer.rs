use crate::gameplay::{
    constants::costs,
    game::{
        event::GameEvent,
        index::GameIndex,
        lifecycle::{EngineCore, FinishedEngine},
        phase::{GamePhase, TradePhase},
        run::GameResult,
        trade::TradeSession,
    },
    primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        player::player_ids,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    ExpectedActiveLifecycle,
    InvalidInitialPlacement,
    InvalidResourceTransfer,
    InvalidBuild,
    InvalidDevCardDraw,
    InvalidDevCardUse,
    InvalidTradeSession,
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
        GameEvent::CommandRejected {
            counts_toward_limit,
            ..
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            if *counts_toward_limit {
                active.invalid_actions += 1;
                active.stats.action_rejections += 1;
            }
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
        GameEvent::InitialResourcesGranted {
            player_id,
            resources,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .transfer_from_bank(*resources, *player_id)
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
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
        GameEvent::DiceRolled { .. } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active.stats.dice_rolls += 1;
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
        GameEvent::DevCardBought { player_id } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .transfer_to_bank(costs::DEV_CARD, *player_id)
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            active.stats.dev_cards_bought += 1;
        }
        GameEvent::DevCardDrawn { player_id, card } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            let drawn = active
                .game
                .bank
                .draw_dev_card()
                .ok_or(ReplayError::InvalidDevCardDraw)?;
            if drawn != *card {
                return Err(ReplayError::InvalidDevCardDraw);
            }
            active.game.players.get_mut(*player_id).dev_cards_add(*card);
        }
        GameEvent::DevCardUsed { player_id, usage } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .players
                .get_mut(*player_id)
                .dev_cards_move_to_used(usage.card_kind())
                .map_err(|_| ReplayError::InvalidDevCardUse)?;
            match usage {
                crate::gameplay::primitives::dev_card::DevCardUsage::Knight { .. } => {}
                crate::gameplay::primitives::dev_card::DevCardUsage::YearOfPlenty(resources) => {
                    for resource in resources {
                        active
                            .game
                            .transfer_from_bank((*resource).into(), *player_id)
                            .map_err(|_| ReplayError::InvalidResourceTransfer)?;
                    }
                }
                crate::gameplay::primitives::dev_card::DevCardUsage::RoadBuild(paths) => {
                    for path in paths {
                        active
                            .game
                            .builds
                            .try_build(*player_id, Build::Road(Road { path: *path }))
                            .map_err(|_| ReplayError::InvalidBuild)?;
                    }
                }
                crate::gameplay::primitives::dev_card::DevCardUsage::Monopoly(resource) => {
                    for other_id in player_ids(active.game.players.count()) {
                        if other_id == *player_id {
                            continue;
                        }
                        let resources = (
                            *resource,
                            active.game.players.get(other_id).resources()[*resource],
                        )
                            .into();
                        active
                            .game
                            .players_resource_transfer(other_id, *player_id, resources)
                            .map_err(|_| ReplayError::InvalidResourceTransfer)?;
                    }
                }
            }
            active
                .index
                .refresh_after_dev_card(&active.game, *player_id, usage);
            active.stats.dev_cards_used += 1;
        }
        GameEvent::BankTradeCompleted { player_id, trade } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            active
                .game
                .trade_with_bank(*player_id, *trade)
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            active.stats.bank_trades += 1;
        }
        GameEvent::TradeOpened {
            session_id,
            proposer_id,
            scope,
            offer,
            ..
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            if active.trade_sessions.len() != session_id.0 as usize {
                return Err(ReplayError::InvalidTradeSession);
            }
            active.trade_sessions.push(TradeSession::new(
                *session_id,
                *proposer_id,
                *scope,
                offer.clone(),
                active.game.players.count(),
            ));
            active.phase = GamePhase::Trade(TradePhase {
                session: *session_id,
            });
        }
        GameEvent::TradeOfferAdded {
            session_id,
            player_id,
            offer_id,
            offer,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            let session = active
                .trade_sessions
                .get_mut(session_id.0 as usize)
                .ok_or(ReplayError::InvalidTradeSession)?;
            let added = session.add_counter_offer(*player_id, offer.clone());
            if added != *offer_id {
                return Err(ReplayError::InvalidTradeSession);
            }
        }
        GameEvent::TradeResponseUpdated {
            session_id,
            player_id,
            response,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            let session = active
                .trade_sessions
                .get_mut(session_id.0 as usize)
                .ok_or(ReplayError::InvalidTradeSession)?;
            session.set_response(*player_id, response.clone());
        }
        GameEvent::TradeCompleted {
            session_id,
            proposer_id,
            peer_id,
            offer_id,
        } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            let session = active
                .trade_sessions
                .get_mut(session_id.0 as usize)
                .ok_or(ReplayError::InvalidTradeSession)?;
            let offer = session
                .offer(*offer_id)
                .cloned()
                .ok_or(ReplayError::InvalidTradeSession)?;
            session.open = false;
            active
                .game
                .players_resource_exchange(
                    (*proposer_id, offer.trade.give),
                    (*peer_id, offer.trade.take),
                )
                .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            active.phase = GamePhase::Turn(crate::gameplay::game::phase::TurnPhase::RegularCommand);
        }
        GameEvent::TradeCancelled { session_id, .. } => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            let session = active
                .trade_sessions
                .get_mut(session_id.0 as usize)
                .ok_or(ReplayError::InvalidTradeSession)?;
            session.open = false;
            active.phase = GamePhase::Turn(crate::gameplay::game::phase::TurnPhase::RegularCommand);
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
