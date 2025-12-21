use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Index, IndexMut, Not},
};

use crate::{
    gameplay::{
        field::state::BuildCollection,
        primitives::{
            player::PlayerId,
            resource::{HasCost, ResourceCollection},
        },
    },
    topology::{
        HasPos, Hex, Intersection, Path,
        collision::CollisionChecker,
        graph::{self, EdgeInsertationError},
    },
};

/* traits & aliases */

pub type IntersectionOccupancy = BTreeSet<Intersection>;

pub trait Occupying {
    fn occupancy(&self) -> BTreeSet<Intersection>;
}

pub trait Buildable: HasPos + Occupying {}

/* BuildData */

#[derive(Debug, Default)]
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

    /* getters */
    pub fn longest_road(&self) -> Option<PlayerId> {
        self.longest_road
    }

    /* modifiers */
    pub fn try_build(&mut self, player_id: PlayerId, build: Builds) -> Result<(), BuildingError> {
        let checker = &CollisionChecker {
            other_occupancy: &self.occupancy((0..self.players.len()).filter(|id| id != &player_id)),
            this_occupancy: &self.occupancy([player_id]),
        };

        match build {
            Builds::Road(road) => match self.players[player_id].roads.extend(road.pos, &checker) {
                Ok(_) => Ok(()),
                Err(err) => Err(BuildingError::Road(err)),
            },
            Builds::Settlement(settlement) => match checker.can_place(&settlement) {
                true => Ok({
                    self.players[player_id]
                        .settlements
                        .insert(settlement)
                        .not()
                        .then(|| log::warn!("seems like settlement was placed on top of another"));
                }),
                false => Err(BuildingError::Settlement()),
            },
            Builds::City(city) => {
                match self.players[player_id]
                    .settlements
                    .contains(&Settlement { pos: city.pos })
                {
                    true => Ok({
                        self.players[player_id]
                            .settlements
                            .remove(&Settlement { pos: city.pos })
                            .not()
                            .then(|| log::warn!("settlement non-existent"));

                        self.players[player_id]
                            .cities
                            .insert(city)
                            .not()
                            .then(|| log::warn!("city already exists"));
                    }),
                    false => todo!(),
                }
            }
        }
    }

    pub fn try_init_place(
        &mut self,
        player_id: PlayerId,
        road: Road,
        settlement: Settlement,
    ) -> Result<(), BuildingError> {
        let checker = &CollisionChecker {
            other_occupancy: &self.occupancy((0..self.players.len()).filter(|id| id != &player_id)),
            this_occupancy: &self.occupancy([player_id]),
        };

        let settlement_ok = checker
            .full_occupancy()
            .builds_occupancy
            .is_disjoint(&checker.building_deadzone(&settlement));

        match settlement_ok {
            true => {
                self[player_id].settlements.insert(settlement);
            }
            false => return Err(BuildingError::InitSettlement()),
        }

        let road_ok = checker
            .full_occupancy()
            .roads_occupancy
            .paths
            .contains(&road.pos);

        road_ok
            .not()
            .then(|| log::error!("invalid initial road placement"));

        match road_ok {
            true => Ok({
                self[player_id].roads.add_edge(road.pos);
            }),
            false => Err(BuildingError::InitRoad()),
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
        Self {
            occupancy: self.occupancy.union(&other.occupancy).copied().collect(),
            paths: self.paths.union(&other.paths).copied().collect(),
        }
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
            roads_occupancy: PathOccupancy {
                occupancy: self
                    .roads_occupancy
                    .occupancy
                    .union(&other.roads_occupancy.occupancy)
                    .copied()
                    .collect(),
                paths: self
                    .roads_occupancy
                    .paths
                    .union(&other.roads_occupancy.paths)
                    .copied()
                    .collect(),
            },
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

#[derive(Debug)]
pub enum BuildingError {
    Road(EdgeInsertationError),
    Settlement(),
    City(),
    InitRoad(),
    InitSettlement(),
}

/* impls */

impl Index<PlayerId> for BuildDataContainer {
    type Output = PlayerBuildData;

    fn index(&self, index: PlayerId) -> &Self::Output {
        &self.players[index]
    }
}

impl IndexMut<PlayerId> for BuildDataContainer {
    fn index_mut(&mut self, index: PlayerId) -> &mut Self::Output {
        &mut self.players[index]
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
