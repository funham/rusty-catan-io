use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::gameplay::primitives::player::PlayerId;

use super::trade::TradeSessionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DecisionId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenDecision {
    pub id: DecisionId,
    pub player_id: PlayerId,
    pub kind: DecisionKind,
    pub lifetime: DecisionLifetime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionLifetime {
    OneShot,
    UntilSessionClosed(TradeSessionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionKind {
    InitPlacement,
    InitAction,
    PostDiceAction,
    PostDevCardAction,
    RegularAction,
    MoveRobber,
    ChooseRobbedPlayer { robber_pos: crate::topology::Hex },
    DropHalf { required: u16 },
    TradeResponse { session: TradeSessionId },
    TradeOwnerAction { session: TradeSessionId },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PendingDecisions {
    decisions: SmallVec<[OpenDecision; 64]>,
}

impl PendingDecisions {
    pub fn get(&self, id: DecisionId) -> Option<&OpenDecision> {
        self.decisions.iter().find(|decision| decision.id == id)
    }

    pub fn push(&mut self, decision: OpenDecision) {
        self.decisions.push(decision);
    }

    pub fn iter(&self) -> impl Iterator<Item = &OpenDecision> {
        self.decisions.iter()
    }

    pub fn close(&mut self, id: DecisionId) -> Option<OpenDecision> {
        let index = self
            .decisions
            .iter()
            .position(|decision| decision.id == id)?;
        Some(self.decisions.remove(index))
    }

    pub fn close_session(&mut self, session: TradeSessionId) -> Vec<OpenDecision> {
        let mut closed = Vec::new();
        let mut index = 0;
        while index < self.decisions.len() {
            if self.decisions[index].lifetime == DecisionLifetime::UntilSessionClosed(session) {
                closed.push(self.decisions.remove(index));
            } else {
                index += 1;
            }
        }
        closed
    }

    pub fn close_all(&mut self) -> impl IntoIterator<Item = OpenDecision> {
        std::mem::take(&mut self.decisions)
    }
}
