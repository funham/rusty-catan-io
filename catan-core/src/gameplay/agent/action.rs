use serde::{Deserialize, Serialize};

use crate::{
    agent::Agent,
    gameplay::{
        game::event::PlayerContext,
        primitives::{
            build::{Build, Road},
            dev_card::DevCardUsage,
            player::PlayerId,
            resource::ResourceCollection,
            trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
        },
    },
    topology::{Hex, Intersection},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct InitStageAction {
    pub establishment_position: Intersection,
    pub road: Road,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChoosePlayerToRobAction(pub PlayerId);

#[derive(Debug, Serialize, Deserialize)]
pub struct DropHalfAction(pub ResourceCollection);

#[derive(Debug, Serialize, Deserialize)]
pub struct MoveRobbersAction(pub Hex);

#[derive(Debug, Serialize, Deserialize)]
pub enum TradeAnswer {
    Accepted,
    Declined,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum InitAction {
    RollDice,
    UseDevCard(DevCardUsage),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PostDevCardAction {
    RollDice,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PostDiceAction {
    UseDevCard(DevCardUsage),
    RegularAction(RegularAction),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RegularAction {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Build),
    BuyDevCard,
    EndMove,
}

pub trait Request: Sized {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self;
}

impl Request for InitStageAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.init_stage_action(context)
    }
}

impl Request for InitAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.init_action(context)
    }
}

impl Request for TradeAnswer {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.answer_trade(context)
    }
}

impl Request for PostDevCardAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.after_dev_card_action(context)
    }
}

impl Request for PostDiceAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.after_dice_action(context)
    }
}

impl Request for RegularAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.regular_action(context)
    }
}

impl Request for ChoosePlayerToRobAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.choose_player_to_rob(context)
    }
}

impl Request for MoveRobbersAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.move_robbers(context)
    }
}

impl Request for DropHalfAction {
    fn request(agent: &mut dyn Agent, context: &PlayerContext) -> Self {
        agent.drop_half(context)
    }
}