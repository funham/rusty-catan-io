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
    topology::{HasPos, Hex, Intersection, Path, collision::CollisionChecker, graph},
};

/* traits & aliases */

pub type IntersectionOccupancy = BTreeSet<Intersection>;

pub trait Occupying {
    fn occupancy(&self) -> BTreeSet<Intersection>;
}

pub trait Buildable: HasPos + Occupying {}

/* BuildData */

#[derive(Debug)]
pub struct BuildDataContainer {
    players: Vec<PlayerBuildData>,
    longest_road: Option<PlayerId>,
}

#[derive(Debug)]
pub struct PlayerBuildData {
    pub settlements: BTreeSet<Settlement>,
    pub cities: BTreeSet<City>,
    pub roads: graph::RoadGraph, // derives default
}

impl BuildDataContainer {
    /* occupancy */
    pub fn builds_occupancy<Players>(&self, ids: Players) -> IntersectionOccupancy
    where
        Players: IntoIterator<Item = PlayerId>,
    {
        ids.into_iter()
            .map(|id| self.players[id].roads_occupancy().occupancy)
            .fold(BTreeSet::default(), |acc, x| {
                acc.union(&x).copied().collect()
            })
    }

    pub fn roads_occupancy<Players>(&self, ids: Players) -> PathOccupancy
    where
        Players: IntoIterator<Item = PlayerId>,
    {
        ids.into_iter()
            .map(|id| self.players[id].roads_occupancy())
            .fold(PathOccupancy::default(), |acc, x| acc.union(&x))
    }

    pub fn occupancy<Players>(&self, ids: Players) -> AggregateOccupancy
    where
        Players: IntoIterator<Item = PlayerId>,
    {
        let ids = ids.into_iter().collect::<Vec<_>>();
        AggregateOccupancy {
            builds_occupancy: self.builds_occupancy(ids.clone()),
            roads_occupancy: self.roads_occupancy(ids),
        }
    }

    pub fn builds_occupancy_full(&self) -> IntersectionOccupancy {
        self.builds_occupancy(0..self.players.len())
    }

    pub fn roads_occupancy_full(&self) -> PathOccupancy {
        self.roads_occupancy(0..self.players.len())
    }

    pub fn occupancy_full(&self) -> AggregateOccupancy {
        AggregateOccupancy {
            builds_occupancy: self.builds_occupancy_full(),
            roads_occupancy: self.roads_occupancy_full(),
        }
    }

    pub fn create_collision_checker(&self, player_id: PlayerId) -> CollisionChecker {
        CollisionChecker {
            other_occupancy: todo!(),
            this_occupancy: todo!(),
        }
    }

    /* getters */
    pub fn longest_road(&self) -> Option<PlayerId> {
        self.longest_road
    }

    /* modifiers */
    pub fn try_build(&mut self, player_id: PlayerId, build: Builds) -> Result<(), ()> {
        let graph = &self.players[player_id].roads;
        match build {
            Builds::Road(road) => todo!(),
            Builds::Settlement(settlement) => todo!(),
            Builds::City(city) => todo!(),
        }
    }

    /* queries */
    pub fn builds_on_hex(&self, _hex: Hex) -> BTreeMap<PlayerId, BuildCollection> {
        todo!("do I really need that?")
    }
}

impl PlayerBuildData {
    pub fn generic_occupancy<Pos, Builds, BuildItem>(builds: Builds) -> IntersectionOccupancy
    where
        Builds: Iterator<Item = BuildItem>,
        BuildItem: Buildable<Pos = Pos>,
    {
        builds.map(|b| b.occupancy()).flatten().collect()
    }
    pub fn builds_occupancy(&self) -> IntersectionOccupancy {
        Self::generic_occupancy(self.settlements.iter())
            .union(&mut Self::generic_occupancy(self.cities.iter()))
            .copied()
            .collect()
    }

    pub fn roads_occupancy(&self) -> PathOccupancy {
        PathOccupancy {
            occupancy: Self::generic_occupancy(self.roads.iter()),
            paths: self.roads.edges().clone(),
        }
    }

    pub fn occupancy(&self) -> AggregateOccupancy {
        AggregateOccupancy {
            builds_occupancy: self.builds_occupancy(),
            roads_occupancy: self.roads_occupancy(),
        }
    }
}

/* primitives */

pub struct AggregateOccupancy {
    pub builds_occupancy: IntersectionOccupancy,
    pub roads_occupancy: PathOccupancy,
}


#[derive(Debug, Default)]
pub struct PathOccupancy {
    pub occupancy: IntersectionOccupancy,
    pub paths: BTreeSet<Path>,
}

impl PathOccupancy {
    pub fn union(&self, other: &Self) -> Self {
        Self { occupancy: self.occupancy.union(&other.occupancy).copied().collect(), paths: self.paths.union(&other.paths).copied().collect() }
    }
}

impl AggregateOccupancy {
    pub fn get_for<T: OccupancyGetter>(&self) -> &T::OccupancyType {
        <T as OccupancyGetter>::get(self)
    }

    pub fn union(&self, other: &AggregateOccupancy) -> AggregateOccupancy {
        AggregateOccupancy {
            builds_occupancy: self
                .builds_occupancy
                .union(&other.builds_occupancy)
                .copied()
                .collect(),
            roads_occupancy: PathOccupancy {occupancy: self
                .roads_occupancy.occupancy
                .union(&other.roads_occupancy.occupancy)
                .copied()
                .collect(),
                paths: self
                .roads_occupancy.paths
                .union(&other.roads_occupancy.paths)
                .copied()
                .collect()
            }
        }
    }
}

pub trait OccupancyGetter: Occupying {
    type OccupancyType;
    fn get<'a>(x: &'a AggregateOccupancy) -> &'a Self::OccupancyType;
}

impl OccupancyGetter for Road {
    type OccupancyType = PathOccupancy;
    fn get<'a>(x: &'a AggregateOccupancy) -> &'a Self::OccupancyType {
        &x.roads_occupancy
    }
}

impl<T: Occupying + HasPos<Pos = Intersection>> OccupancyGetter for T {
    type OccupancyType = IntersectionOccupancy;
    fn get<'a>(x: &'a AggregateOccupancy) -> &'a Self::OccupancyType {
        &x.builds_occupancy
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct City {
    pub pos: Intersection,
}

impl City {
    pub const fn harvesting_rate() -> u16 {
        2
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Road {
    pub pos: Path,
}

#[derive(Debug)]
pub enum Builds {
    Settlement(Settlement),
    City(City),
    Road(Road),
}

/* impls */

impl Index<PlayerId> for BuildDataContainer {
    type Output = PlayerBuildData;

    fn index(&self, index: PlayerId) -> &Self::Output {
        todo!()
    }
}

impl IndexMut<PlayerId> for BuildDataContainer {
    fn index_mut(&mut self, index: PlayerId) -> &mut Self::Output {
        todo!()
    }
}

impl HasCost for Builds {
    fn cost(&self) -> ResourceCollection {
        (self as &dyn HasCost).cost()
    }
}

/* Buildable impls */

impl<T: Buildable> Buildable for &T {}

impl Buildable for Settlement {}
impl Buildable for Road {}
impl Buildable for City {}

/* Occupying impls */

impl<T: Occupying> Occupying for &T {
    fn occupancy(&self) -> IntersectionOccupancy {
        <T as Occupying>::occupancy(&self)
    }
}

impl Occupying for Settlement {
    fn occupancy(&self) -> IntersectionOccupancy {
        IntersectionOccupancy::from([self.get_pos()])
    }
}

impl Occupying for Road {
    fn occupancy(&self) -> IntersectionOccupancy {
        self.get_pos().intersections_iter().collect()
    }
}

impl Occupying for City {
    fn occupancy(&self) -> IntersectionOccupancy {
        IntersectionOccupancy::from([self.get_pos()])
    }
}

/* HasPos impls */

impl HasPos for Settlement {
    type Pos = Intersection;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

impl HasPos for City {
    type Pos = Intersection;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

impl HasPos for Road {
    type Pos = Path;
    fn get_pos(&self) -> Self::Pos {
        self.pos
    }
}

/* HasCost impls */

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
