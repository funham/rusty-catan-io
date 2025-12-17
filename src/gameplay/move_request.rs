use std::{default, marker::PhantomData};

use crate::{
    gameplay::{
        dev_card::UsableDevCardKind,
        field::Field,
        player::*,
        resource::{self, HasCost, Resource, ResourceCollection},
    },
    topology::{Edge, Hex},
};

#[derive(Debug, Clone, Copy)]
pub enum DevCardUsage {
    Knight(RobRequest),
    YearOfPlenty((Resource, Resource)),
    RoadBuild((Edge, Edge)),
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
    pub player: Option<PlayerId>,
}

impl RobRequest {
    pub fn with_robbing(hex: Hex, player: PlayerId) -> Option<Self> {
        todo!()
    }

    pub fn without_robbing(hex: Hex) -> Self {
        Self { hex, player: None }
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
