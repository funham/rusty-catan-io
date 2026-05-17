use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    algorithm, constants,
    gameplay::game::command::{
        ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
        MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
    },
    gameplay::{
        field::state::{BoardLayout, BoardState},
        game::{
            event::{
                EventCause, EventTransaction, GameEndPlayerStats, GameEvent, ResourceDistribution,
            },
            index::GameIndex,
            init::GameInitializationState,
            lifecycle::{ActiveEngine, EngineCore, FinishedEngine},
            projector,
            query::GameQuery,
            reducer::{self, ReplayError},
            run::{GameResult, GameRunStats, RunOptions},
            state::GameState,
        },
        primitives::{
            PortKind, Tile,
            bank::{BankResourceExchangeError::*, PlayerResourceExchangeError},
            build::{Build, Establishment},
            dev_card::{DevCardUsage, UsableDevCard},
            player::{PlayerId, player_ids},
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
    output::{CommandRejectionReason, GameOutput, OutputSink, VecOutputSink},
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

#[derive(Debug, Clone)]
pub struct EngineTransition {
    pub status: GameStatus,
    pub transaction: EventTransaction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    Replay(ReplayError),
}

impl From<ReplayError> for EngineError {
    fn from(value: ReplayError) -> Self {
        Self::Replay(value)
    }
}

pub struct GameEngine {
    core: EngineCore,
    runtime: EngineRuntime,
}

pub struct EngineRuntime {
    random: GameRandom,
    dice: RandomDiceRoller,
    max_turns: Option<u64>,
    max_invalid_actions: Option<u64>,
    next_tx_id: u64,
    current_tx_id: u64,
}

impl EngineRuntime {
    fn new(options: RunOptions) -> Self {
        Self {
            random: options.random,
            dice: RandomDiceRoller::new(),
            max_turns: options.max_turns,
            max_invalid_actions: options.max_invalid_actions,
            next_tx_id: 1,
            current_tx_id: 0,
        }
    }
}

impl std::ops::Deref for GameEngine {
    type Target = ActiveEngine;

    fn deref(&self) -> &Self::Target {
        self.core
            .as_active()
            .expect("active engine state is required for this operation")
    }
}

impl std::ops::DerefMut for GameEngine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.core
            .active_mut()
            .expect("active engine state is required for this operation")
    }
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
        Self {
            core: EngineCore::active(game),
            runtime: EngineRuntime::new(options),
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
        let core = EngineCore::from_snapshot_parts(
            game.clone(),
            snapshot.phase,
            snapshot.pending.clone(),
            snapshot.next_decision_id,
            snapshot.trade_sessions.clone(),
            snapshot.stats,
            snapshot.invalid_actions,
            snapshot.pending_discards.clone(),
            snapshot.result.clone(),
        );
        let mut runtime = EngineRuntime::new(options);
        runtime.next_tx_id = snapshot.next_tx_id;
        Self { core, runtime }
    }

    pub fn snapshot(&self) -> GameEngineSnapshot {
        match &self.core {
            EngineCore::Active(active) => GameEngineSnapshot {
                schema: "rusty-catan.engine-snapshot.v1".to_owned(),
                state: GameStateSnapshot::from_state(&active.game),
                phase: active.phase,
                pending: active.pending.clone(),
                next_decision_id: active.next_decision_id,
                trade_sessions: active.trade_sessions.clone(),
                stats: active.stats,
                invalid_actions: active.invalid_actions,
                pending_discards: active.pending_discards.clone(),
                result: None,
                next_tx_id: self.runtime.next_tx_id,
            },
            EngineCore::Finished(finished) => GameEngineSnapshot {
                schema: "rusty-catan.engine-snapshot.v1".to_owned(),
                state: GameStateSnapshot::from_state(&finished.game),
                phase: GamePhase::Ended,
                pending: PendingDecisions::default(),
                next_decision_id: 0,
                trade_sessions: SmallVec::new(),
                stats: finished.stats,
                invalid_actions: 0,
                pending_discards: SmallVec::new(),
                result: Some(finished.result.clone()),
                next_tx_id: self.runtime.next_tx_id,
            },
        }
    }

    pub fn set_dice_seed(&mut self, seed: u64) {
        self.runtime.dice = RandomDiceRoller::with_seed(seed);
    }

    pub fn start(&mut self) -> Result<EngineTransition, EngineError> {
        let mut sink = VecOutputSink::default();
        let status = self.start_projected(&mut sink)?;
        Ok(self.transition_from_outputs(EventCause::Start, status, sink.into_vec()))
    }

    fn start_projected(&mut self, sink: &mut impl OutputSink) -> Result<GameStatus, EngineError> {
        if self.phase != GamePhase::NotStarted {
            return Ok(GameStatus::Waiting);
        }
        if self.init.is_some() {
            let tx_id = self.begin_transaction(EventCause::Start);
            let mut transaction =
                crate::gameplay::game::event::EventTransaction::new(tx_id, EventCause::Start);
            transaction.events =
                crate::gameplay::game::decider::decide(&self.core, GameInput::Start);
            for event in &transaction.events {
                reducer::reduce(&mut self.core, event)?;
            }
            for output in projector::project_transaction(&transaction) {
                sink.push(output);
            }
            return Ok(GameStatus::Waiting);
        }
        self.begin_transaction(EventCause::Start);
        self.emit_event(GameEvent::GameStarted, sink);
        self.start_turn(sink);
        Ok(GameStatus::Waiting)
    }

    pub fn apply(&mut self, input: GameInput) -> Result<EngineTransition, EngineError> {
        let mut sink = VecOutputSink::default();
        let cause = match &input {
            GameInput::Start => EventCause::Start,
            GameInput::Submit {
                player_id,
                decision_id,
                ..
            } => EventCause::PlayerCommand {
                player_id: *player_id,
                decision_id: *decision_id,
            },
        };
        if !matches!(input, GameInput::Start) {
            self.begin_transaction(cause.clone());
        }
        let status = self.apply_projected(input, &mut sink)?;
        Ok(self.transition_from_outputs(cause, status, sink.into_vec()))
    }

    fn apply_projected(
        &mut self,
        input: GameInput,
        sink: &mut impl OutputSink,
    ) -> Result<GameStatus, EngineError> {
        if matches!(input, GameInput::Submit { .. }) {
            let context = crate::gameplay::game::decider::DecisionContext {
                max_turns: self.runtime.max_turns,
                dice_roll: self.dice_roll_for_decider(&input),
                stolen_resource: self.stolen_resource_for_decider(&input),
            };
            let events = crate::gameplay::game::decider::decide_with_context(
                &self.core,
                input.clone(),
                context,
            );
            if !events.is_empty() {
                let tx_id = self.runtime.current_tx_id;
                let transaction = EventTransaction {
                    tx_id,
                    cause: match input {
                        GameInput::Submit {
                            player_id,
                            decision_id,
                            ..
                        } => EventCause::PlayerCommand {
                            player_id,
                            decision_id,
                        },
                        GameInput::Start => EventCause::Start,
                    },
                    events,
                };
                for event in &transaction.events {
                    reducer::reduce(&mut self.core, event)?;
                }
                for output in projector::project_transaction(&transaction) {
                    sink.push(output);
                }
                let status = if self.core.result().is_some() {
                    GameStatus::Ended
                } else {
                    GameStatus::Waiting
                };
                return Ok(status);
            }
        }
        Ok(match input {
            GameInput::Start => return self.start_projected(sink),
            GameInput::Submit {
                player_id,
                decision_id,
                command,
            } => self.apply_submit(player_id, decision_id, command, sink),
        })
    }

    fn dice_roll_for_decider(&mut self, input: &GameInput) -> Option<DiceRoll> {
        let GameInput::Submit {
            player_id,
            decision_id,
            command,
        } = input
        else {
            return None;
        };
        let active = self.core.as_active()?;
        let decision = active.pending.get(*decision_id)?;
        if decision.player_id != *player_id {
            return None;
        }
        match (decision.kind, command) {
            (
                DecisionKind::InitCommand,
                PlayerCommand::InitCommand(crate::gameplay::game::command::InitCommand::RollDice),
            )
            | (
                DecisionKind::PostDevCardCommand,
                PlayerCommand::PostDevCard(
                    crate::gameplay::game::command::PostDevCardCommand::RollDice,
                ),
            ) => Some(self.roll_dice()),
            _ => None,
        }
    }

    fn stolen_resource_for_decider(
        &mut self,
        input: &GameInput,
    ) -> Option<crate::gameplay::primitives::resource::Resource> {
        let GameInput::Submit {
            player_id,
            decision_id,
            command,
        } = input
        else {
            return None;
        };
        let active = self.core.as_active()?;
        let decision = active.pending.get(*decision_id)?;
        if decision.player_id != *player_id {
            return None;
        }
        let usage = match (decision.kind, command) {
            (
                DecisionKind::InitCommand,
                PlayerCommand::InitCommand(
                    crate::gameplay::game::command::InitCommand::UseDevCard(usage),
                ),
            )
            | (
                DecisionKind::PostDiceCommand,
                PlayerCommand::PostDice(
                    crate::gameplay::game::command::PostDiceCommand::UseDevCard(usage),
                ),
            ) => usage,
            _ => return None,
        };
        let crate::gameplay::primitives::dev_card::DevCardUsage::Knight {
            robbed_id: Some(robbed_id),
            ..
        } = usage
        else {
            return None;
        };
        let resources = *active.game.players.get(*robbed_id).resources();
        self.runtime
            .random
            .with_rng(|rng| resources.peek_random(rng))
    }

    fn transition_from_outputs(
        &self,
        cause: EventCause,
        status: GameStatus,
        outputs: Vec<GameOutput>,
    ) -> EngineTransition {
        let mut tx_id = self.runtime.current_tx_id;
        let mut events = crate::gameplay::game::event::EventBatch::new();
        for output in outputs {
            if let GameOutput::Event(record) = output {
                tx_id = record.tx_id;
                events.push(record.event);
            }
        }
        EngineTransition {
            status,
            transaction: EventTransaction {
                tx_id,
                cause,
                events,
            },
        }
    }

    pub fn run_stats(&self) -> GameRunStats {
        self.core.stats()
    }

    pub fn result(&self) -> Option<&GameResult> {
        self.core.result()
    }

    pub fn lifecycle(&self) -> &EngineCore {
        &self.core
    }

    pub fn is_started(&self) -> bool {
        self.core
            .as_active()
            .is_none_or(|active| active.phase != GamePhase::NotStarted)
    }

    pub fn pending_decisions(&self) -> impl Iterator<Item = &OpenDecision> {
        self.core
            .as_active()
            .into_iter()
            .flat_map(|active| active.pending.iter())
    }

    pub fn index(&self) -> &GameIndex {
        self.core.index()
    }

    pub fn state(&self) -> &GameState {
        self.core.state()
    }

    pub fn legal_initial_placements(&self, player_id: PlayerId) -> Vec<InitialPlacementCommand> {
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
        if self.core.result().is_some() {
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
            (DecisionKind::InitCommand, PlayerCommand::InitCommand(action)) => {
                self.apply_init_action(decision, action, sink)
            }
            (DecisionKind::PostDiceCommand, PlayerCommand::PostDice(action)) => {
                self.apply_post_dice_action(decision, action, sink)
            }
            (DecisionKind::PostDevCardCommand, PlayerCommand::PostDevCard(action)) => {
                self.apply_post_dev_card_action(decision, action, sink)
            }
            (DecisionKind::RegularCommand, PlayerCommand::Regular(action)) => {
                self.apply_regular_action(decision, action, sink)
            }
            (DecisionKind::RegularCommand, PlayerCommand::Trade(command)) => {
                self.apply_trade_owner_command(decision, command, sink)
            }
            (DecisionKind::TradeResponse { session }, PlayerCommand::Trade(command)) => {
                self.apply_trade_response_command(decision, session, command, sink)
            }
            (DecisionKind::TradeOwnerAction { session }, PlayerCommand::Trade(command)) => {
                self.apply_trade_owner_session_command(decision, session, command, sink)
            }
            (DecisionKind::MoveRobber, PlayerCommand::MoveRobbers(MoveRobberCommand(hex))) => {
                self.apply_move_robber(decision, hex, sink)
            }
            (
                DecisionKind::ChooseRobbedPlayer { robber_pos },
                PlayerCommand::ChooseRobbedPlayer(ChooseRobbedPlayerCommand(robbed_id)),
            ) => self.apply_choose_robbed_player(decision, robber_pos, robbed_id, sink),
            (
                DecisionKind::DropHalf { required },
                PlayerCommand::DropHalf(DropHalfCommand(drop)),
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
        action: InitialPlacementCommand,
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
                    let initial_resources = grant_resources
                        .then(|| self.grant_second_initial_resources(player_id, settlement))
                        .flatten();
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
                    if let Some(resources) = initial_resources {
                        self.emit_event(
                            GameEvent::InitialResourcesGranted {
                                player_id,
                                resources,
                            },
                            sink,
                        );
                    }
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
                let next_player =
                    PlayerId::try_from((player_id.index() + 1) % self.game.players.count())
                        .expect("player count should fit in u8");
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

    fn grant_second_initial_resources(
        &mut self,
        player_id: PlayerId,
        settlement: Establishment,
    ) -> Option<ResourceCollection> {
        let Some(init) = self.init.as_mut() else {
            return None;
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
        Some(resources)
    }

    fn apply_init_action(
        &mut self,
        decision: OpenDecision,
        action: InitCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        match action {
            InitCommand::RollDice => {
                if self.execute_dice_roll(decision.player_id, sink) == GameStatus::Ended {
                    return GameStatus::Ended;
                }
                if matches!(
                    self.phase,
                    GamePhase::Turn(super::phase::TurnPhase::RegularCommand)
                ) {
                    self.open_decision(
                        decision.player_id,
                        DecisionKind::PostDiceCommand,
                        DecisionLifetime::OneShot,
                        sink,
                    );
                    self.phase = GamePhase::Turn(super::phase::TurnPhase::PostDiceCommand);
                }
            }
            InitCommand::UseDevCard(usage) => {
                match self.execute_dev_card(decision.player_id, usage, sink) {
                    Ok(()) => {
                        if self.end_if_won(sink) {
                            return GameStatus::Ended;
                        }
                        self.phase = GamePhase::Turn(super::phase::TurnPhase::PostDevCardCommand);
                        self.open_decision(
                            decision.player_id,
                            DecisionKind::PostDevCardCommand,
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
                            DecisionKind::InitCommand,
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
        action: PostDevCardCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        match action {
            PostDevCardCommand::RollDice => {
                if self.execute_dice_roll(decision.player_id, sink) == GameStatus::Ended {
                    return GameStatus::Ended;
                }
                if matches!(
                    self.phase,
                    GamePhase::Turn(super::phase::TurnPhase::RegularCommand)
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
        action: PostDiceCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        match action {
            PostDiceCommand::UseDevCard(usage) => {
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
                            DecisionKind::PostDiceCommand,
                            DecisionLifetime::OneShot,
                            sink,
                        );
                    }
                }
            }
            PostDiceCommand::RegularCommand(action) => {
                self.apply_regular_action_after_closed(decision.player_id, action, sink);
            }
        }
        GameStatus::Waiting
    }

    fn apply_regular_action(
        &mut self,
        decision: OpenDecision,
        action: RegularCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.close_decision(decision.id, sink);
        self.apply_regular_action_after_closed(decision.player_id, action, sink)
    }

    fn apply_regular_action_after_closed(
        &mut self,
        player_id: PlayerId,
        action: RegularCommand,
        sink: &mut impl OutputSink,
    ) -> GameStatus {
        self.stats.regular_actions += 1;
        match action {
            RegularCommand::Build(build) => match self.execute_build(player_id, build, sink) {
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
            RegularCommand::TradeWithBank(trade) => {
                match self.execute_trade_with_bank(player_id, trade) {
                    Ok(()) => {
                        self.emit_event(GameEvent::BankTradeCompleted { player_id, trade }, sink);
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
            RegularCommand::BuyDevCard => match self.game.buy_dev_card(player_id) {
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
            RegularCommand::OfferPublicTrade(offer) => {
                let decision = self.synthetic_regular_decision(player_id);
                self.open_trade_from_offer(
                    decision,
                    TradeScope::Public,
                    trade_from_public_offer(offer),
                    sink,
                )
            }
            RegularCommand::OfferPersonalTrade(offer) => {
                let (scope, trade) = super::trade::trade_from_personal_offer(offer);
                let decision = self.synthetic_regular_decision(player_id);
                self.open_trade_from_offer(decision, scope, trade, sink)
            }
            RegularCommand::EndMove => {
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
        for player_id in player_ids(self.game.players.count()) {
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
                self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularCommand);
                self.open_decision(
                    session.proposer,
                    DecisionKind::RegularCommand,
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
        self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularCommand);
        self.open_decision(
            proposer_id,
            DecisionKind::RegularCommand,
            DecisionLifetime::OneShot,
            sink,
        );
    }

    fn start_turn(&mut self, sink: &mut impl OutputSink) -> GameStatus {
        let turn_no = self.game.turn.get_turns_played();
        if let Some(max_turns) = self.runtime.max_turns
            && turn_no >= max_turns
        {
            self.phase = GamePhase::Ended;
            self.close_all_decisions(sink);
            let result = GameResult::LimitReached { turns: turn_no };
            self.emit_event(
                GameEvent::GameFinished {
                    result: result.clone(),
                    stats: None,
                },
                sink,
            );
            self.finish_core(result);
            return GameStatus::Ended;
        }
        if self.end_if_won(sink) {
            return GameStatus::Ended;
        }

        let player_id = self.game.turn.get_turn_index();
        self.game.players.get_mut(player_id).dev_cards_reset_queue();
        self.phase = GamePhase::Turn(super::phase::TurnPhase::InitCommand);
        self.emit_event(GameEvent::TurnStarted { player_id, turn_no }, sink);
        self.open_decision(
            player_id,
            DecisionKind::InitCommand,
            DecisionLifetime::OneShot,
            sink,
        );
        GameStatus::Waiting
    }

    fn open_regular_decision(&mut self, player_id: PlayerId, sink: &mut impl OutputSink) {
        self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularCommand);
        self.open_decision(
            player_id,
            DecisionKind::RegularCommand,
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
            kind: DecisionKind::RegularCommand,
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
                self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularCommand);
            }
            DiceOutcome::Seven => self.execute_seven(player_id, sink),
        }
        GameStatus::Waiting
    }

    fn roll_dice(&mut self) -> DiceRoll {
        self.runtime.dice.roll()
    }

    fn execute_harvesting(&mut self, player: PlayerId, num: TileNum) -> ResourceDistribution {
        let by_player = algorithm::resource_distribution_for_roll(&self.game, player, num);
        for (player_id, resources) in &by_player {
            let _ = self.game.transfer_from_bank(*resources, *player_id);
        }
        by_player
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
        algorithm::robbery_candidates(rob_hex, robber_id, &self.game.builds, &self.game.players)
            .collect()
    }

    fn execute_robber_move(
        &mut self,
        player_id: PlayerId,
        target_hex: Hex,
        robbed_id: Option<PlayerId>,
        sink: &mut impl OutputSink,
    ) {
        let result = {
            let active = self
                .core
                .active_mut()
                .expect("robber move requires active engine");
            self.runtime.random.with_rng(|rng| {
                active
                    .game
                    .use_robbers_with_rng(target_hex, player_id, robbed_id, rng)
            })
        };
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
        let stolen = {
            let active = self
                .core
                .active_mut()
                .expect("dev-card usage requires active engine");
            let stolen = self.runtime.random.with_rng(|rng| {
                active
                    .game
                    .use_dev_card_with_rng(usage.clone(), player_id, rng)
            })?;
            active
                .index
                .refresh_after_dev_card(&active.game, player_id, &usage);
            stolen
        };
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
        let active = self
            .core
            .active_mut()
            .expect("build requires active engine");
        active
            .index
            .refresh_after_build(&active.game, player_id, build);
        self.emit_event(GameEvent::Built { player_id, build }, sink);
        Ok(())
    }

    fn execute_trade_with_bank(
        &mut self,
        player_id: PlayerId,
        trade: BankTrade,
    ) -> Result<(), String> {
        let ports = &self.index.ports_acquired[player_id.index()];
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
        self.phase = GamePhase::Ended;
        self.close_all_decisions(sink);
        let result = GameResult::Win(winner);
        self.emit_event(
            GameEvent::GameFinished {
                result: result.clone(),
                stats: Some(stats),
            },
            sink,
        );
        self.finish_core(result);
        true
    }

    fn game_end_stats(&self) -> crate::gameplay::game::event::GameEndStats {
        let query = GameQuery::new(&self.game, &self.index);
        player_ids(self.game.players.count())
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
        let closed = self.pending.close_session(session_id);
        for decision in closed {
            self.emit_event(
                GameEvent::DecisionClosed {
                    decision_id: decision.id,
                },
                sink,
            );
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
        let closed = self.pending.close_all().into_iter().collect::<Vec<_>>();
        for decision in closed {
            self.emit_event(
                GameEvent::DecisionClosed {
                    decision_id: decision.id,
                },
                sink,
            );
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
        if let Some(limit) = self.runtime.max_invalid_actions
            && self.invalid_actions >= limit
        {
            let reason = format!("invalid action limit reached: {limit}");
            self.phase = GamePhase::Ended;
            self.close_all_decisions(sink);
            let result = GameResult::Interrupted { reason };
            self.emit_event(
                GameEvent::GameFinished {
                    result: result.clone(),
                    stats: None,
                },
                sink,
            );
            self.finish_core(result);
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
            self.runtime.current_tx_id,
            event,
        ));
    }

    fn record_event(&mut self, event: &GameEvent) {
        match &mut self.core {
            EngineCore::Active(active) => active.stats.record_event(event),
            EngineCore::Finished(finished) => finished.stats.record_event(event),
        }
    }

    fn begin_transaction(&mut self, _cause: EventCause) -> u64 {
        self.runtime.current_tx_id = self.runtime.next_tx_id;
        self.runtime.next_tx_id += 1;
        self.runtime.current_tx_id
    }

    fn finish_core(&mut self, result: GameResult) {
        let Some(active) = self.core.take_active() else {
            return;
        };
        self.core = EngineCore::Finished(FinishedEngine {
            game: active.game,
            index: active.index,
            result,
            stats: active.stats,
        });
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
    pub fn test_force_regular_action_phase(&mut self, player_id: impl Into<PlayerId>) {
        let player_id = player_id.into();
        self.phase = GamePhase::Turn(super::phase::TurnPhase::RegularCommand);
        self.pending = PendingDecisions::default();
        self.next_decision_id = self.next_decision_id.max(100);
        let mut sink = Vec::new();
        self.open_decision(
            player_id,
            DecisionKind::RegularCommand,
            DecisionLifetime::OneShot,
            &mut sink,
        );
    }

    pub fn test_give_resources(
        &mut self,
        player_id: impl Into<PlayerId>,
        resources: ResourceCollection,
    ) {
        let player_id = player_id.into();
        match self.game.transfer_from_bank(resources, player_id) {
            Ok(()) | Err(BankIsShort) => {}
            Err(AccountIsShort { .. }) => unreachable!(),
        }
    }

    pub fn test_take_resources(
        &mut self,
        player_id: impl Into<PlayerId>,
        resources: ResourceCollection,
    ) {
        let player_id = player_id.into();
        let _ = self.game.transfer_to_bank(resources, player_id);
    }

    pub fn open_decision_for_test(
        &mut self,
        player_id: impl Into<PlayerId>,
        kind: DecisionKind,
    ) -> OpenDecision {
        let player_id = player_id.into();
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
        proposer: impl Into<PlayerId>,
        scope: TradeScope,
        trade: PlayerTrade,
    ) -> TradeSessionId {
        let proposer = proposer.into();
        let id = TradeSessionId(self.trade_sessions.len() as u64);
        let player_count = self.game.players.count();
        self.trade_sessions
            .push(TradeSession::new(id, proposer, scope, trade, player_count));
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
        player_id: impl Into<PlayerId>,
        offer_id: TradeOfferId,
    ) {
        let player_id = player_id.into();
        self.trade_session_mut(session)
            .expect("session should exist")
            .set_response(player_id, TradeResponseState::Accepted { offer_id });
    }

    pub fn test_mark_ended(&mut self) {
        self.phase = GamePhase::Ended;
        self.finish_core(GameResult::Interrupted {
            reason: "test ended".to_owned(),
        });
    }
}

#[cfg(test)]
mod tests;
