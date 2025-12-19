use crate::{
    gameplay::{
        dev_card::UsableDevCardKind,
        player::*,
        resource::{HasCost, Resource, ResourceCollection},
    },
    topology::{Hex, Path},
};

#[derive(Debug, Clone, Copy)]
pub enum DevCardUsage {
    Knight(RobRequest),
    YearOfPlenty((Resource, Resource)),
    RoadBuild((Path, Path)),
    Monopoly(Resource),
}

impl DevCardUsage {
    pub fn card(&self) -> UsableDevCardKind {
        match self {
            DevCardUsage::Knight(_) => UsableDevCardKind::Knight,
            DevCardUsage::YearOfPlenty(_) => UsableDevCardKind::YearOfPlenty,
            DevCardUsage::RoadBuild(_) => UsableDevCardKind::YearOfPlenty,
            DevCardUsage::Monopoly(_) => UsableDevCardKind::Monopoly,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RobRequest {
    pub hex: Hex,
    pub robbed: Option<PlayerId>,
}

impl RobRequest {
    pub fn just_move(hex: Hex) -> Self {
        Self { hex, robbed: None }
    }
}

#[derive(Debug)]
pub enum Buildable {
    Settlement(Settlement),
    City(City),
    Road(Road),
}

pub struct PublicTradeOffer {
    give: ResourceCollection,
    take: ResourceCollection,
}

pub struct PersonalTradeOffer {
    give: ResourceCollection,
    take: ResourceCollection,
    peer: PlayerId,
}

pub enum BankTradeKind {
    Common,
    PortUniversal,
    PortSpecial,
}

pub struct BankTrade {
    give: Resource,
    take: Resource,
    kind: BankTradeKind,
}

impl HasCost for Buildable {
    fn cost(&self) -> ResourceCollection {
        (self as &dyn HasCost).cost()
    }
}

pub enum TradeAnswer {
    Accepted,
    Declined,
}
pub enum MoveRequestInit {
    ThrowDice,
    UseKnight(RobRequest),
}
pub enum MoveRequestAfterDevCard {
    ThrowDice,
}

pub enum MoveRequestAfterDiceThrow {
    UseDevCard(DevCardUsage),
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Buildable),
    EndMove,
}

pub enum MoveRequestAfterDiceThrowAndDevCard {
    OfferPublicTrade(PublicTradeOffer),
    OfferPersonalTrade(PersonalTradeOffer),
    TradeWithBank(BankTrade),
    Build(Buildable),
    EndMove,
}
