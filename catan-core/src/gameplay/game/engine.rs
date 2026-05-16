use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    constants,
    gameplay::game::action::{
        ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction, MoveRobbersAction,
        PostDevCardAction, PostDiceAction, RegularAction,
    },
    gameplay::{
        field::state::{BoardLayout, BoardState},
        game::{
            event::{EventCause, GameEndPlayerStats, GameEvent, ResourceDistribution},
            index::GameIndex,
            init::GameInitializationState,
            query::GameQuery,
            run::{GameResult, GameRunStats, RunOptions},
            state::GameState,
        },
        primitives::{
            PortKind, Tile,
            bank::{BankResourceExchangeError::*, PlayerResourceExchangeError},
            build::{Build, Establishment},
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::ResourceCollection,
            trade::{BankTrade, BankTradeKind, PlayerTrade},
        },
        random::GameRandom,
    },
    math::dice::{DiceOutcome, DiceRoll, DiceRoller, RandomDiceRoller, TileNum},
    topology::Hex,
};
use smallvec::SmallVec;

use super::{
    decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision, PendingDecisions},
    input::{GameInput, PlayerCommand, TradeCommand, TradeResponseCommand},
    output::{CommandRejectionReason, GameOutput, OutputSink},
    phase::{GamePhase, TradePhase},
    trade::{
        TradeOfferId, TradeResponseState, TradeScope, TradeSession, TradeSessionId,
        trade_from_public_offer, trade_has_overlapping_resources, trade_is_funded,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    Waiting,
    Ended,
}

pub struct GameEngine {
    game: GameState,
    init: Option<GameInitializationState>,
    index: GameIndex,
    phase: GamePhase,
    pending: PendingDecisions,
    next_decision_id: u64,
    trade_sessions: SmallVec<[TradeSession; 16]>,
    stats: GameRunStats,
    random: GameRandom,
    dice: RandomDiceRoller,
    max_turns: Option<u64>,
    max_invalid_actions: Option<u64>,
    invalid_actions: u64,
    pending_discards: SmallVec<[PlayerId; 8]>,
    result: Option<GameResult>,
    next_tx_id: u64,
    current_tx_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateSnapshot {
    pub board_state: BoardState,
    pub turn: crate::gameplay::primitives::turn::GameTurn,
    pub bank: crate::gameplay::primitives::bank::Bank,
    pub players: crate::gameplay::primitives::player::PlayerDataContainer,
    pub builds: crate::gameplay::primitives::build::BoardBuildData,
}

impl GameStateSnapshot {
    pub fn from_state(state: &GameState) -> Self {
        Self {
            board_state: state.board_state.clone(),
            turn: state.turn.clone(),
            bank: state.bank.clone(),
            players: state.players.clone(),
            builds: state.builds.clone(),
        }
    }

    pub fn into_state(self, board: Arc<BoardLayout>) -> GameState {
        GameState {
            board,
            board_state: self.board_state,
            turn: self.turn,
            bank: self.bank,
            players: self.players,
            builds: self.builds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEngineSnapshot {
    pub schema: String,
    pub state: GameStateSnapshot,
    pub phase: GamePhase,
    pub pending: PendingDecisions,
    pub next_decision_id: u64,
    pub trade_sessions: SmallVec<[TradeSession; 16]>,
    pub stats: GameRunStats,
    pub invalid_actions: u64,
    pub pending_discards: SmallVec<[PlayerId; 8]>,
    pub result: Option<GameResult>,
    pub next_tx_id: u64,
}

impl GameEngine {
    pub fn new(game: GameState) -> Self {
        Self::new_with_options(game, RunOptions::default())
    }

    pub fn new_with_options(game: GameState, options: RunOptions) -> Self {
        let index = GameIndex::rebuild(&game);
        Self {
            game,
            init: None,
            index,
            phase: GamePhase::NotStarted,
            pending: PendingDecisions::default(),
            next_decision_id: 0,
            trade_sessions: SmallVec::new(),
            stats: GameRunStats::default(),
            random: options.random,
            dice: RandomDiceRoller::new(),
            max_turns: options.max_turns,
            max_invalid_actions: options.max_invalid_actions,
            invalid_actions: 0,
            pending_discards: SmallVec::new(),
            result: None,
            next_tx_id: 1,
            current_tx_id: 0,
        }
    }

    pub fn from_init(init: GameInitializationState, options: RunOptions) -> Self {
        let game = init.clone().finish();
        let mut engine = Self::new_with_options(game, options);
        engine.init = Some(init);
        engine
    }

    pub fn from_snapshot(
        snapshot: GameEngineSnapshot,
        board: Arc<BoardLayout>,
        options: RunOptions,
    ) -> Self {
        let game = snapshot.state.into_state(board);
        let index = GameIndex::rebuild(&game);
        Self {
            game,
            init: None,
            index,
            phase: snapshot.phase,
            pending: snapshot.pending,
            next_decision_id: snapshot.next_decision_id,
            trade_sessions: snapshot.trade_sessions,
            stats: snapshot.stats,
            random: options.random,
            dice: RandomDiceRoller::new(),
            max_turns: options.max_turns,
            max_invalid_actions: options.max_invalid_actions,
            invalid_actions: snapshot.invalid_actions,
            pending_discards: snapshot.pending_discards,
            result: snapshot.result,
            next_tx_id: snapshot.next_tx_id,
            current_tx_id: 0,
        }
    }

    pub fn snapshot(&self) -> GameEngineSnapshot {
        GameEngineSnapshot {
            schema: "rusty-catan.engine-snapshot.v1".to_owned(),
            state: GameStateSnapshot::from_state(&self.game),
            phase: self.phase,
            pending: self.pending.clone(),
            next_decision_id: self.next_decision_id,
            trade_sessions: self.trade_sessions.clone(),
            stats: self.stats,
            invalid_actions: self.invalid_actions,
            pending_discards: self.pending_discards.clone(),
            result: self.result.clone(),
            next_tx_id: self.next_tx_id,
        }
    }

    pub fn set_dice_seed(&mut self, seed: u64) {
        self.dice = RandomDiceRoller::with_seed(seed);
    }

    pub fn start(&mut self, sink: &mut impl OutputSink) -> GameStatus {
        if self.phase != GamePhase::NotStarted {
            return GameStatus::Waiting;
        }
        self.begin_transaction(EventCause::Start);
        self.emit_event(GameEvent::GameStarted, sink);
        if let Some(init) = &self.init {
            self.phase = GamePhase::InitialPlacement;
            self.open_decision(
                init.turn.get_turn_index(),
                DecisionKind::InitPlacement,
                DecisionLifetime::OneShot,
                sink,
            );
        } else {
            self.start_turn(sink);
        }
        GameStatus::Waiting
    }

    pub fn apply(&mut self, input: GameInput, sink: &mut impl OutputSink) -> GameStatus {
        match input {
            GameInput::Start => self.start(sink),
            GameInput::Submit {
                player_id,
                decision_id,
                command,
            } => {
                self.begin_transaction(EventCause::PlayerCommand {
                    player_id,
                    decision_id,
                });
                self.apply_submit(player_id, decision_id, command, sink)
            }
        }
    }

    pub fn run_stats(&self) -> GameRunStats {
        self.stats
    }

    pub fn result(&self) -> Option<&GameResult> {
        self.result.as_ref()
    }

    pub fn is_started(&self) -> bool {
        self.phase != GamePhase::NotStarted
    }

    pub fn pending_decisions(&self) -> impl Iterator<Item = &OpenDecision> {
        self.pending.iter()
    }

    pub fn index(&self) -> &GameIndex {
        &self.index
    }

    pub fn state(&self) -> &GameState {
        &self.game
    }

    pub fn legal_initial_placements(&self, player_id: PlayerId) -> Vec<InitStageAction> {
        self.game
            .builds
            .query()
            .possible_initial_placements(&self.game.board, player_id)
    }

    fn apply_submit(
        &mut self,
        player_id: PlayerId,
        decision_id: DecisionId,
        command: PlayerCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        if self.result.is_some() {
            self.reject(
                player_id,
                Some(decision_id),
                CommandRejectionReason::GameEnded,
                sink,
            );
            return GameStatus::Ended;
        }

        let Some(decision) = self.pending.get(decision_id).cloned() else {
            self.reject(
                player_id,
                Some(decision_id),
                CommandRejectionReason::StaleDecision,
                sink,
            );
            return GameStatus::Waiting;
        };

        if decision.player_id != player_id {
            self.reject(
                player_id,
                Some(decision_id),
                CommandRejectionReason::WrongPlayer {
                    expected: decision.player_id,
                },
                sink,
            );
            return GameStatus::Waiting;
        }

        match (decision.kind, command) {
            (DecisionKind::InitPlacement, PlayerCommand::InitialPlacement(action)) => {
                self.apply_initial_placement(decision, action, sink)
            }
            (DecisionKind::InitAction, PlayerCommand::InitAction(action)) => {
                self.apply_init_action(decision, action, sink)
            }
            (DecisionKind::PostDiceAction, PlayerCommand::PostDice(action)) => {
                self.apply_post_dice_action(decision, action, sink)
            }
            (DecisionKind::PostDevCardAction, PlayerCommand::PostDevCard(action)) => {
                self.apply_post_dev_card_action(decision, action, sink)
            }
            (DecisionKind::RegularAction, PlayerCommand::Regular(action)) => {
                self.apply_regular_action(decision, action, sink)
            }
            (DecisionKind::RegularAction, PlayerCommand::Trade(command)) => {
                self.apply_trade_owner_command(decision, command, sink)
            }
            (DecisionKind::TradeResponse { session }, PlayerCommand::Trade(command)) => {
                self.apply_trade_response_command(decision, session, command, sink)
            }
            (DecisionKind::TradeOwnerAction { session }, PlayerCommand::Trade(command)) => {
                self.apply_trade_owner_session_command(decision, session, command, sink)
            }
            (DecisionKind::MoveRobber, PlayerCommand::MoveRobbers(MoveRobbersAction(hex))) => {
                self.apply_move_robber(decision, hex, sink)
            }
            (
                DecisionKind::ChooseRobbedPlayer { robber_pos },
                PlayerCommand::ChooseRobbedPlayer(ChoosePlayerToRobAction(robbed_id)),
            ) => self.apply_choose_robbed_player(decision, robber_pos, robbed_id, sink),
            (
                DecisionKind::DropHalf { required },
                PlayerCommand::DropHalf(DropHalfAction(drop)),
            ) => self.apply_drop_half(decision, required, drop, sink),
            _ => {
                self.reject(
                    player_id,
                    Some(decision_id),
                    CommandRejectionReason::WrongPhase,
                    sink,
                );
                GameStatus::Waiting
            }
        }
    }

    fn apply_initial_placement(
        &mut self,
        decision: OpenDecision,
        action: InitStageAction,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        let player_id = decision.player_id;
        let (settlement, road) = action.as_builds();
        if self.init.is_some() {
            let result = self
                .init
                .as_mut()
                .expect("checked above")
                .builds
                .try_init_place(player_id, road, settlement);
            return match result {
                Ok(()) => {
                    let grant_resources = self
                        .init
                        .as_ref()
                        .expect("init state exists")
                        .turn
                        .get_rounds_played()
                        == 1;
                    if grant_resources {
                        self.grant_second_initial_resources(player_id, settlement);
                    }
                    self.game = self
                        .init
                        .as_ref()
                        .expect("init state exists")
                        .clone()
                        .finish();
                    self.index = GameIndex::rebuild(&self.game);
                    self.close_decision(decision.id, sink);
                    self.emit_event(
                        GameEvent::InitialPlacementBuilt {
                            player_id,
                            settlement: settlement.vtx,
                            road,
                        },
                        sink,
                    );
                    let (rounds_played, next_player) = {
                        let init = self.init.as_mut().expect("init state exists");
                        init.turn.next();
                        (init.turn.get_rounds_played(), init.turn.get_turn_index())
                    };
                    if rounds_played < 2 {
                        self.open_decision(
                            next_player,
                            DecisionKind::InitPlacement,
                            DecisionLifetime::OneShot,
                            sink,
                        );
                    } else {
                        let init = self.init.take().expect("init state exists");
                        self.game = init.finish();
                        self.index = GameIndex::rebuild(&self.game);
                        self.start_turn(sink);
                    }
                    GameStatus::Waiting
                }
                Err(err) => {
                    self.record_rejected(
                        player_id,
                        Some(decision.id),
                        format!("invalid initial placement: {err:?}"),
                        sink,
                    );
                    GameStatus::Waiting
                }
            };
        }

        match self.game.builds.try_init_place(player_id, road, settlement) {
            Ok(()) => {
                self.close_decision(decision.id, sink);
                self.index = GameIndex::rebuild(&self.game);
                self.emit_event(
                    GameEvent::InitialPlacementBuilt {
                        player_id,
                        settlement: settlement.vtx,
                        road,
                    },
                    sink,
                );
                let next_player = (player_id + 1) % self.game.players.count();
                self.open_decision(
                    next_player,
                    DecisionKind::InitPlacement,
                    DecisionLifetime::OneShot,
                    sink,
                );
                GameStatus::Waiting
            }
            Err(err) => {
                self.record_rejected(
                    player_id,
                    Some(decision.id),
                    format!("invalid initial placement: {err:?}"),
                    sink,
                );
                GameStatus::Waiting
            }
        }
    }

    fn grant_second_initial_resources(&mut self, player_id: PlayerId, settlement: Establishment) {
        let Some(init) = self.init.as_mut() else {
            return;
        };
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

        let _ = ResourceCollection::transfer(
            &mut init.bank.resources,
            init.players.get_mut(player_id).resources(),
            resources,
        );
    }

    fn apply_init_action(
        &mut self,
        decision: OpenDecision,
        action: InitAction,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        match action {
            InitAction::RollDice => {
                if self.execute_dice_roll(decision.player_id, sink) == GameStatus::Ended {
                    return GameStatus::Ended;
                }
                if matches!(
                    self.phase,
                    GamePhase::Turn(super::phase::TurnPhase::RegularAction)
                ) {
                    self.open_decision(
                        decision.player_id,
                        DecisionKind::PostDiceAction,
                        DecisionLifetime::OneShot,
                        sink,
                    );
                    self.phase = GamePhase::Turn(super::phase::TurnPhase::PostDiceAction);
                }
            }
            InitAction::UseDevCard(usage) => {
                match self.execute_dev_card(decision.player_id, usage, sink) {
                    Ok(()) => {
                        if self.end_if_won(sink) {
                            return GameStatus::Ended;
                        }
                        self.phase = GamePhase::Turn(super::phase::TurnPhase::PostDevCardAction);
                        self.open_decision(
                            decision.player_id,
                            DecisionKind::PostDevCardAction,
                            DecisionLifetime::OneShot,
                            sink,
                        );
                    }
                    Err(err) => {
                        self.record_rejected(
                            decision.player_id,
                            Some(decision.id),
                            format!("invalid dev-card usage: {err:?}"),
                            sink,
                        );
                        self.open_decision(
                            decision.player_id,
                            DecisionKind::InitAction,
                            DecisionLifetime::OneShot,
                            sink,
                        );
                    }
                }
            }
        }
        GameStatus::Waiting
    }

    fn apply_post_dev_card_action(
        &mut self,
        decision: OpenDecision,
        action: PostDevCardAction,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        match action {
            PostDevCardAction::RollDice => {
                if self.execute_dice_roll(decision.player_id, sink) == GameStatus::Ended {
                    return GameStatus::Ended;
                }
                if matches!(
                    self.phase,
                    GamePhase::Turn(super::phase::TurnPhase::RegularAction)
                ) {
                    self.open_regular_decision(decision.player_id, sink);
                }
            }
        }
        GameStatus::Waiting
    }

    fn apply_post_dice_action(
        &mut self,
        decision: OpenDecision,
        action: PostDiceAction,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        match action {
            PostDiceAction::UseDevCard(usage) => {
                match self.execute_dev_card(decision.player_id, usage, sink) {
                    Ok(()) => {
                        if self.end_if_won(sink) {
                            return GameStatus::Ended;
                        }
                        self.open_regular_decision(decision.player_id, sink);
                    }
                    Err(err) => {
                        self.record_rejected(
                            decision.player_id,
                            Some(decision.id),
                            format!("invalid post-dice dev-card usage: {err:?}"),
                            sink,
                        );
                        self.open_decision(
                            decision.player_id,
                            DecisionKind::PostDiceAction,
                            DecisionLifetime::OneShot,
                            sink,
                        );
                    }
                }
            }
            PostDiceAction::RegularAction(action) => {
                self.apply_regular_action_after_closed(decision.player_id, action, sink);
            }
        }
        GameStatus::Waiting
    }

    fn apply_regular_action(
        &mut self,
        decision: OpenDecision,
        action: RegularAction,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        self.apply_regular_action_after_closed(decision.player_id, action, sink)
    }

    fn apply_regular_action_after_closed(
        &mut self,
        player_id: PlayerId,
        action: RegularAction,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.stats.regular_actions += 1;
        match action {
            RegularAction::Build(build) => match self.execute_build(player_id, build, sink) {
                Ok(()) => {
                    if self.end_if_won(sink) {
                        GameStatus::Ended
                    } else {
                        self.open_regular_decision(player_id, sink);
                        GameStatus::Waiting
                    }
                }
                Err(reason) => {
                    self.record_rejected(player_id, None, reason, sink);
                    self.open_regular_decision(player_id, sink);
                    GameStatus::Waiting
                }
            },
            RegularAction::TradeWithBank(trade) => {
                match self.execute_trade_with_bank(player_id, trade) {
                    Ok(()) => {
                        self.emit_event(GameEvent::Traded { player_id, trade }, sink);
                        self.open_regular_decision(player_id, sink);
                        GameStatus::Waiting
                    }
                    Err(reason) => {
                        self.record_rejected(player_id, None, reason, sink);
                        self.open_regular_decision(player_id, sink);
                        GameStatus::Waiting
                    }
                }
            }
            RegularAction::BuyDevCard => match self.game.buy_dev_card(player_id) {
                Ok(card) => {
                    self.emit_event(GameEvent::DevCardBought { player_id }, sink);
                    self.emit_event(GameEvent::DevCardDrawn { player_id, card }, sink);
                    if self.end_if_won(sink) {
                        GameStatus::Ended
                    } else {
                        self.open_regular_decision(player_id, sink);
                        GameStatus::Waiting
                    }
                }
                Err(err) => {
                    self.record_rejected(
                        player_id,
                        None,
                        format!("invalid buy-dev-card action: {err:?}"),
                        sink,
                    );
                    self.open_regular_decision(player_id, sink);
                    GameStatus::Waiting
                }
            },
            RegularAction::OfferPublicTrade(offer) => {
                let decision = self.synthetic_regular_decision(player_id);
                self.open_trade_from_offer(
                    decision,
                    TradeScope::Public,
                    trade_from_public_offer(offer),
                    sink,
                )
            }
            RegularAction::OfferPersonalTrade(offer) => {
                let (scope, trade) = super::trade::trade_from_personal_offer(offer);
                let decision = self.synthetic_regular_decision(player_id);
                self.open_trade_from_offer(decision, scope, trade, sink)
            }
            RegularAction::EndMove => {
                let turn_no = self.game.turn.get_turns_played();
                self.emit_event(GameEvent::TurnEnded { player_id, turn_no }, sink);
                self.game.turn.next();
                self.start_turn(sink);
                GameStatus::Waiting
            }
        }
    }

    fn apply_trade_owner_command(
        &mut self,
        decision: OpenDecision,
        command: TradeCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        match command {
            TradeCommand::Propose { scope, offer } => {
                self.open_trade_from_offer(decision, scope, trade_from_public_offer(offer), sink)
            }
            _ => {
                self.reject(
                    decision.player_id,
                    Some(decision.id),
                    CommandRejectionReason::WrongPhase,
                    sink,
                );
                GameStatus::Waiting
            }
        }
    }

    fn open_trade_from_offer(
        &mut self,
        decision: OpenDecision,
        scope: TradeScope,
        trade: PlayerTrade,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        if trade_has_overlapping_resources(&trade) {
            self.record_rejected(
                decision.player_id,
                Some(decision.id),
                "same resource cannot appear on both sides of a player trade".to_owned(),
                sink,
            );
            return GameStatus::Waiting;
        }
        if let Some(reason) =
            invalid_trade_scope_reason(scope, decision.player_id, self.game.players.count())
        {
            self.record_rejected(decision.player_id, Some(decision.id), reason, sink);
            return GameStatus::Waiting;
        }

        let session_id = TradeSessionId(self.trade_sessions.len() as u64);
        let session = TradeSession::new(
            session_id,
            decision.player_id,
            scope,
            trade.clone(),
            self.game.players.count(),
        );
        let offer_id = session.original_offer_id();
        self.close_decision(decision.id, sink);
        self.trade_sessions.push(session);
        self.phase = GamePhase::Trade(TradePhase {
            session: session_id,
        });
        self.emit_event(
            GameEvent::TradeOpened {
                session_id,
                proposer_id: decision.player_id,
                scope,
                offer_id,
                offer: trade,
            },
            sink,
        );
        self.open_decision(
            decision.player_id,
            DecisionKind::TradeOwnerAction {
                session: session_id,
            },
            DecisionLifetime::UntilSessionClosed(session_id),
            sink,
        );
        for player_id in 0..self.game.players.count() {
            if player_id != decision.player_id && scope.includes(player_id) {
                self.open_decision(
                    player_id,
                    DecisionKind::TradeResponse {
                        session: session_id,
                    },
                    DecisionLifetime::UntilSessionClosed(session_id),
                    sink,
                );
            }
        }
        GameStatus::Waiting
    }

    fn apply_trade_response_command(
        &mut self,
        decision: OpenDecision,
        session_id: TradeSessionId,
        command: TradeCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        let Some(session) = self.trade_session(session_id) else {
            self.reject(
                decision.player_id,
                Some(decision.id),
                CommandRejectionReason::StaleDecision,
                sink,
            );
            return GameStatus::Waiting;
        };
        if !session.open || !session.scope.includes(decision.player_id) {
            self.reject(
                decision.player_id,
                Some(decision.id),
                CommandRejectionReason::WrongPhase,
                sink,
            );
            return GameStatus::Waiting;
        }

        match command {
            TradeCommand::Respond(TradeResponseCommand::Accept { offer_id }) => {
                let Some(offer) = self
                    .trade_session(session_id)
                    .and_then(|session| session.offer(offer_id))
                    .cloned()
                else {
                    self.record_rejected(
                        decision.player_id,
                        Some(decision.id),
                        "unknown trade offer".to_owned(),
                        sink,
                    );
                    return GameStatus::Waiting;
                };
                if let Some(peer) = offer.peer
                    && peer != decision.player_id
                {
                    self.record_rejected(
                        decision.player_id,
                        Some(decision.id),
                        "only the counteroffer owner can accept that counteroffer".to_owned(),
                        sink,
                    );
                    return GameStatus::Waiting;
                }
                let response = TradeResponseState::Accepted { offer_id };
                self.trade_session_mut(session_id)
                    .expect("session existence was checked")
                    .set_response(decision.player_id, response.clone());
                self.emit_event(
                    GameEvent::TradeResponseUpdated {
                        session_id,
                        player_id: decision.player_id,
                        response,
                    },
                    sink,
                );
                GameStatus::Waiting
            }
            TradeCommand::Respond(TradeResponseCommand::Reject) => {
                let response = TradeResponseState::Rejected;
                self.trade_session_mut(session_id)
                    .expect("session existence was checked")
                    .set_response(decision.player_id, response.clone());
                self.emit_event(
                    GameEvent::TradeResponseUpdated {
                        session_id,
                        player_id: decision.player_id,
                        response,
                    },
                    sink,
                );
                GameStatus::Waiting
            }
            TradeCommand::Respond(TradeResponseCommand::Counter { offer }) => {
                if trade_has_overlapping_resources(&offer) {
                    self.record_rejected(
                        decision.player_id,
                        Some(decision.id),
                        "same resource cannot appear on both sides of a player trade".to_owned(),
                        sink,
                    );
                    return GameStatus::Waiting;
                }
                let offer_id = self
                    .trade_session_mut(session_id)
                    .expect("session existence was checked")
                    .add_counter_offer(decision.player_id, offer.clone());
                self.emit_event(
                    GameEvent::TradeOfferAdded {
                        session_id,
                        player_id: decision.player_id,
                        offer_id,
                        offer,
                    },
                    sink,
                );
                let response = TradeResponseState::Countered { offer_id };
                self.trade_session_mut(session_id)
                    .expect("session existence was checked")
                    .set_response(decision.player_id, response.clone());
                self.emit_event(
                    GameEvent::TradeResponseUpdated {
                        session_id,
                        player_id: decision.player_id,
                        response,
                    },
                    sink,
                );
                GameStatus::Waiting
            }
            _ => {
                self.reject(
                    decision.player_id,
                    Some(decision.id),
                    CommandRejectionReason::WrongPhase,
                    sink,
                );
                GameStatus::Waiting
            }
        }
    }

    fn apply_trade_owner_session_command(
        &mut self,
        decision: OpenDecision,
        session_id: TradeSessionId,
        command: TradeCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        match command {
            TradeCommand::Commit { offer_id } => {
                self.commit_trade(decision, session_id, offer_id, sink)
            }
            TradeCommand::Cancel => {
                self.cancel_trade(decision.player_id, session_id, sink);
                GameStatus::Waiting
            }
            _ => {
                self.reject(
                    decision.player_id,
                    Some(decision.id),
                    CommandRejectionReason::WrongPhase,
                    sink,
                );
                GameStatus::Waiting
            }
        }
    }

    fn commit_trade(
        &mut self,
        decision: OpenDecision,
        session_id: TradeSessionId,
        offer_id: TradeOfferId,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        let Some(session) = self.trade_session(session_id).cloned() else {
            self.reject(
                decision.player_id,
                Some(decision.id),
                CommandRejectionReason::StaleDecision,
                sink,
            );
            return GameStatus::Waiting;
        };
        let Some(offer) = session.offer(offer_id).cloned() else {
            self.record_rejected(
                decision.player_id,
                Some(decision.id),
                "unknown trade offer".to_owned(),
                sink,
            );
            return GameStatus::Waiting;
        };
        let Some(peer_id) = offer
            .peer
            .or_else(|| session.accepted_peer_for_offer(offer_id))
        else {
            self.record_rejected(
                decision.player_id,
                Some(decision.id),
                "no player has accepted the selected trade offer".to_owned(),
                sink,
            );
            return GameStatus::Waiting;
        };

        let proposer_resources = self.game.players.get(session.proposer).resources();
        let peer_resources = self.game.players.get(peer_id).resources();
        if !trade_is_funded(proposer_resources, peer_resources, &offer.trade) {
            self.record_rejected(
                decision.player_id,
                Some(decision.id),
                "trade resources are no longer available".to_owned(),
                sink,
            );
            return GameStatus::Waiting;
        }

        match self.execute_player_trade(session.proposer, peer_id, &offer.trade) {
            Ok(()) => {
                self.close_trade_session(session_id, sink);
                self.emit_event(
                    GameEvent::TradeCompleted {
                        session_id,
                        proposer_id: session.proposer,
                        peer_id,
                        offer_id,
                    },
                    sink,
                );
                self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularAction);
                self.open_decision(
                    session.proposer,
                    DecisionKind::RegularAction,
                    DecisionLifetime::OneShot,
                    sink,
                );
            }
            Err(err) => {
                self.record_rejected(
                    decision.player_id,
                    Some(decision.id),
                    format!("failed to execute trade: {err:?}"),
                    sink,
                );
            }
        }
        GameStatus::Waiting
    }

    fn execute_player_trade(
        &mut self,
        proposer: PlayerId,
        peer: PlayerId,
        trade: &PlayerTrade,
    ) -> Result<(), PlayerResourceExchangeError> {
        self.game
            .players_resource_exchange((proposer, trade.give), (peer, trade.take))
    }

    fn cancel_trade(
        &mut self,
        proposer_id: PlayerId,
        session_id: TradeSessionId,
        sink: &mut impl OutputSink,
    ) {
        self.close_trade_session(session_id, sink);
        self.emit_event(
            GameEvent::TradeCancelled {
                session_id,
                proposer_id,
            },
            sink,
        );
        self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularAction);
        self.open_decision(
            proposer_id,
            DecisionKind::RegularAction,
            DecisionLifetime::OneShot,
            sink,
        );
    }

    fn start_turn(&mut self, sink: &mut impl OutputSink) -> GameStatus {
        let turn_no = self.game.turn.get_turns_played();
        if let Some(max_turns) = self.max_turns
            && turn_no >= max_turns
        {
            self.result = Some(GameResult::LimitReached { turns: turn_no });
            self.phase = GamePhase::Ended;
            self.close_all_decisions(sink);
            self.emit_event(
                GameEvent::GameFinished {
                    result: GameResult::LimitReached { turns: turn_no },
                    stats: None,
                },
                sink,
            );
            return GameStatus::Ended;
        }
        if self.end_if_won(sink) {
            return GameStatus::Ended;
        }

        let player_id = self.game.turn.get_turn_index();
        self.game.players.get_mut(player_id).dev_cards_reset_queue();
        self.phase = GamePhase::Turn(super::phase::TurnPhase::InitAction);
        self.emit_event(GameEvent::TurnStarted { player_id, turn_no }, sink);
        self.open_decision(
            player_id,
            DecisionKind::InitAction,
            DecisionLifetime::OneShot,
            sink,
        );
        GameStatus::Waiting
    }

    fn open_regular_decision(&mut self, player_id: PlayerId, sink: &mut impl OutputSink) {
        self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularAction);
        self.open_decision(
            player_id,
            DecisionKind::RegularAction,
            DecisionLifetime::OneShot,
            sink,
        );
    }

    fn synthetic_regular_decision(&mut self, player_id: PlayerId) -> OpenDecision {
        let id = DecisionId(self.next_decision_id);
        self.next_decision_id += 1;
        OpenDecision {
            id,
            player_id,
            kind: DecisionKind::RegularAction,
            lifetime: DecisionLifetime::OneShot,
        }
    }

    fn execute_dice_roll(&mut self, player_id: PlayerId, sink: &mut impl OutputSink) -> GameStatus {
        let roll = self.roll_dice();
        self.emit_event(
            GameEvent::DiceRolled {
                player_id,
                value: roll,
            },
            sink,
        );
        match roll.resolve() {
            DiceOutcome::Harvest(num) => {
                let by_player = self.execute_harvesting(player_id, num);
                self.emit_event(GameEvent::ResourcesDistributed { by_player }, sink);
                self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularAction);
            }
            DiceOutcome::Seven => self.execute_seven(player_id, sink),
        }
        GameStatus::Waiting
    }

    fn roll_dice(&mut self) -> DiceRoll {
        self.dice.roll()
    }

    fn execute_harvesting(&mut self, player: PlayerId, num: TileNum) -> ResourceDistribution {
        let hexes = self.game.board.hexes_by_num(num).clone();
        let player_ids = (player..self.game.players.count()).chain(0..player);
        let mut by_player = ResourceDistribution::new();

        for pid in player_ids {
            for est in self.game.builds[pid].establishments.clone() {
                let coinc = est.vtx.as_set();

                for hex in hexes.iter().filter(|hex| coinc.contains(hex)) {
                    if *hex == self.game.board_state.robber_pos {
                        continue;
                    }
                    if let Tile::Resource { resource, .. } = self.game.board.arrangement[*hex] {
                        let amount = est.stage.harvest_amount() as u16;
                        let resources = (resource, amount).into();
                        if self.game.transfer_from_bank(resources, pid).is_ok() {
                            Self::add_distribution(&mut by_player, pid, resources);
                        }
                    }
                }
            }
        }
        by_player
    }

    fn add_distribution(
        by_player: &mut ResourceDistribution,
        player_id: PlayerId,
        resources: ResourceCollection,
    ) {
        if let Some((_, existing)) = by_player
            .iter_mut()
            .find(|(existing_id, _)| *existing_id == player_id)
        {
            *existing += &resources;
        } else {
            by_player.push((player_id, resources));
        }
    }

    fn execute_seven(&mut self, player_id: PlayerId, sink: &mut impl OutputSink) {
        self.pending_discards = GameQuery::new(&self.game, &self.index)
            .player_ids_starting_from(player_id)
            .into_iter()
            .filter(|pid| self.game.players.get(*pid).resources().total() > 7)
            .collect();
        self.open_next_discard_or_robber(player_id, sink);
    }

    fn open_next_discard_or_robber(&mut self, player_id: PlayerId, sink: &mut impl OutputSink) {
        if let Some(pid) = self.pending_discards.first().copied() {
            let required = self.game.players.get(pid).resources().total() / 2;
            self.phase = GamePhase::Turn(super::phase::TurnPhase::DropHalf {
                player_id: pid,
                required,
            });
            self.open_decision(
                pid,
                DecisionKind::DropHalf { required },
                DecisionLifetime::OneShot,
                sink,
            );
        } else {
            self.phase = GamePhase::Turn(super::phase::TurnPhase::MoveRobber);
            self.open_decision(
                player_id,
                DecisionKind::MoveRobber,
                DecisionLifetime::OneShot,
                sink,
            );
        }
    }

    fn apply_drop_half(
        &mut self,
        decision: OpenDecision,
        required: u16,
        dropped: ResourceCollection,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        let player_id = decision.player_id;
        if dropped.total() != required {
            self.record_rejected(
                player_id,
                Some(decision.id),
                format!("must discard exactly {required} cards"),
                sink,
            );
            return GameStatus::Waiting;
        }
        match self.game.transfer_to_bank(dropped, player_id) {
            Ok(()) => {
                self.close_decision(decision.id, sink);
                self.emit_event(
                    GameEvent::PlayerDiscarded {
                        player_id,
                        resources: dropped,
                    },
                    sink,
                );
                if self.pending_discards.first() == Some(&player_id) {
                    self.pending_discards.remove(0);
                }
                let robber_player = self.game.turn.get_turn_index();
                self.open_next_discard_or_robber(robber_player, sink);
            }
            Err(AccountIsShort { .. }) => {
                self.record_rejected(
                    player_id,
                    Some(decision.id),
                    "discarded resources are not available".to_owned(),
                    sink,
                );
            }
            Err(BankIsShort) => unreachable!("discard deposits into bank"),
        }
        GameStatus::Waiting
    }

    fn apply_move_robber(
        &mut self,
        decision: OpenDecision,
        target_hex: Hex,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        let player_id = decision.player_id;
        if target_hex == self.game.board_state.robber_pos {
            self.record_rejected(
                player_id,
                Some(decision.id),
                "robber must move to a new hex".to_owned(),
                sink,
            );
            return GameStatus::Waiting;
        }
        let candidates = self.robbery_candidates(target_hex, player_id);
        match candidates.as_slice() {
            [] => {
                self.close_decision(decision.id, sink);
                self.execute_robber_move(player_id, target_hex, None, sink);
                self.open_regular_decision(player_id, sink);
            }
            [only] => {
                self.close_decision(decision.id, sink);
                self.execute_robber_move(player_id, target_hex, Some(*only), sink);
                self.open_regular_decision(player_id, sink);
            }
            _ => {
                self.close_decision(decision.id, sink);
                self.phase = GamePhase::Turn(super::phase::TurnPhase::ChooseRobbedPlayer {
                    robber_pos: target_hex,
                });
                self.open_decision(
                    player_id,
                    DecisionKind::ChooseRobbedPlayer {
                        robber_pos: target_hex,
                    },
                    DecisionLifetime::OneShot,
                    sink,
                );
            }
        }
        GameStatus::Waiting
    }

    fn apply_choose_robbed_player(
        &mut self,
        decision: OpenDecision,
        robber_pos: Hex,
        robbed_id: PlayerId,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        let player_id = decision.player_id;
        if !self
            .robbery_candidates(robber_pos, player_id)
            .contains(&robbed_id)
        {
            self.record_rejected(
                player_id,
                Some(decision.id),
                "chosen player cannot be robbed from the selected hex".to_owned(),
                sink,
            );
            return GameStatus::Waiting;
        }
        self.close_decision(decision.id, sink);
        self.execute_robber_move(player_id, robber_pos, Some(robbed_id), sink);
        self.open_regular_decision(player_id, sink);
        GameStatus::Waiting
    }

    fn robbery_candidates(&self, rob_hex: Hex, robber_id: PlayerId) -> Vec<PlayerId> {
        self.game
            .builds
            .query()
            .builds_on_hex(rob_hex)
            .into_iter()
            .filter(|(id, builds)| {
                *id != robber_id
                    && !builds.establishments.is_empty()
                    && !self.game.players.get(*id).resources().is_empty()
            })
            .map(|(id, _)| id)
            .collect()
    }

    fn execute_robber_move(
        &mut self,
        player_id: PlayerId,
        target_hex: Hex,
        robbed_id: Option<PlayerId>,
        sink: &mut impl OutputSink,
    ) {
        let result = self.random.with_rng(|rng| {
            self.game
                .use_robbers_with_rng(target_hex, player_id, robbed_id, rng)
        });
        if let Ok(stolen) = result {
            self.emit_event(
                GameEvent::RobberMoved {
                    player_id,
                    hex: target_hex,
                    robbed_id,
                },
                sink,
            );
            if let (Some(robbed_id), Some(resource)) = (robbed_id, stolen) {
                self.emit_event(
                    GameEvent::ResourceStolen {
                        player_id,
                        robbed_id,
                        resource,
                    },
                    sink,
                );
            }
        }
    }

    fn execute_dev_card(
        &mut self,
        player_id: PlayerId,
        usage: DevCardUsage,
        sink: &mut impl OutputSink,
    ) -> Result<(), crate::gameplay::game::state::DevCardUsageError> {
        let stolen = self.random.with_rng(|rng| {
            self.game
                .use_dev_card_with_rng(usage.clone(), player_id, rng)
        })?;
        self.index
            .refresh_after_dev_card(&self.game, player_id, &usage);
        self.emit_event(
            GameEvent::DevCardUsed {
                player_id,
                usage: usage.clone(),
            },
            sink,
        );
        if let DevCardUsage::Knight { rob_hex, robbed_id } = usage {
            self.emit_event(
                GameEvent::RobberMoved {
                    player_id,
                    hex: rob_hex,
                    robbed_id,
                },
                sink,
            );
            if let (Some(robbed_id), Some(resource)) = (robbed_id, stolen) {
                self.emit_event(
                    GameEvent::ResourceStolen {
                        player_id,
                        robbed_id,
                        resource,
                    },
                    sink,
                );
            }
        }
        Ok(())
    }

    fn execute_build(
        &mut self,
        player_id: PlayerId,
        build: Build,
        sink: &mut impl OutputSink,
    ) -> Result<(), String> {
        self.game
            .build(player_id, build)
            .map_err(|err| format!("invalid build action: {err:?}"))?;
        self.index.refresh_after_build(&self.game, player_id, build);
        self.emit_event(GameEvent::Built { player_id, build }, sink);
        Ok(())
    }

    fn execute_trade_with_bank(
        &mut self,
        player_id: PlayerId,
        trade: BankTrade,
    ) -> Result<(), String> {
        let ports = &self.index.ports_aquired[player_id];
        let required_port = match trade.kind {
            BankTradeKind::BankGeneric => None,
            BankTradeKind::PortGeneric => Some(PortKind::Universal),
            BankTradeKind::PortSpecific => Some(PortKind::Special(trade.give)),
        };
        if let Some(required_port) = required_port
            && !ports.contains(&required_port)
        {
            return Err(format!("missing port {required_port:?}"));
        }
        self.game
            .trade_with_bank(player_id, trade)
            .map_err(|err| format!("invalid bank trade action: {err:?}"))
    }

    fn end_if_won(&mut self, sink: &mut impl OutputSink) -> bool {
        let Some(winner) = GameQuery::new(&self.game, &self.index).check_win_condition() else {
            return false;
        };
        let stats = self.game_end_stats();
        self.result = Some(GameResult::Win(winner));
        self.phase = GamePhase::Ended;
        self.close_all_decisions(sink);
        self.emit_event(
            GameEvent::GameFinished {
                result: GameResult::Win(winner),
                stats: Some(stats),
            },
            sink,
        );
        true
    }

    fn game_end_stats(&self) -> crate::gameplay::game::event::GameEndStats {
        let query = GameQuery::new(&self.game, &self.index);
        (0..self.game.players.count())
            .map(|player_id| {
                let build_and_dev_card_vp = query.count_dev_card_build_vp(player_id);
                let has_longest_road = query.longest_road_owner() == Some(player_id);
                let has_largest_army = query.largest_army_owner() == Some(player_id);
                let award_vp = u16::from(has_longest_road) * constants::LONGEST_ROAD_VP
                    + u16::from(has_largest_army) * constants::LARGEST_ARMY_VP;
                let builds = self.game.builds.by_player(player_id);
                GameEndPlayerStats {
                    player_id,
                    total_vp: build_and_dev_card_vp + award_vp,
                    build_and_dev_card_vp,
                    award_vp,
                    settlements: builds.settlements_count() as u16,
                    cities: builds.cities_count() as u16,
                    roads: builds.roads_count() as u16,
                    longest_road_length: query.count_max_tract_length(player_id),
                    knights_used: self.game.players.get(player_id).dev_cards().used
                        [UsableDevCard::Knight],
                    has_longest_road,
                    has_largest_army,
                }
            })
            .collect()
    }

    fn close_trade_session(&mut self, session_id: TradeSessionId, sink: &mut impl OutputSink) {
        if let Some(session) = self.trade_session_mut(session_id) {
            session.open = false;
        }
        for decision in self.pending.close_session(session_id) {
            sink.push(GameOutput::DecisionClosed {
                decision_id: decision.id,
            });
        }
    }

    fn open_decision(
        &mut self,
        player_id: PlayerId,
        kind: DecisionKind,
        lifetime: DecisionLifetime,
        sink: &mut impl OutputSink,
    ) -> OpenDecision {
        let decision = OpenDecision {
            id: DecisionId(self.next_decision_id),
            player_id,
            kind,
            lifetime,
        };
        self.next_decision_id += 1;
        self.pending.push(decision.clone());
        self.emit_event(GameEvent::DecisionOpened(decision.clone()), sink);
        sink.push(GameOutput::DecisionOpened(decision.clone()));
        decision
    }

    fn close_decision(&mut self, id: DecisionId, sink: &mut impl OutputSink) {
        if self.pending.close(id).is_some() {
            self.emit_event(GameEvent::DecisionClosed { decision_id: id }, sink);
            sink.push(GameOutput::DecisionClosed { decision_id: id });
        }
    }

    fn close_all_decisions(&mut self, sink: &mut impl OutputSink) {
        for decision in self.pending.close_all() {
            sink.push(GameOutput::DecisionClosed {
                decision_id: decision.id,
            });
        }
    }

    fn reject(
        &mut self,
        player_id: PlayerId,
        decision_id: Option<DecisionId>,
        reason: CommandRejectionReason,
        sink: &mut impl OutputSink,
    ) {
        self.reject_with_limit_count(player_id, decision_id, reason, false, sink);
    }

    fn reject_with_limit_count(
        &mut self,
        player_id: PlayerId,
        decision_id: Option<DecisionId>,
        reason: CommandRejectionReason,
        counts_toward_limit: bool,
        sink: &mut impl OutputSink,
    ) {
        self.emit_event(
            GameEvent::CommandRejected {
                player_id,
                decision_id,
                reason: reason.clone(),
                counts_toward_limit,
            },
            sink,
        );
        sink.push(GameOutput::CommandRejected {
            player_id,
            decision_id,
            reason,
        });
    }

    fn record_rejected(
        &mut self,
        player_id: PlayerId,
        decision_id: Option<DecisionId>,
        reason: String,
        sink: &mut impl OutputSink,
    ) {
        self.invalid_actions += 1;
        if let Some(limit) = self.max_invalid_actions
            && self.invalid_actions >= limit
        {
            let reason = format!("invalid action limit reached: {limit}");
            self.result = Some(GameResult::Interrupted {
                reason: reason.clone(),
            });
            self.phase = GamePhase::Ended;
            self.close_all_decisions(sink);
            self.emit_event(
                GameEvent::GameFinished {
                    result: GameResult::Interrupted { reason },
                    stats: None,
                },
                sink,
            );
            return;
        }
        self.reject_with_limit_count(
            player_id,
            decision_id,
            CommandRejectionReason::IllegalCommand(reason),
            true,
            sink,
        );
    }

    fn emit_event(&mut self, event: GameEvent, sink: &mut impl OutputSink) {
        self.record_event(&event);
        sink.push(crate::gameplay::game::projector::project_event(
            self.current_tx_id,
            event,
        ));
    }

    fn record_event(&mut self, event: &GameEvent) {
        self.stats.record_event(event);
    }

    fn begin_transaction(&mut self, _cause: EventCause) {
        self.current_tx_id = self.next_tx_id;
        self.next_tx_id += 1;
    }

    fn trade_session(&self, id: TradeSessionId) -> Option<&TradeSession> {
        self.trade_sessions.get(id.0 as usize)
    }

    fn trade_session_mut(&mut self, id: TradeSessionId) -> Option<&mut TradeSession> {
        self.trade_sessions.get_mut(id.0 as usize)
    }
}

fn invalid_trade_scope_reason(
    scope: TradeScope,
    proposer: PlayerId,
    player_count: usize,
) -> Option<String> {
    match scope {
        TradeScope::Public => None,
        TradeScope::Targeted(peer) if peer >= player_count => {
            Some(format!("targeted trade peer {peer} is out of range"))
        }
        TradeScope::Targeted(peer) if peer == proposer => {
            Some("targeted trade peer cannot be the proposer".to_owned())
        }
        TradeScope::Targeted(_) => None,
    }
}

#[cfg(test)]
impl GameEngine {
    pub fn test_force_regular_action_phase(&mut self, player_id: PlayerId) {
        self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularAction);
        self.pending = PendingDecisions::default();
        self.next_decision_id = self.next_decision_id.max(100);
        let mut sink = Vec::new();
        self.open_decision(
            player_id,
            DecisionKind::RegularAction,
            DecisionLifetime::OneShot,
            &mut sink,
        );
    }

    pub fn test_give_resources(&mut self, player_id: PlayerId, resources: ResourceCollection) {
        match self.game.transfer_from_bank(resources, player_id) {
            Ok(()) | Err(BankIsShort) => {}
            Err(AccountIsShort { .. }) => unreachable!(),
        }
    }

    pub fn test_take_resources(&mut self, player_id: PlayerId, resources: ResourceCollection) {
        let _ = self.game.transfer_to_bank(resources, player_id);
    }

    pub fn open_decision_for_test(
        &mut self,
        player_id: PlayerId,
        kind: DecisionKind,
    ) -> OpenDecision {
        let lifetime = match kind {
            DecisionKind::TradeResponse { session }
            | DecisionKind::TradeOwnerAction { session } => {
                DecisionLifetime::UntilSessionClosed(session)
            }
            _ => DecisionLifetime::OneShot,
        };
        let mut sink = Vec::new();
        self.open_decision(player_id, kind, lifetime, &mut sink)
    }

    pub fn test_open_trade_session(
        &mut self,
        proposer: PlayerId,
        scope: TradeScope,
        trade: PlayerTrade,
    ) -> TradeSessionId {
        let id = TradeSessionId(self.trade_sessions.len() as u64);
        self.trade_sessions.push(TradeSession::new(
            id,
            proposer,
            scope,
            trade,
            self.game.players.count(),
        ));
        id
    }

    pub fn test_trade_original_offer(&self, session: TradeSessionId) -> TradeOfferId {
        self.trade_session(session)
            .expect("session should exist")
            .original_offer_id()
    }

    pub fn test_set_trade_response_accept(
        &mut self,
        session: TradeSessionId,
        player_id: PlayerId,
        offer_id: TradeOfferId,
    ) {
        self.trade_session_mut(session)
            .expect("session should exist")
            .set_response(player_id, TradeResponseState::Accepted { offer_id });
    }

    pub fn test_mark_ended(&mut self) {
        self.result = Some(GameResult::Interrupted {
            reason: "test ended".to_owned(),
        });
        self.phase = GamePhase::Ended;
    }
}

#[cfg(test)]
mod tests;
