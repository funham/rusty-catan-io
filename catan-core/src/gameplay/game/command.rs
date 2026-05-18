use serde::{Deserialize, Serialize};

use crate::{
    gameplay::primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        dev_card::DevCardUsage,
        player::PlayerId,
        resource::ResourceSet,
        trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer},
    },
    topology::{Hex, Intersection, Path},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InitialPlacementCommand {
    settlement: Intersection,
    road: Path,
}

impl InitialPlacementCommand {
    pub fn try_new(settlement: Intersection, road: Path) -> Option<Self> {
        settlement
            .paths()
            .contains(&road)
            .then_some(Self { settlement, road })
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
pub struct ChooseRobbedPlayerCommand(pub PlayerId);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DropHalfCommand(pub ResourceSet);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoveRobberCommand(pub Hex);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TradeAnswer {
    Accept,
    Decline,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InitCommand {
    RollDice,
    UseDevCard(DevCardUsage),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PostDevCardCommand {
    RollDice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PostDiceCommand {
    UseDevCard(DevCardUsage),
    RegularCommand(RegularCommand),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RegularCommand {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Build),
    BuyDevCard,
    EndMove,
}
