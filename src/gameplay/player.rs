use std::cell::RefCell;
use std::collections::BTreeSet;
use std::convert::Infallible;
use std::rc::Rc;

use crate::gameplay::dev_card::{DevCardData, OpponentDevCardData};
use crate::gameplay::resource::{HasCost, ResourceCollection};
use crate::gameplay::strategy;
use crate::topology::*;

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

pub type PlayerId = usize;

pub struct PlayerBuildData {
    pub settlements: BTreeSet<Settlement>,
    pub cities: BTreeSet<City>,
    pub roads: graph::RoadGraph, // derives default
}

impl PlayerBuildData {
    pub fn builds_occupancy(&self) -> BTreeSet<Intersection> {
        self.settlements
            .iter()
            .map(|s| s.pos)
            .chain(self.cities.iter().map(|c| c.pos))
            .collect()
    }

    pub fn roads_occupancy(&self) -> BTreeSet<Path> {
        self.roads.roads_occupancy().clone()
    }
}

pub struct PlayerData {
    pub resources: ResourceCollection,
    pub dev_cards: DevCardData,
}

pub struct Player {
    pub data: PlayerData,
    pub strategy: Rc<RefCell<dyn strategy::Strategy>>,
}

pub struct OpponentData {
    pub dev_cards: OpponentDevCardData,
}

impl From<&PlayerData> for OpponentData {
    fn from(player_data: &PlayerData) -> Self {
        Self {
            dev_cards: OpponentDevCardData {
                queued: player_data.dev_cards.queued.total(),
                active: player_data.dev_cards.active.total(),
                played: player_data.dev_cards.played.clone(),
            },
        }
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
