use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::player::PlayerId;

use super::trade::TradeSessionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    NotStarted,
    InitialPlacement,
    Turn(TurnPhase),
    Trade(TradePhase),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnPhase {
    InitCommand,
    PostDiceCommand,
    PostDevCardCommand,
    RegularCommand,
    MoveRobber,
    ChooseRobbedPlayer { robber_pos: crate::topology::Hex },
    DropHalf { player_id: PlayerId, required: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradePhase {
    pub session: TradeSessionId,
}
