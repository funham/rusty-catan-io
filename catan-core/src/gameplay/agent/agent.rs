use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        game::state::Perspective,
        primitives::{
            build::{Establishment, Road},
            player::PlayerId,
            resource::ResourceCollection,
            trade::PlayerTrade,
        },
    },
    topology::Hex,
};

use super::action::{
    FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceThrowAnswer, TradeAction,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum AgentRequest {
    Init(Perspective),
    AfterDevCard(Perspective),
    AfterDiceThrow(Perspective),
    Rest(Perspective),
    RobHex(Perspective),
    RobPlayer(Perspective),
    Initialization(Perspective),
    AnswerTrade {
        perspective: Perspective,
        trade: PlayerTrade,
    },
    DropHalf(Perspective),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AgentResponse {
    Init(InitialAction),
    AfterDevCard(PostDevCardAction),
    AfterDiceThrow(PostDiceThrowAnswer),
    Rest(FinalStateAnswer),
    RobHex(Hex),
    RobPlayer(PlayerId),
    Initialization {
        establishment: Establishment,
        road: Road,
    },
    AnswerTrade(TradeAction),
    DropHalf(ResourceCollection),
}

pub trait Agent {
    fn respond(&mut self, request: AgentRequest) -> AgentResponse;
}
