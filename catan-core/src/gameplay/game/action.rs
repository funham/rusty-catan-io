use serde::{Deserialize, Serialize};

use crate::{
    gameplay::primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        dev_card::DevCardUsage,
        player::PlayerId,
        resource::ResourceCollection,
        trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
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
