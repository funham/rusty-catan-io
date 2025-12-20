use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Index, IndexMut},
};

use crate::{
    gameplay::{
        field::state::BuildCollection,
        primitives::{
            player::PlayerId,
            resource::{HasCost, ResourceCollection},
        },
    },
    topology::{HasPos, Hex, Intersection, Path, graph},
};

#[derive(Debug)]
pub struct GameBuildData;

impl GameBuildData {
    pub fn builds_on_hex(&self, hex: Hex) -> BTreeMap<PlayerId, BuildCollection> {
        todo!()
    }
}

impl Index<PlayerId> for GameBuildData {
    type Output = PlayerBuildData;

    fn index(&self, index: PlayerId) -> &Self::Output {
        todo!()
    }
}

impl IndexMut<PlayerId> for GameBuildData {
    fn index_mut(&mut self, index: PlayerId) -> &mut Self::Output {
        todo!()
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum Buildable {
    Settlement(Settlement),
    City(City),
    Road(Road),
}

impl HasCost for Buildable {
    fn cost(&self) -> ResourceCollection {
        (self as &dyn HasCost).cost()
    }
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
