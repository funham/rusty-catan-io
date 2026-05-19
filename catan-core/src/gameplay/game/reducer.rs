use crate::gameplay::{
    constants::costs,
    game::{
        decision::{DecisionId, DecisionKind, OpenDecision},
        event::GameEvent,
        index::GameIndex,
        lifecycle::{ActiveEngine, EngineCore, FinishedEngine},
        phase::{GamePhase, TradePhase, TurnPhase},
        run::GameResult,
        trade::{TradeOfferId, TradeResponseState, TradeScope, TradeSession, TradeSessionId},
    },
    primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        dev_card::{DevCardKind, DevCardUsage},
        player::{PlayerId, player_ids},
        resource::{Resource, ResourceSet},
        trade::{BankTrade, PlayerTrade},
    },
};
use crate::{
    algorithm,
    math::dice::{DiceOutcome, DiceRoll},
    topology::{Hex, Intersection},
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

#[inline]
pub fn reduce(lifecycle: &mut EngineCore, event: &GameEvent) -> Result<(), ReplayError> {
    match event {
        GameEvent::GameFinished { result, .. } => finish(lifecycle, result),
        GameEvent::CommandRejected {
            counts_toward_limit,
            ..
        } => {
            if *counts_toward_limit {
                if let Some(active) = lifecycle.active_mut() {
                    active.invalid_actions += 1;
                    active.stats.action_rejections += 1;
                }
            }
            Ok(())
        }
        _ => {
            let active = lifecycle
                .active_mut()
                .ok_or(ReplayError::ExpectedActiveLifecycle)?;
            reduce_active(active, event)
        }
    }
}

#[inline]
fn reduce_active(active: &mut ActiveEngine, event: &GameEvent) -> Result<(), ReplayError> {
    match event {
        GameEvent::GameStarted => game_started(active),
        GameEvent::DecisionOpened(decision) => decision_opened(active, decision),
        GameEvent::DecisionClosed { decision_id } => decision_closed(active, *decision_id),
        GameEvent::CommandRejected { .. } => {}
        GameEvent::InitialPlacementBuilt {
            player_id,
            settlement,
            road,
        } => initial_placement_built(active, *player_id, *settlement, *road)?,
        GameEvent::InitialResourcesGranted {
            player_id,
            resources,
        } => initial_resources_granted(active, *player_id, *resources)?,
        GameEvent::ResourcesDistributed { by_player } => resources_distributed(active, by_player),
        GameEvent::DiceRolled { player_id, value } => dice_rolled(active, *player_id, *value),
        GameEvent::ResourceStolen {
            player_id,
            robbed_id,
            resource,
        } => resource_stolen(active, *player_id, *robbed_id, *resource)?,
        GameEvent::TurnStarted { player_id, .. } => turn_started(active, *player_id),
        GameEvent::TurnEnded { .. } => turn_ended(active),
        GameEvent::Built { player_id, build } => built(active, *player_id, *build)?,
        GameEvent::DevCardBought { player_id } => dev_card_bought(active, *player_id)?,
        GameEvent::DevCardDrawn { player_id, card } => dev_card_drawn(active, *player_id, *card)?,
        GameEvent::DevCardUsed { player_id, usage } => dev_card_used(active, *player_id, usage)?,
        GameEvent::BankTradeCompleted { player_id, trade } => {
            bank_trade_completed(active, *player_id, *trade)?
        }
        GameEvent::TradeOpened {
            session_id,
            proposer_id,
            scope,
            offer,
            ..
        } => trade_opened(active, *session_id, *proposer_id, *scope, offer.clone())?,
        GameEvent::TradeOfferAdded {
            session_id,
            player_id,
            offer_id,
            offer,
        } => trade_offer_added(active, *session_id, *player_id, *offer_id, offer.clone())?,
        GameEvent::TradeResponseUpdated {
            session_id,
            player_id,
            response,
        } => trade_response_updated(active, *session_id, *player_id, response.clone())?,
        GameEvent::TradeCompleted {
            session_id,
            proposer_id,
            peer_id,
            offer_id,
        } => trade_completed(active, *session_id, *proposer_id, *peer_id, *offer_id)?,
        GameEvent::TradeCancelled { session_id, .. } => trade_cancelled(active, *session_id)?,
        GameEvent::PlayerDiscarded {
            player_id,
            resources,
        } => player_discarded(active, *player_id, *resources)?,
        GameEvent::RobberMoved { hex, .. } => robber_moved(active, *hex),
        GameEvent::GameFinished { .. } => {}
    }
    Ok(())
}

#[inline]
fn game_started(active: &mut ActiveEngine) {
    active.stats.game_started += 1;
}

#[inline]
fn decision_opened(active: &mut ActiveEngine, decision: &OpenDecision) {
    active.next_decision_id = active.next_decision_id.max(decision.id.0 + 1);
    active.pending.push(decision.clone());

    if let Some(phase) = phase_for_decision(decision) {
        active.phase = phase;
    }
    if !matches!(decision.kind, DecisionKind::InitialPlacement) {
        active.stats.decision_requests += 1;
    }
}

#[inline]
fn phase_for_decision(decision: &OpenDecision) -> Option<GamePhase> {
    match decision.kind {
        DecisionKind::InitialPlacement => Some(GamePhase::InitialPlacement),
        DecisionKind::InitCommand => Some(GamePhase::Turn(TurnPhase::InitCommand)),
        DecisionKind::PostDiceCommand => Some(GamePhase::Turn(TurnPhase::PostDiceCommand)),
        DecisionKind::PostDevCardCommand => Some(GamePhase::Turn(TurnPhase::PostDevCardCommand)),
        DecisionKind::RegularCommand => Some(GamePhase::Turn(TurnPhase::RegularCommand)),
        DecisionKind::MoveRobber => Some(GamePhase::Turn(TurnPhase::MoveRobber)),
        DecisionKind::ChooseRobbedPlayer { robber_pos } => {
            Some(GamePhase::Turn(TurnPhase::ChooseRobbedPlayer {
                robber_pos,
            }))
        }
        DecisionKind::DropHalf { required } => Some(GamePhase::Turn(TurnPhase::DropHalf {
            player_id: decision.player_id,
            required,
        })),
        DecisionKind::TradeResponse { .. } | DecisionKind::TradeOwnerAction { .. } => None,
    }
}

#[inline]
fn decision_closed(active: &mut ActiveEngine, decision_id: DecisionId) {
    active.pending.close(decision_id);
}

#[inline]
fn initial_placement_built(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    settlement: Intersection,
    road: Road,
) -> Result<(), ReplayError> {
    active
        .game
        .builds
        .try_init_place(
            player_id,
            road,
            Establishment {
                vtx: settlement,
                stage: EstablishmentType::Settlement,
            },
        )
        .map_err(|_| ReplayError::InvalidInitialPlacement)?;
    if let Some(setup_turn) = active.setup_turn.as_mut() {
        setup_turn.next();
    }
    active.index = GameIndex::rebuild(&active.game);
    Ok(())
}

#[inline]
fn initial_resources_granted(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    resources: ResourceSet,
) -> Result<(), ReplayError> {
    active
        .game
        .transfer_from_bank(resources, player_id)
        .map_err(|_| ReplayError::InvalidResourceTransfer)
}

#[inline]
fn resources_distributed(active: &mut ActiveEngine, by_player: &[(PlayerId, ResourceSet)]) {
    for (player_id, resources) in by_player {
        let _ = active.game.transfer_from_bank(*resources, *player_id);
    }
    active.stats.resources_distributed += 1;
}

#[inline]
fn dice_rolled(active: &mut ActiveEngine, player_id: PlayerId, value: DiceRoll) {
    if matches!(value.resolve(), DiceOutcome::Seven) {
        active.pending_discards =
            algorithm::player_order_from(player_id, active.game.players.count())
                .filter(|pid| active.game.players.get(*pid).resources().total() > 7)
                .collect();
    }
    active.stats.dice_rolls += 1;
}

#[inline]
fn resource_stolen(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    robbed_id: PlayerId,
    resource: Resource,
) -> Result<(), ReplayError> {
    active
        .game
        .players_resource_transfer(robbed_id, player_id, resource.into())
        .map_err(|_| ReplayError::InvalidResourceTransfer)
}

#[inline]
fn turn_started(active: &mut ActiveEngine, player_id: PlayerId) {
    if let Some(setup_turn) = active.setup_turn.take() {
        active.game.turn = setup_turn.into_regular();
        active.index = GameIndex::rebuild(&active.game);
    }
    active
        .game
        .players
        .get_mut(player_id)
        .dev_cards_reset_queue();
    active.stats.turns_started += 1;
}

#[inline]
fn turn_ended(active: &mut ActiveEngine) {
    active.game.turn.next();
    active.stats.turns_ended += 1;
    active.stats.regular_actions += 1;
}

#[inline]
fn built(active: &mut ActiveEngine, player_id: PlayerId, build: Build) -> Result<(), ReplayError> {
    active
        .game
        .build(player_id, build)
        .map_err(|_| ReplayError::InvalidBuild)?;
    active
        .index
        .refresh_after_build(&active.game, player_id, build);
    active.stats.regular_actions += 1;
    active.stats.builds += 1;
    Ok(())
}

#[inline]
fn dev_card_bought(active: &mut ActiveEngine, player_id: PlayerId) -> Result<(), ReplayError> {
    active
        .game
        .transfer_to_bank(costs::DEV_CARD, player_id)
        .map_err(|_| ReplayError::InvalidResourceTransfer)?;
    active.stats.regular_actions += 1;
    active.stats.dev_cards_bought += 1;
    Ok(())
}

#[inline]
fn dev_card_drawn(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    card: DevCardKind,
) -> Result<(), ReplayError> {
    let drawn = active
        .game
        .bank
        .draw_dev_card()
        .ok_or(ReplayError::InvalidDevCardDraw)?;
    if drawn != card {
        return Err(ReplayError::InvalidDevCardDraw);
    }
    active.game.players.get_mut(player_id).dev_cards_add(card);
    Ok(())
}

#[inline]
fn dev_card_used(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    usage: &DevCardUsage,
) -> Result<(), ReplayError> {
    active
        .game
        .players
        .get_mut(player_id)
        .dev_cards_move_to_used(usage.card_kind())
        .map_err(|_| ReplayError::InvalidDevCardUse)?;
    apply_dev_card_usage(active, player_id, usage)?;
    active
        .index
        .refresh_after_dev_card(&active.game, player_id, usage);
    active.stats.dev_cards_used += 1;
    Ok(())
}

#[inline]
fn apply_dev_card_usage(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    usage: &DevCardUsage,
) -> Result<(), ReplayError> {
    match usage {
        DevCardUsage::Knight { .. } => Ok(()),
        DevCardUsage::YearOfPlenty(resources) => {
            for resource in resources {
                active
                    .game
                    .transfer_from_bank((*resource).into(), player_id)
                    .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            }
            Ok(())
        }
        DevCardUsage::RoadBuild(paths) => {
            for path in paths {
                active
                    .game
                    .builds
                    .try_build(player_id, Build::Road(Road { path: *path }))
                    .map_err(|_| ReplayError::InvalidBuild)?;
            }
            Ok(())
        }
        DevCardUsage::Monopoly(resource) => {
            for other_id in player_ids(active.game.players.count()) {
                if other_id == player_id {
                    continue;
                }
                let resources = (
                    *resource,
                    active.game.players.get(other_id).resources()[*resource],
                )
                    .into();
                active
                    .game
                    .players_resource_transfer(other_id, player_id, resources)
                    .map_err(|_| ReplayError::InvalidResourceTransfer)?;
            }
            Ok(())
        }
    }
}

#[inline]
fn bank_trade_completed(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    trade: BankTrade,
) -> Result<(), ReplayError> {
    active
        .game
        .trade_with_bank(player_id, trade)
        .map_err(|_| ReplayError::InvalidResourceTransfer)?;
    active.stats.regular_actions += 1;
    active.stats.bank_trades += 1;
    Ok(())
}

#[inline]
fn trade_opened(
    active: &mut ActiveEngine,
    session_id: TradeSessionId,
    proposer_id: PlayerId,
    scope: TradeScope,
    offer: PlayerTrade,
) -> Result<(), ReplayError> {
    if active.trade_sessions.len() != session_id.0 as usize {
        return Err(ReplayError::InvalidTradeSession);
    }
    active.trade_sessions.push(TradeSession::new(
        session_id,
        proposer_id,
        scope,
        offer,
        active.game.players.count(),
    ));
    active.phase = GamePhase::Trade(TradePhase {
        session: session_id,
    });
    Ok(())
}

#[inline]
fn trade_offer_added(
    active: &mut ActiveEngine,
    session_id: TradeSessionId,
    player_id: PlayerId,
    offer_id: TradeOfferId,
    offer: PlayerTrade,
) -> Result<(), ReplayError> {
    let session = active
        .trade_sessions
        .get_mut(session_id.0 as usize)
        .ok_or(ReplayError::InvalidTradeSession)?;
    let added = session.add_counter_offer(player_id, offer);
    if added != offer_id {
        return Err(ReplayError::InvalidTradeSession);
    }
    Ok(())
}

#[inline]
fn trade_response_updated(
    active: &mut ActiveEngine,
    session_id: TradeSessionId,
    player_id: PlayerId,
    response: TradeResponseState,
) -> Result<(), ReplayError> {
    let session = active
        .trade_sessions
        .get_mut(session_id.0 as usize)
        .ok_or(ReplayError::InvalidTradeSession)?;
    session.set_response(player_id, response);
    Ok(())
}

#[inline]
fn trade_completed(
    active: &mut ActiveEngine,
    session_id: TradeSessionId,
    proposer_id: PlayerId,
    peer_id: PlayerId,
    offer_id: TradeOfferId,
) -> Result<(), ReplayError> {
    let (proposer_resources, peer_resources) = {
        let session = active
            .trade_sessions
            .get_mut(session_id.0 as usize)
            .ok_or(ReplayError::InvalidTradeSession)?;
        let offer = session
            .offer(offer_id)
            .ok_or(ReplayError::InvalidTradeSession)?;
        let resources = (offer.trade.give, offer.trade.take);
        session.open = false;
        resources
    };
    active
        .game
        .players_resource_exchange((proposer_id, proposer_resources), (peer_id, peer_resources))
        .map_err(|_| ReplayError::InvalidResourceTransfer)?;
    active.phase = GamePhase::Turn(TurnPhase::RegularCommand);
    Ok(())
}

#[inline]
fn trade_cancelled(
    active: &mut ActiveEngine,
    session_id: TradeSessionId,
) -> Result<(), ReplayError> {
    let session = active
        .trade_sessions
        .get_mut(session_id.0 as usize)
        .ok_or(ReplayError::InvalidTradeSession)?;
    session.open = false;
    active.phase = GamePhase::Turn(TurnPhase::RegularCommand);
    Ok(())
}

#[inline]
fn player_discarded(
    active: &mut ActiveEngine,
    player_id: PlayerId,
    resources: ResourceSet,
) -> Result<(), ReplayError> {
    active
        .game
        .transfer_to_bank(resources, player_id)
        .map_err(|_| ReplayError::InvalidResourceTransfer)?;
    if active.pending_discards.first() == Some(&player_id) {
        active.pending_discards.remove(0);
    }
    active.stats.player_discards += 1;
    Ok(())
}

#[inline]
fn robber_moved(active: &mut ActiveEngine, hex: Hex) {
    active.game.board_state.robber_pos = hex;
    active.stats.robber_moves += 1;
}

fn finish(lifecycle: &mut EngineCore, result: &GameResult) -> Result<(), ReplayError> {
    let mut active = lifecycle
        .take_active()
        .ok_or(ReplayError::ExpectedActiveLifecycle)?;
    match result {
        GameResult::Win(_) => active.stats.games_ended += 1,
        GameResult::Interrupted { .. } | GameResult::LimitReached { .. } => {
            active.stats.games_interrupted += 1;
        }
    }
    *lifecycle = EngineCore::Finished(FinishedEngine {
        game: active.game,
        index: active.index,
        result: result.clone(),
        stats: active.stats,
    });
    Ok(())
}
