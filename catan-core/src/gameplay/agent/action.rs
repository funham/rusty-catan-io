use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        agent::agent::PlayerRuntime,
        game::view::PlayerDecisionContext,
        primitives::{
            build::{Build, Establishment, EstablishmentType, Road},
            dev_card::DevCardUsage,
            player::PlayerId,
            resource::ResourceCollection,
            trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
        },
    },
    topology::{Hex, Intersection, Path},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InitStageAction {
    settlement: Intersection,
    road: Path,
}

impl InitStageAction {
    pub fn try_new(settlement: Intersection, road: Path) -> Option<Self> {
        match settlement.paths().contains(&road) {
            true => Some(Self { settlement, road }),
            false => None,
        }
    }

    pub fn as_builds(&self) -> (Establishment, Road) {
        (
            Establishment {
                vtx: self.settlement,
                stage: EstablishmentType::Settlement,
            },
            Road { path: self.road },
        )
    }

    pub fn settlement_pos(&self) -> Intersection {
        self.settlement
    }

    pub fn road_pos(&self) -> Path {
        self.road
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChoosePlayerToRobAction(pub PlayerId);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DropHalfAction(pub ResourceCollection);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoveRobbersAction(pub Hex);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TradeAnswer {
    Accept,
    Decline,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InitAction {
    RollDice,
    UseDevCard(DevCardUsage),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PostDevCardAction {
    RollDice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PostDiceAction {
    UseDevCard(DevCardUsage),
    RegularAction(RegularAction),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RegularAction {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Build),
    BuyDevCard,
    EndMove,
}

pub trait DecisionRequest: Sized {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self;
}

impl DecisionRequest for InitStageAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.init_stage_action(context)
    }
}

impl DecisionRequest for InitAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.init_action(context)
    }
}

impl DecisionRequest for TradeAnswer {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.answer_trade(context)
    }
}

impl DecisionRequest for PostDevCardAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.after_dev_card_action(context)
    }
}

impl DecisionRequest for PostDiceAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.after_dice_action(context)
    }
}

impl DecisionRequest for RegularAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.regular_action(context)
    }
}

impl DecisionRequest for MoveRobbersAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.move_robbers(context)
    }
}

impl DecisionRequest for DropHalfAction {
    fn request(player: &mut dyn PlayerRuntime, context: PlayerDecisionContext<'_>) -> Self {
        player.drop_half(context)
    }
}
