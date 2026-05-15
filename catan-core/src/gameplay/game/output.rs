use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::player::PlayerId;

use super::{decision::DecisionId, event::GameEvent};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameOutput {
    Event(GameEvent),
    DecisionOpened(super::decision::OpenDecision),
    DecisionClosed {
        decision_id: DecisionId,
    },
    CommandRejected {
        player_id: PlayerId,
        decision_id: Option<DecisionId>,
        reason: CommandRejectionReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandRejectionReason {
    WrongPlayer { expected: PlayerId },
    StaleDecision,
    WrongPhase,
    GameEnded,
    IllegalCommand(String),
}

pub trait OutputSink {
    fn push(&mut self, output: GameOutput);
}

#[derive(Debug, Default, Clone)]
pub struct VecOutputSink {
    outputs: Vec<GameOutput>,
}

impl VecOutputSink {
    pub fn into_vec(self) -> Vec<GameOutput> {
        self.outputs
    }

    pub fn as_slice(&self) -> &[GameOutput] {
        &self.outputs
    }
}

impl OutputSink for VecOutputSink {
    fn push(&mut self, output: GameOutput) {
        self.outputs.push(output);
    }
}

impl OutputSink for Vec<GameOutput> {
    fn push(&mut self, output: GameOutput) {
        self.push(output);
    }
}
