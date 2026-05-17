use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    algorithm,
    gameplay::{
        field::state::{BoardLayout, BoardState},
        game::{
            command::{self, InitialPlacementCommand},
            decider,
            event::{EventCause, EventTransaction, GameEvent},
            index::GameIndex,
            init::GameInitializationState,
            lifecycle::EngineCore,
            reducer::{self, ReplayError},
            run::{GameResult, GameRunStats, RunOptions},
            state::GameState,
        },
        primitives::{self, player::PlayerId, turn},
        random::GameRandom,
    },
    math::dice::{DiceRoll, DiceRoller, RandomDiceRoller},
};
use smallvec::SmallVec;

use super::{
    decision::{DecisionKind, OpenDecision, PendingDecisions},
    input::{GameInput, PlayerCommand},
    phase::GamePhase,
    trade::TradeSession,
};

#[cfg(test)]
use super::{
    decision::{DecisionId, DecisionLifetime},
    trade::{TradeOfferId, TradeResponseState, TradeScope, TradeSessionId},
};
#[cfg(test)]
use crate::gameplay::game::lifecycle::FinishedEngine;
#[cfg(test)]
use primitives::{
    bank::BankResourceExchangeError, resource::ResourceCollection, trade::PlayerTrade,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateSnapshot {
    pub board_state: BoardState,
    pub turn: turn::GameTurn,
    pub bank: primitives::Bank,
    pub players: primitives::PlayerDataContainer,
    pub builds: primitives::BoardBuildData,
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
    #[serde(default)]
    pub setup_turn: Option<turn::GameTurn<turn::BackAndForthCycle>>,
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
        let setup_turn = init.turn;
        let mut engine = Self::new_with_options(game, options);
        if let Some(active) = engine.core.active_mut() {
            active.setup_turn = Some(setup_turn);
        }
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
            snapshot.setup_turn.clone(),
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
                setup_turn: active.setup_turn.clone(),
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
                phase: GamePhase::NotStarted,
                setup_turn: None,
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
        let tx_id = self.begin_transaction(EventCause::Start);
        let transaction = EventTransaction {
            tx_id,
            cause: EventCause::Start,
            events: decider::decide(&self.core, GameInput::Start),
        };
        for event in &transaction.events {
            reducer::reduce(&mut self.core, event)?;
        }
        Ok(EngineTransition {
            status: self.status(),
            transaction,
        })
    }

    pub fn apply(&mut self, input: GameInput) -> Result<EngineTransition, EngineError> {
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
        let tx_id = self.begin_transaction(cause.clone());
        let events = match input {
            GameInput::Start => decider::decide(&self.core, GameInput::Start),
            submit @ GameInput::Submit { .. } => {
                let context = decider::DecisionContext {
                    max_turns: self.runtime.max_turns,
                    max_invalid_actions: self.runtime.max_invalid_actions,
                    dice_roll: self.dice_roll_for_decider(&submit),
                    stolen_resource: self.stolen_resource_for_decider(&submit),
                };
                decider::decide_with_context(&self.core, submit, context)
            }
        };
        let transaction = EventTransaction {
            tx_id,
            cause,
            events,
        };
        for event in &transaction.events {
            reducer::reduce(&mut self.core, event)?;
        }
        Ok(EngineTransition {
            status: self.status(),
            transaction,
        })
    }

    fn status(&self) -> GameStatus {
        if self.core.result().is_some() {
            GameStatus::Ended
        } else {
            GameStatus::Waiting
        }
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
                PlayerCommand::InitCommand(command::InitCommand::RollDice),
            )
            | (
                DecisionKind::PostDevCardCommand,
                PlayerCommand::PostDevCard(command::PostDevCardCommand::RollDice),
            ) => Some(self.runtime.dice.roll()),
            _ => None,
        }
    }

    fn stolen_resource_for_decider(&mut self, input: &GameInput) -> Option<primitives::Resource> {
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
                PlayerCommand::InitCommand(command::InitCommand::UseDevCard(usage)),
            )
            | (
                DecisionKind::PostDiceCommand,
                PlayerCommand::PostDice(command::PostDiceCommand::UseDevCard(usage)),
            ) => usage,
            _ => {
                let robbed_id = match (decision.kind, command) {
                    (
                        DecisionKind::MoveRobber,
                        PlayerCommand::MoveRobber(command::MoveRobberCommand(hex)),
                    ) => {
                        let mut candidates = algorithm::robbery_candidates(
                            *hex,
                            *player_id,
                            &active.game.builds,
                            &active.game.players,
                        );
                        let only = candidates.next()?;
                        candidates.next().is_none().then_some(only)?
                    }
                    (
                        DecisionKind::ChooseRobbedPlayer { .. },
                        PlayerCommand::ChooseRobbedPlayer(command::ChooseRobbedPlayerCommand(
                            robbed_id,
                        )),
                    ) => *robbed_id,
                    _ => return None,
                };
                let resources = *active.game.players.get(robbed_id).resources();
                return self
                    .runtime
                    .random
                    .with_rng(|rng| resources.peek_random(rng));
            }
        };
        let primitives::DevCardUsage::Knight {
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

    pub fn run_stats(&self) -> GameRunStats {
        self.core.stats()
    }

    pub fn result(&self) -> Option<&GameResult> {
        self.core.result()
    }

    pub fn lifecycle(&self) -> &EngineCore {
        &self.core
    }

    pub fn replay_event(&mut self, event: &GameEvent) -> Result<(), EngineError> {
        reducer::reduce(&mut self.core, event)?;
        Ok(())
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
        let state = self.core.state();
        state
            .builds
            .query()
            .possible_initial_placements(&state.board, player_id)
    }

    fn begin_transaction(&mut self, _cause: EventCause) -> u64 {
        self.runtime.current_tx_id = self.runtime.next_tx_id;
        self.runtime.next_tx_id += 1;
        self.runtime.current_tx_id
    }

    #[cfg(test)]
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

    #[cfg(test)]
    fn trade_session(&self, id: TradeSessionId) -> Option<&TradeSession> {
        self.core
            .as_active()
            .and_then(|active| active.trade_sessions.get(id.0 as usize))
    }
}

#[cfg(test)]
impl GameEngine {
    pub fn test_force_regular_action_phase(&mut self, player_id: impl Into<PlayerId>) {
        let player_id = player_id.into();
        let active = self
            .core
            .active_mut()
            .expect("test engine should be active");
        active.phase = GamePhase::Turn(super::phase::TurnPhase::RegularCommand);
        active.pending = PendingDecisions::default();
        active.next_decision_id = active.next_decision_id.max(100);
        let decision = OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id,
            kind: DecisionKind::RegularCommand,
            lifetime: DecisionLifetime::OneShot,
        };
        active.next_decision_id += 1;
        active.pending.push(decision);
    }

    pub fn test_give_resources(
        &mut self,
        player_id: impl Into<PlayerId>,
        resources: ResourceCollection,
    ) {
        let player_id = player_id.into();
        let active = self
            .core
            .active_mut()
            .expect("test engine should be active");
        match active.game.transfer_from_bank(resources, player_id) {
            Ok(()) | Err(BankResourceExchangeError::BankIsShort) => {}
            Err(BankResourceExchangeError::AccountIsShort { .. }) => unreachable!(),
        }
    }

    pub fn test_take_resources(
        &mut self,
        player_id: impl Into<PlayerId>,
        resources: ResourceCollection,
    ) {
        let player_id = player_id.into();
        let active = self
            .core
            .active_mut()
            .expect("test engine should be active");
        let _ = active.game.transfer_to_bank(resources, player_id);
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
        let active = self
            .core
            .active_mut()
            .expect("test engine should be active");
        let decision = OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id,
            kind,
            lifetime,
        };
        active.next_decision_id += 1;
        active.pending.push(decision.clone());
        decision
    }

    pub fn test_open_trade_session(
        &mut self,
        proposer: impl Into<PlayerId>,
        scope: TradeScope,
        trade: PlayerTrade,
    ) -> TradeSessionId {
        let proposer = proposer.into();
        let active = self
            .core
            .active_mut()
            .expect("test engine should be active");
        let id = TradeSessionId(active.trade_sessions.len() as u64);
        let player_count = active.game.players.count();
        active
            .trade_sessions
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
        let active = self
            .core
            .active_mut()
            .expect("test engine should be active");
        active
            .trade_sessions
            .get_mut(session.0 as usize)
            .expect("session should exist")
            .set_response(player_id, TradeResponseState::Accepted { offer_id });
    }

    pub fn test_mark_ended(&mut self) {
        self.finish_core(GameResult::Interrupted {
            reason: "test ended".to_owned(),
        });
    }
}

#[cfg(test)]
mod tests;
