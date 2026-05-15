use serde::{Deserialize, Serialize};

use crate::{
    gameplay::game::action::{
        ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction, MoveRobbersAction,
        PostDevCardAction, PostDiceAction, RegularAction,
    },
    gameplay::primitives::{
        player::PlayerId,
        trade::{PlayerTrade, PublicTradeOffer},
    },
};

use super::{
    decision::DecisionId,
    trade::{TradeOfferId, TradeScope},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameInput {
    Start,
    Submit {
        player_id: PlayerId,
        decision_id: DecisionId,
        command: PlayerCommand,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerCommand {
    InitialPlacement(InitStageAction),
    InitAction(InitAction),
    PostDice(PostDiceAction),
    PostDevCard(PostDevCardAction),
    Regular(RegularAction),
    MoveRobbers(MoveRobbersAction),
    ChooseRobbedPlayer(ChoosePlayerToRobAction),
    DropHalf(DropHalfAction),
    Trade(TradeCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeCommand {
    Propose {
        scope: TradeScope,
        offer: PublicTradeOffer,
    },
    Respond(TradeResponseCommand),
    Commit {
        offer_id: TradeOfferId,
    },
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeResponseCommand {
    Accept { offer_id: TradeOfferId },
    Reject,
    Counter { offer: PlayerTrade },
}
