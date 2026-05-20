use crate::gameplay::{
    constants::costs,
    game::{
        decision::{DecisionId, DecisionKind, OpenDecision},
        engine::lifecycle::{EngineState, FinishedEngine, PlayingEngine, SetupEngine},
        event::GameEvent,
        index::GameIndex,
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
pub enum EngineApplyError {
    WrongState,
    InvalidInitialPlacement,
    InvalidResourceTransfer,
    InvalidBuild,
    InvalidDevCardDraw,
    InvalidDevCardUse,
    InvalidTradeSession,
}

#[inline]
pub fn apply_event(lifecycle: &mut EngineState, event: &GameEvent) -> Result<(), EngineApplyError> {
    match event {
        GameEvent::GameFinished { result, .. } => finish(lifecycle, result),
        GameEvent::GameStarted => game_started(lifecycle),
        GameEvent::CommandRejected {
            counts_toward_limit,
            ..
        } => command_rejected(lifecycle, *counts_toward_limit),
        GameEvent::InitialPlacementBuilt { .. }
        | GameEvent::InitialResourcesGranted { .. }
        | GameEvent::TurnStarted { .. } => apply_setup_or_playing(lifecycle, event),
        _ => apply_current_phase(lifecycle, event),
    }
}

#[inline]
fn command_rejected(
    lifecycle: &mut EngineState,
    counts_toward_limit: bool,
) -> Result<(), EngineApplyError> {
    if counts_toward_limit {
        match lifecycle {
            EngineState::Setup(active) => {
                active.invalid_actions += 1;
                active.stats.action_rejections += 1;
            }
            EngineState::Playing(active) => {
                active.invalid_actions += 1;
                active.stats.action_rejections += 1;
            }
            _ => {}
        }
    }
    Ok(())
}

#[inline]
fn apply_current_phase(
    lifecycle: &mut EngineState,
    event: &GameEvent,
) -> Result<(), EngineApplyError> {
    match lifecycle {
        EngineState::Setup(setup) => apply_setup(setup, event),
        EngineState::Playing(active) => apply_playing(active, event),
        _ => Err(EngineApplyError::WrongState),
    }
}

#[inline]
fn game_started(lifecycle: &mut EngineState) -> Result<(), EngineApplyError> {
    let EngineState::Unstarted(unstarted) = lifecycle else {
        return Err(EngineApplyError::WrongState);
    };
    let mut setup = unstarted.clone().into_setup();
    setup.stats.game_started += 1;
    *lifecycle = EngineState::Setup(setup);
    Ok(())
}

#[inline]
fn apply_setup_or_playing(
    lifecycle: &mut EngineState,
    event: &GameEvent,
) -> Result<(), EngineApplyError> {
    match lifecycle {
        EngineState::Setup(setup) => {
            apply_setup(setup, event)?;
            if let GameEvent::TurnStarted { player_id, .. } = event {
                let mut active = setup.clone().into_playing();
                turn_started(&mut active, *player_id);
                *lifecycle = EngineState::Playing(active);
            }
            Ok(())
        }
        EngineState::Playing(active) => apply_playing(active, event),
        _ => Err(EngineApplyError::WrongState),
    }
}

#[inline]
fn apply_playing(active: &mut PlayingEngine, event: &GameEvent) -> Result<(), EngineApplyError> {
    match event {
        GameEvent::GameStarted => return Err(EngineApplyError::WrongState),
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
fn decision_opened(active: &mut PlayingEngine, decision: &OpenDecision) {
    active.next_decision_id = active.next_decision_id.max(decision.id.0 + 1);
    active.pending.push(decision.clone());

    if !matches!(decision.kind, DecisionKind::InitialPlacement) {
        active.stats.decision_requests += 1;
    }
}

#[inline]
fn decision_closed(active: &mut PlayingEngine, decision_id: DecisionId) {
    active.pending.close(decision_id);
}

#[inline]
fn apply_setup(active: &mut SetupEngine, event: &GameEvent) -> Result<(), EngineApplyError> {
    match event {
        GameEvent::DecisionOpened(decision) => setup_decision_opened(active, decision),
        GameEvent::DecisionClosed { decision_id } => {
            active.pending.close(*decision_id);
        }
        GameEvent::InitialPlacementBuilt {
            player_id,
            settlement,
            road,
        } => setup_initial_placement_built(active, *player_id, *settlement, *road)?,
        GameEvent::InitialResourcesGranted {
            player_id,
            resources,
        } => setup_initial_resources_granted(active, *player_id, *resources)?,
        GameEvent::TurnStarted { .. } => {}
        _ => return Err(EngineApplyError::WrongState),
    }
    Ok(())
}

#[inline]
fn setup_decision_opened(active: &mut SetupEngine, decision: &OpenDecision) {
    active.next_decision_id = active.next_decision_id.max(decision.id.0 + 1);
    active.pending.push(decision.clone());
}

#[inline]
fn setup_initial_placement_built(
    active: &mut SetupEngine,
    player_id: PlayerId,
    settlement: Intersection,
    road: Road,
) -> Result<(), EngineApplyError> {
    active
        .table
        .builds
        .try_init_place(
            player_id,
            road,
            Establishment {
                vtx: settlement,
                stage: EstablishmentType::Settlement,
            },
        )
        .map_err(|_| EngineApplyError::InvalidInitialPlacement)?;
    active.setup_turn.next();
    active.index = GameIndex::rebuild_table(&active.table);
    Ok(())
}

#[inline]
fn setup_initial_resources_granted(
    active: &mut SetupEngine,
    player_id: PlayerId,
    resources: ResourceSet,
) -> Result<(), EngineApplyError> {
    active
        .table
        .transfer_from_bank(resources, player_id)
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)
}

#[inline]
fn initial_placement_built(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    settlement: Intersection,
    road: Road,
) -> Result<(), EngineApplyError> {
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
        .map_err(|_| EngineApplyError::InvalidInitialPlacement)?;
    active.index = GameIndex::rebuild(&active.game);
    Ok(())
}

#[inline]
fn initial_resources_granted(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    resources: ResourceSet,
) -> Result<(), EngineApplyError> {
    active
        .game
        .transfer_from_bank(resources, player_id)
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)
}

#[inline]
fn resources_distributed(active: &mut PlayingEngine, by_player: &[(PlayerId, ResourceSet)]) {
    for (player_id, resources) in by_player {
        let _ = active.game.transfer_from_bank(*resources, *player_id);
    }
    active.stats.resources_distributed += 1;
}

#[inline]
fn dice_rolled(active: &mut PlayingEngine, player_id: PlayerId, value: DiceRoll) {
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
    active: &mut PlayingEngine,
    player_id: PlayerId,
    robbed_id: PlayerId,
    resource: Resource,
) -> Result<(), EngineApplyError> {
    active
        .game
        .players_resource_transfer(robbed_id, player_id, resource.into())
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)
}

#[inline]
fn turn_started(active: &mut PlayingEngine, player_id: PlayerId) {
    active
        .game
        .players
        .get_mut(player_id)
        .dev_cards_reset_queue();
    active.stats.turns_started += 1;
}

#[inline]
fn turn_ended(active: &mut PlayingEngine) {
    active.game.turn.next();
    active.stats.turns_ended += 1;
    active.stats.regular_actions += 1;
}

#[inline]
fn built(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    build: Build,
) -> Result<(), EngineApplyError> {
    active
        .game
        .build(player_id, build)
        .map_err(|_| EngineApplyError::InvalidBuild)?;
    active
        .index
        .refresh_after_build(&active.game, player_id, build);
    active.stats.regular_actions += 1;
    active.stats.builds += 1;
    Ok(())
}

#[inline]
fn dev_card_bought(
    active: &mut PlayingEngine,
    player_id: PlayerId,
) -> Result<(), EngineApplyError> {
    active
        .game
        .transfer_to_bank(costs::DEV_CARD, player_id)
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)?;
    active.stats.regular_actions += 1;
    active.stats.dev_cards_bought += 1;
    Ok(())
}

#[inline]
fn dev_card_drawn(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    card: DevCardKind,
) -> Result<(), EngineApplyError> {
    let drawn = active
        .game
        .bank
        .draw_dev_card()
        .ok_or(EngineApplyError::InvalidDevCardDraw)?;
    if drawn != card {
        return Err(EngineApplyError::InvalidDevCardDraw);
    }
    active.game.players.get_mut(player_id).dev_cards_add(card);
    Ok(())
}

#[inline]
fn dev_card_used(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    usage: &DevCardUsage,
) -> Result<(), EngineApplyError> {
    active
        .game
        .players
        .get_mut(player_id)
        .dev_cards_move_to_used(usage.card_kind())
        .map_err(|_| EngineApplyError::InvalidDevCardUse)?;
    apply_dev_card_usage(active, player_id, usage)?;
    active
        .index
        .refresh_after_dev_card(&active.game, player_id, usage);
    active.stats.dev_cards_used += 1;
    Ok(())
}

#[inline]
fn apply_dev_card_usage(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    usage: &DevCardUsage,
) -> Result<(), EngineApplyError> {
    match usage {
        DevCardUsage::Knight { .. } => Ok(()),
        DevCardUsage::YearOfPlenty(resources) => {
            for resource in resources {
                active
                    .game
                    .transfer_from_bank((*resource).into(), player_id)
                    .map_err(|_| EngineApplyError::InvalidResourceTransfer)?;
            }
            Ok(())
        }
        DevCardUsage::RoadBuild(paths) => {
            for path in paths {
                active
                    .game
                    .builds
                    .try_build(player_id, Build::Road(Road { path: *path }))
                    .map_err(|_| EngineApplyError::InvalidBuild)?;
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
                    .map_err(|_| EngineApplyError::InvalidResourceTransfer)?;
            }
            Ok(())
        }
    }
}

#[inline]
fn bank_trade_completed(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    trade: BankTrade,
) -> Result<(), EngineApplyError> {
    active
        .game
        .trade_with_bank(player_id, trade)
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)?;
    active.stats.regular_actions += 1;
    active.stats.bank_trades += 1;
    Ok(())
}

#[inline]
fn trade_opened(
    active: &mut PlayingEngine,
    session_id: TradeSessionId,
    proposer_id: PlayerId,
    scope: TradeScope,
    offer: PlayerTrade,
) -> Result<(), EngineApplyError> {
    if active.trade_sessions.len() != session_id.0 as usize {
        return Err(EngineApplyError::InvalidTradeSession);
    }
    active.trade_sessions.push(TradeSession::new(
        session_id,
        proposer_id,
        scope,
        offer,
        active.game.players.count(),
    ));
    Ok(())
}

#[inline]
fn trade_offer_added(
    active: &mut PlayingEngine,
    session_id: TradeSessionId,
    player_id: PlayerId,
    offer_id: TradeOfferId,
    offer: PlayerTrade,
) -> Result<(), EngineApplyError> {
    let session = active
        .trade_sessions
        .get_mut(session_id.0 as usize)
        .ok_or(EngineApplyError::InvalidTradeSession)?;
    let added = session.add_counter_offer(player_id, offer);
    if added != offer_id {
        return Err(EngineApplyError::InvalidTradeSession);
    }
    Ok(())
}

#[inline]
fn trade_response_updated(
    active: &mut PlayingEngine,
    session_id: TradeSessionId,
    player_id: PlayerId,
    response: TradeResponseState,
) -> Result<(), EngineApplyError> {
    let session = active
        .trade_sessions
        .get_mut(session_id.0 as usize)
        .ok_or(EngineApplyError::InvalidTradeSession)?;
    session.set_response(player_id, response);
    Ok(())
}

#[inline]
fn trade_completed(
    active: &mut PlayingEngine,
    session_id: TradeSessionId,
    proposer_id: PlayerId,
    peer_id: PlayerId,
    offer_id: TradeOfferId,
) -> Result<(), EngineApplyError> {
    let (proposer_resources, peer_resources) = {
        let session = active
            .trade_sessions
            .get_mut(session_id.0 as usize)
            .ok_or(EngineApplyError::InvalidTradeSession)?;
        let offer = session
            .offer(offer_id)
            .ok_or(EngineApplyError::InvalidTradeSession)?;
        let resources = (offer.trade.give, offer.trade.take);
        session.open = false;
        resources
    };
    active
        .game
        .players_resource_exchange((proposer_id, proposer_resources), (peer_id, peer_resources))
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)?;
    Ok(())
}

#[inline]
fn trade_cancelled(
    active: &mut PlayingEngine,
    session_id: TradeSessionId,
) -> Result<(), EngineApplyError> {
    let session = active
        .trade_sessions
        .get_mut(session_id.0 as usize)
        .ok_or(EngineApplyError::InvalidTradeSession)?;
    session.open = false;
    Ok(())
}

#[inline]
fn player_discarded(
    active: &mut PlayingEngine,
    player_id: PlayerId,
    resources: ResourceSet,
) -> Result<(), EngineApplyError> {
    active
        .game
        .transfer_to_bank(resources, player_id)
        .map_err(|_| EngineApplyError::InvalidResourceTransfer)?;
    if active.pending_discards.first() == Some(&player_id) {
        active.pending_discards.remove(0);
    }
    active.stats.player_discards += 1;
    Ok(())
}

#[inline]
fn robber_moved(active: &mut PlayingEngine, hex: Hex) {
    active.game.board_state.robber_pos = hex;
    active.stats.robber_moves += 1;
}

fn finish(lifecycle: &mut EngineState, result: &GameResult) -> Result<(), EngineApplyError> {
    let mut active = match lifecycle {
        EngineState::Playing(active) => active.clone(),
        EngineState::Setup(setup) => setup.clone().into_playing(),
        _ => return Err(EngineApplyError::WrongState),
    };
    match result {
        GameResult::Win(_) => active.stats.games_ended += 1,
        GameResult::Interrupted { .. } | GameResult::LimitReached { .. } => {
            active.stats.games_interrupted += 1;
        }
    }
    *lifecycle = EngineState::Finished(FinishedEngine {
        game: active.game,
        index: active.index,
        result: result.clone(),
        stats: active.stats,
    });
    Ok(())
}
