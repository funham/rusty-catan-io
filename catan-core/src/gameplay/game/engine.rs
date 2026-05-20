pub mod decider;
pub mod lifecycle;
pub mod reducer;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    decision::{DecisionKind, OpenDecision},
    input::{DecisionResponse, GameInput, PlayerCommand},
};
#[cfg(test)]
use crate::gameplay::game::engine::lifecycle::PlayingEngine;
use crate::{
    algorithm,
    gameplay::{
        field::state::{BoardLayout, BoardState},
        game::{
            command::{self, InitialPlacementCommand},
            engine::{lifecycle::EngineState, reducer::EngineApplyError},
            event::{EventCause, EventTransaction, GameEvent},
            index::GameIndex,
            run::{GameResult, RunOptions},
            state::{GameState, SetupGameState, TableState},
        },
        primitives::{self, player::PlayerId, turn},
        random::GameRandom,
    },
    math::dice::DiceRoll,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    Apply(EngineApplyError),
}

impl From<EngineApplyError> for EngineError {
    fn from(value: EngineApplyError) -> Self {
        Self::Apply(value)
    }
}

pub struct GameEngine {
    core: EngineState,
    runtime: EngineRuntime,
}

pub struct EngineRuntime {
    random: GameRandom,
    max_turns: Option<u64>,
    max_invalid_actions: Option<u64>,
}

impl EngineRuntime {
    fn new(options: RunOptions) -> Self {
        Self {
            random: options.random,
            max_turns: options.max_turns,
            max_invalid_actions: options.max_invalid_actions,
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
            board_state: state.board_state,
            turn: state.turn.clone(),
            bank: state.bank.clone(),
            players: state.players.clone(),
            builds: state.builds.clone(),
        }
    }

    pub fn into_state(self, board: Arc<BoardLayout>) -> GameState {
        GameState {
            table: TableState {
                board,
                board_state: self.board_state,
                bank: self.bank,
                players: self.players,
                builds: self.builds,
            },
            turn: self.turn,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEngineSnapshot {
    pub schema: String,
    pub state: EngineState,
}

impl GameEngine {
    fn apply_event_to_core(&mut self, event: &GameEvent) -> Result<(), EngineError> {
        reducer::apply_event(&mut self.core, event)?;
        Ok(())
    }

    pub fn new(game: GameState) -> Self {
        Self::new_with_options(game, RunOptions::default())
    }

    pub fn new_with_options(game: GameState, options: RunOptions) -> Self {
        Self {
            core: EngineState::playing(game),
            runtime: EngineRuntime::new(options),
        }
    }

    pub fn from_init(init: SetupGameState, options: RunOptions) -> Self {
        Self {
            core: EngineState::unstarted(init),
            runtime: EngineRuntime::new(options),
        }
    }

    pub fn from_snapshot(
        snapshot: GameEngineSnapshot,
        board: Arc<BoardLayout>,
        options: RunOptions,
    ) -> Self {
        let _ = board;
        let mut core = snapshot.state;
        core.rebuild_indexes();
        Self {
            core,
            runtime: EngineRuntime::new(options),
        }
    }

    pub fn snapshot(&self) -> GameEngineSnapshot {
        GameEngineSnapshot {
            schema: "rusty-catan.engine-snapshot.v2".to_owned(),
            state: self.core.clone(),
        }
    }

    pub fn start(&mut self) -> Result<EventTransaction, EngineError> {
        let transaction = EventTransaction {
            cause: EventCause::Start,
            events: decider::decide(&self.core, GameInput::Start),
        };
        for event in &transaction.events {
            self.apply_event_to_core(event)?;
        }
        Ok(transaction)
    }

    pub fn apply(&mut self, input: GameInput) -> Result<EventTransaction, EngineError> {
        let cause = match &input {
            GameInput::Start => EventCause::Start,
            GameInput::Submit(DecisionResponse { token, .. }) => EventCause::PlayerCommand(*token),
        };
        let events = match input {
            GameInput::Start => decider::decide(&self.core, GameInput::Start),
            submit @ GameInput::Submit(_) => {
                let context = decider::DecisionContext {
                    max_turns: self.runtime.max_turns,
                    max_invalid_actions: self.runtime.max_invalid_actions,
                    dice_roll: self.dice_roll_for_decider(&submit),
                    stolen_resource: self.stolen_resource_for_decider(&submit),
                };
                decider::decide_with_context(&self.core, submit, context)
            }
        };
        let transaction = EventTransaction { cause, events };
        for event in &transaction.events {
            self.apply_event_to_core(event)?;
        }
        Ok(transaction)
    }

    pub fn submit(&mut self, response: DecisionResponse) -> Result<EventTransaction, EngineError> {
        self.apply(GameInput::Submit(response))
    }

    fn dice_roll_for_decider(&mut self, input: &GameInput) -> Option<DiceRoll> {
        let GameInput::Submit(response) = input else {
            return None;
        };
        let DecisionResponse { token, command } = response.clone();

        let EngineState::Playing(active) = &self.core else {
            return None;
        };
        let decision = active.pending.get(token.id)?;
        if decision.player_id != token.player_id {
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
            ) => Some(self.runtime.random.roll_dice()),
            _ => None,
        }
    }

    fn stolen_resource_for_decider(&mut self, input: &GameInput) -> Option<primitives::Resource> {
        let GameInput::Submit(response) = input.clone() else {
            return None;
        };
        let DecisionResponse { token, command } = response;

        let EngineState::Playing(active) = &self.core else {
            return None;
        };
        let decision = active.pending.get(token.id)?;
        if decision.player_id != token.player_id {
            return None;
        }
        let usage = match (decision.kind, &command) {
            (
                DecisionKind::InitCommand,
                PlayerCommand::InitCommand(command::InitCommand::UseDevCard(usage)),
            )
            | (
                DecisionKind::PostDiceCommand,
                PlayerCommand::PostDice(command::PostDiceCommand::UseDevCard(usage)),
            ) => usage,
            _ => {
                let robbed_id = match (decision.kind, &command) {
                    (
                        DecisionKind::MoveRobber,
                        PlayerCommand::MoveRobber(command::MoveRobberCommand(hex)),
                    ) => {
                        let mut candidates = algorithm::robbery_candidates(
                            *hex,
                            token.player_id,
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
                return self.runtime.random.pick_resource(&resources);
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
        self.runtime.random.pick_resource(&resources)
    }

    pub fn result(&self) -> Option<&GameResult> {
        self.core.result()
    }

    pub fn lifecycle(&self) -> &EngineState {
        &self.core
    }

    pub fn apply_event(&mut self, event: &GameEvent) -> Result<(), EngineError> {
        self.apply_event_to_core(event)
    }

    pub fn is_started(&self) -> bool {
        !matches!(self.core, EngineState::Unstarted(_))
    }

    pub fn pending_decisions(&self) -> impl Iterator<Item = &OpenDecision> {
        let pending = match &self.core {
            EngineState::Setup(setup) => Some(&setup.pending),
            EngineState::Playing(active) => Some(&active.pending),
            _ => None,
        };
        pending.into_iter().flat_map(|pending| pending.iter())
    }

    pub fn index(&self) -> &GameIndex {
        self.core.index()
    }

    pub fn state(&self) -> &GameState {
        self.core
            .game()
            .expect("full GameState exists only after setup completes")
    }

    pub fn table(&self) -> &TableState {
        self.core.table()
    }

    pub fn legal_initial_placements(&self, player_id: PlayerId) -> Vec<InitialPlacementCommand> {
        let state = self.core.table();
        state
            .builds
            .query()
            .possible_initial_placements(&state.board, player_id)
    }

    #[cfg(test)]
    fn finish_core(&mut self, result: GameResult) {
        if let EngineState::Playing(active) = &self.core {
            use crate::gameplay::game::engine::lifecycle::FinishedEngine;

            self.core = EngineState::Finished(Box::new(FinishedEngine {
                game: active.game.clone(),
                index: active.index.clone(),
                result,
            }));
        }
    }

    #[cfg(test)]
    fn trade_session(&self, id: TradeSessionId) -> Option<&TradeSession> {
        let EngineState::Playing(active) = &self.core else {
            return None;
        };
        active.trade_sessions.get(id.0 as usize)
    }
}

/* ------------ TESTING INFRASTRUCTURE ------------ */

#[cfg(test)]
use super::{
    decision::{DecisionId, DecisionLifetime, PendingDecisions},
    trade::{TradeOfferId, TradeResponseState, TradeScope, TradeSession, TradeSessionId},
};

#[cfg(test)]
use primitives::{bank::BankResourceExchangeError, resource::ResourceSet, trade::PlayerTrade};

#[cfg(test)]
impl GameEngine {
    fn force_playing_for_tests(&mut self) {
        let next = match &self.core {
            EngineState::Unstarted(unstarted) => Some(EngineState::Playing(Box::new(
                unstarted.as_ref().clone().into_setup().into_playing(),
            ))),
            EngineState::Setup(setup) => Some(EngineState::Playing(Box::new(
                setup.as_ref().clone().into_playing(),
            ))),
            _ => None,
        };
        if let Some(next) = next {
            self.core = next;
        }
    }

    fn playing_mut_for_tests(&mut self) -> &mut PlayingEngine {
        self.force_playing_for_tests();
        let EngineState::Playing(active) = &mut self.core else {
            unreachable!("test engine should be active");
        };
        active
    }

    pub fn test_force_regular_action_phase(&mut self, player_id: impl Into<PlayerId>) {
        let player_id = player_id.into();
        let active = self.playing_mut_for_tests();
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

    pub fn test_give_resources(&mut self, player_id: impl Into<PlayerId>, resources: ResourceSet) {
        let player_id = player_id.into();
        let active = self.playing_mut_for_tests();
        match active.game.transfer_from_bank(resources, player_id) {
            Ok(()) | Err(BankResourceExchangeError::BankIsShort) => {}
            Err(BankResourceExchangeError::AccountIsShort { .. }) => unreachable!(),
        }
    }

    pub fn test_take_resources(&mut self, player_id: impl Into<PlayerId>, resources: ResourceSet) {
        let player_id = player_id.into();
        let active = self.playing_mut_for_tests();
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
        let active = self.playing_mut_for_tests();
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
        let active = self.playing_mut_for_tests();
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
        let active = self.playing_mut_for_tests();
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
