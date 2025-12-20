use crate::{
    gameplay::{
        dev_card::UsableDevCardKind,
        player::*,
        resource::{HasCost, Resource, ResourceCollection},
    },
    topology::{Hex, Intersection, Path},
};

#[derive(Debug, Clone, Copy)]
pub struct Robbery {
    pub hex: Hex,
    pub robbed: Option<PlayerId>,
}

impl Robbery {
    pub fn just_move(hex: Hex) -> Self {
        Self { hex, robbed: None }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DevCardUsage {
    Knight(Robbery),
    YearOfPlenty([Resource; 2]),
    RoadBuild([Path; 2]),
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

pub struct PlayerTrade {
    pub give: ResourceCollection,
    pub take: ResourceCollection,
}

impl PlayerTrade {
    pub fn opposite(&self) -> Self {
        Self {
            give: self.take,
            take: self.give,
        }
    }
}

impl HasCost for Buildable {
    fn cost(&self) -> ResourceCollection {
        (self as &dyn HasCost).cost()
    }
}

pub trait HasPos {
    type Pos;
    fn get_pos(&self) -> Self::Pos;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Settlement {
    pub pos: Intersection,
}

impl Settlement {
    pub const fn harvesting_rate() -> u16 {
        1
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PortType {
    Special(Resource),
    General,
}

impl HasPos for Settlement {
    type Pos = Intersection;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

impl HasPos for &Settlement {
    type Pos = Intersection;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct City {
    pub pos: Intersection,
}

impl City {
    pub const fn harvesting_rate() -> u16 {
        2
    }
}

impl HasPos for City {
    type Pos = Intersection;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

impl HasPos for &City {
    type Pos = Intersection;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Road {
    pub pos: Path,
}

impl HasPos for Road {
    type Pos = Path;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

impl HasCost for Settlement {
    fn cost(&self) -> ResourceCollection {
        ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 1,
            sheep: 1,
            ore: 0,
        }
    }
}

impl HasCost for City {
    fn cost(&self) -> ResourceCollection {
        ResourceCollection {
            ore: 3,
            wheat: 2,
            ..Default::default()
        }
    }
}

impl HasCost for Road {
    fn cost(&self) -> ResourceCollection {
        ResourceCollection {
            brick: 1,
            wood: 1,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HexType {
    Some(Resource),
    Desert,
}

#[derive(Debug, Clone, Copy)]
pub struct HexInfo {
    pub hex_type: HexType,
    pub number: u8,
}
