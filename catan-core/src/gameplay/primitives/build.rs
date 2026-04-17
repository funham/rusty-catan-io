//! Build system state, queries, and occupancy logic.
//!
//! This module defines the structures used to store player builds
//! (settlements, cities, roads), compute board occupancy, and query
//! build information during gameplay.
//!
//! The file is structured into internal modules to keep responsibilities
//! clearly separated while avoiding unnecessary file fragmentation.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Index, IndexMut, Not},
};

use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        field::state::{BuildCollection, FieldState},
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

pub use builds::*;
pub use data::*;
pub use occupancy::*;
pub use query::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Builds
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Primitive build structures and traits.
///
/// These represent the atomic pieces placed on the board and the traits
/// required for collision checking and build placement logic.
pub mod builds {
    use super::*;

    /// Set of intersections currently occupied by builds or roads.
    /// BTreeSet is used for deterministic ordering and efficient set operations.
    pub type IntersectionOccupancy = BTreeSet<Intersection>;

    /// Trait for objects that occupy intersections on the board.
    /// Used by collision and placement logic.
    pub trait Occupying {
        fn occupancy(&self) -> BTreeSet<Intersection>;
    }

    /// Marker trait for objects that can be built.
    /// Requires both a position (`HasPos`) and an occupancy definition.
    pub trait Buildable: HasPos + Occupying {}

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
    pub struct Settlement {
        pub pos: Intersection,
    }

    impl Settlement {
        /// Settlements harvest 1 resource from adjacent hexes.
        pub const fn harvesting_rate() -> u16 {
            1
        }
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
    pub struct City {
        pub pos: Intersection,
    }

    impl City {
        /// Cities harvest double resources.
        pub const fn harvesting_rate() -> u16 {
            2
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct Road {
        pub pos: Path,
    }

    /// Enum representing any build action.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Builds {
        Settlement(Settlement),
        City(City),
        Road(Road),
    }

    /// Errors that may occur during building.
    #[derive(Debug)]
    pub enum BuildingError {
        Road(EdgeInsertationError),
        Settlement(),
        City(),
        InitRoad(),
        InitSettlement(),
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

    /// Settlement occupies a single intersection.
    impl Occupying for Settlement {
        fn occupancy(&self) -> IntersectionOccupancy {
            IntersectionOccupancy::from([self.get_pos()])
        }
    }

    /// Road occupies both intersections of its path.
    impl Occupying for Road {
        fn occupancy(&self) -> IntersectionOccupancy {
            self.get_pos().intersections_iter().collect()
        }
    }

    /// City occupies the same intersection as the replaced settlement.
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
            self.pos.clone()
        }
    }

    /* HasCost impls */

    impl HasCost for Builds {
        fn cost(&self) -> ResourceCollection {
            (self as &dyn HasCost).cost()
        }
    }

    /// Standard Catan settlement cost.
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

    /// Standard Catan city upgrade cost.
    impl HasCost for City {
        fn cost(&self) -> ResourceCollection {
            ResourceCollection {
                ore: 3,
                wheat: 2,
                ..Default::default()
            }
        }
    }

    /// Standard Catan road cost.
    impl HasCost for Road {
        fn cost(&self) -> ResourceCollection {
            ResourceCollection {
                brick: 1,
                wood: 1,
                ..Default::default()
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Occupancy
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Structures representing board occupancy used during collision checking.
pub mod occupancy {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PathOccupancy {
        pub occupancy: IntersectionOccupancy,
        pub paths: BTreeSet<Path>,
    }

    impl PathOccupancy {
        /// Union of two road occupancy sets.
        pub fn union(&self, other: &Self) -> Self {
            Self {
                occupancy: self.occupancy.union(&other.occupancy).copied().collect(),
                paths: self.paths.union(&other.paths).cloned().collect(),
            }
        }
    }

    /// Combined occupancy structure used in collision checking.
    pub struct AggregateOccupancy {
        pub builds_occupancy: IntersectionOccupancy,
        pub roads_occupancy: PathOccupancy,
    }

    impl AggregateOccupancy {
        /// Type-driven accessor for occupancy subsets.
        pub fn get_for<T: OccupancyGetter>(&self) -> &T::OccupancyType {
            <T as OccupancyGetter>::get(self)
        }

        /// Union of two aggregate occupancies.
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
                        .cloned()
                        .collect(),
                },
            }
        }
    }

    /// Allows retrieving correct occupancy type depending on build type.
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
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Data
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Core build storage structures.
pub mod data {
    use super::*;

    #[derive(Debug, Default, Clone)]
    pub struct PlayerBuildData {
        pub settlements: BTreeSet<Settlement>,
        pub cities: BTreeSet<City>,
        pub roads: graph::RoadGraph,
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

    #[derive(Debug, Default, Clone)]
    pub struct BuildDataContainer {
        players: Vec<PlayerBuildData>,
        longest_road: Option<PlayerId>,
    }

    impl BuildDataContainer {
        pub fn new(n_players: usize) -> Self {
            Self {
                players: (0..n_players).map(|_| PlayerBuildData::default()).collect(),
                longest_road: None,
            }
        }

        pub fn from_build_collections(players: Vec<BuildCollection>) -> Self {
            Self {
                players: players
                    .into_iter()
                    .map(|player| PlayerBuildData {
                        settlements: player.settlements.into_iter().collect(),
                        cities: player.cities.into_iter().collect(),
                        roads: graph::RoadGraph::from_roads(
                            player.roads.into_iter().map(|road| road.pos),
                        ),
                    })
                    .collect(),
                longest_road: None,
            }
        }

        /* iterfaces */

        #[inline]
        pub fn occupancy(&self) -> BuildContainerOccupancy<'_> {
            BuildContainerOccupancy { container: self }
        }

        #[inline]
        pub fn query(&self) -> BuildContainerQuery<'_> {
            BuildContainerQuery { container: self }
        }

        /* getters */

        #[inline]
        pub fn longest_road(&self) -> Option<PlayerId> {
            self.longest_road
        }

        #[inline]
        pub fn players(&self) -> &[PlayerBuildData] {
            &self.players
        }

        #[inline]
        pub fn players_indexed(&self) -> impl Iterator<Item = (PlayerId, &PlayerBuildData)> {
            self.players
                .iter()
                .enumerate()
                .map(|(id, player)| (id as PlayerId, player))
        }

        /* modifiers */

        pub fn try_build(
            &mut self,
            player_id: PlayerId,
            build: Builds,
        ) -> Result<(), BuildingError> {
            let occ = self.occupancy();

            let checker = CollisionChecker {
                other_occupancy: &occ
                    .occupancy((0..self.players.len()).filter(|id| id != &player_id)),
                this_occupancy: &occ.occupancy([player_id]),
            };

            match build {
                Builds::Road(road) => {
                    match self.players[player_id].roads.extend(&road.pos, &checker) {
                        Ok(_) => Ok(()),
                        Err(err) => Err(BuildingError::Road(err)),
                    }
                }

                Builds::Settlement(settlement) => match checker.can_place(&settlement) {
                    true => Ok({
                        self.players[player_id]
                            .settlements
                            .insert(settlement)
                            .not()
                            .then(|| log::warn!("settlement was placed on top of another"));
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
                                .then(|| log::warn!("settlement is non-existent"));

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
            let occ = self.occupancy();

            let checker = CollisionChecker {
                other_occupancy: &occ
                    .occupancy((0..self.players.len()).filter(|id| id != &player_id)),
                this_occupancy: &occ.occupancy([player_id]),
            };

            let settlement_ok = checker
                .full_occupancy()
                .builds_occupancy
                .is_disjoint(&checker.building_deadzone(&settlement));

            if !settlement_ok {
                return Err(BuildingError::InitSettlement());
            }

            self[player_id].settlements.insert(settlement);

            let road_ok = road.pos.intersections_iter().any(|v| v == settlement.pos)
                && !checker
                    .full_occupancy()
                    .roads_occupancy
                    .paths
                    .contains(&road.pos);

            if !road_ok {
                log::error!("invalid initial road placement");
                return Err(BuildingError::InitRoad());
            }

            self[player_id].roads.add_edge(&road.pos);

            Ok(())
        }
    }

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
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Query
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Read-only query utilities over the build container.
pub mod query {
    use super::*;

    pub struct BuildContainerOccupancy<'a> {
        pub(crate) container: &'a BuildDataContainer,
    }

    impl<'a> BuildContainerOccupancy<'a> {
        pub fn builds_occupancy<Players>(&self, ids: Players) -> IntersectionOccupancy
        where
            Players: IntoIterator<Item = PlayerId> + Clone,
        {
            ids.into_iter()
                .flat_map(|id| {
                    let player = &self.container.players()[id];

                    player
                        .settlements
                        .iter()
                        .map(|s| s.pos)
                        .chain(player.cities.iter().map(|c| c.pos))
                })
                .collect()
        }

        pub fn roads_occupancy<Players>(&self, ids: Players) -> PathOccupancy
        where
            Players: IntoIterator<Item = PlayerId>,
        {
            ids.into_iter()
                .map(|id| self.container.players()[id].roads_occupancy())
                .fold(PathOccupancy::default(), |acc, x| acc.union(&x))
        }

        pub fn occupancy<Players>(&self, ids: Players) -> AggregateOccupancy
        where
            Players: IntoIterator<Item = PlayerId> + Clone,
        {
            AggregateOccupancy {
                builds_occupancy: self.builds_occupancy(ids.clone()),
                roads_occupancy: self.roads_occupancy(ids),
            }
        }

        pub fn builds_occupancy_full(&self) -> IntersectionOccupancy {
            self.builds_occupancy(0..self.container.players().len())
        }

        pub fn roads_occupancy_full(&self) -> PathOccupancy {
            self.roads_occupancy(0..self.container.players().len())
        }

        pub fn occupancy_full(&self) -> AggregateOccupancy {
            AggregateOccupancy {
                builds_occupancy: self.builds_occupancy_full(),
                roads_occupancy: self.roads_occupancy_full(),
            }
        }
    }

    pub struct BuildContainerQuery<'a> {
        pub(crate) container: &'a BuildDataContainer,
    }

    impl<'a> BuildContainerQuery<'a> {
        pub fn builds_on_hex(&self, hex: Hex) -> BTreeMap<PlayerId, BuildCollection> {
            self.container
                .players()
                .iter()
                .enumerate()
                .filter_map(|(player_id, player)| {
                    let settlements = player
                        .settlements
                        .iter()
                        .copied()
                        .filter(|s| s.pos.as_set().contains(&hex))
                        .collect::<Vec<_>>();

                    let cities = player
                        .cities
                        .iter()
                        .copied()
                        .filter(|c| c.pos.as_set().contains(&hex))
                        .collect::<Vec<_>>();

                    let roads = player
                        .roads
                        .iter()
                        .filter(|r| r.pos.as_set().contains(&hex))
                        .collect::<Vec<_>>();

                    if settlements.is_empty() && cities.is_empty() && roads.is_empty() {
                        None
                    } else {
                        Some((
                            player_id,
                            BuildCollection {
                                settlements,
                                cities,
                                roads,
                            },
                        ))
                    }
                })
                .collect()
        }

        pub fn all_builds(&self) -> Vec<BuildCollection> {
            self.container
                .players()
                .iter()
                .map(|player| BuildCollection {
                    settlements: player.settlements.iter().copied().collect(),
                    cities: player.cities.iter().copied().collect(),
                    roads: player.roads.iter().collect(),
                })
                .collect()
        }

        pub fn possible_initial_placements(
            &self,
            field: &FieldState,
            player_id: PlayerId,
        ) -> Vec<(Settlement, Road)> {
            let occ = self.container.occupancy();

            let checker = CollisionChecker {
                other_occupancy: &occ
                    .occupancy((0..self.container.players().len()).filter(|id| id != &player_id)),
                this_occupancy: &occ.occupancy([player_id]),
            };

            let intersections = field
                .arrangement
                .hex_enum_iter()
                .flat_map(|(hex, _)| hex.vertices().collect::<Vec<_>>())
                .collect::<BTreeSet<_>>();

            intersections
                .into_iter()
                .map(|pos| Settlement { pos })
                .filter(|settlement| {
                    checker
                        .full_occupancy()
                        .builds_occupancy
                        .is_disjoint(&checker.building_deadzone(settlement))
                })
                .flat_map(|settlement| {
                    settlement
                        .pos
                        .paths()
                        .into_iter()
                        .map(move |path| (settlement, Road { pos: path }))
                })
                .collect()
        }
    }
}
