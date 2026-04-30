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
    ops::{Index, IndexMut},
};

use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        field::state::{BoardLayout, BuildCollection},
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
    // pub trait Buildable: HasPos + Occupying {}

    // TODO: merge Settlement and City in a single type like `Establishment`
    // with an enum field `stage` or `kind` that can hold either `Settelement` or `City`

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
    pub enum EstablishmentType {
        Settlement,
        City,
    }

    impl EstablishmentType {
        pub const fn harvest_amount(&self) -> u8 {
            match self {
                Self::Settlement => 1,
                Self::City => 2,
            }
        }
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
    pub struct Establishment {
        pub pos: Intersection,
        pub stage: EstablishmentType,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct Road {
        pub pos: Path,
    }

    /// Enum representing any build action.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, strum::IntoStaticStr)]
    pub enum Build {
        Establishment(Establishment),
        Road(Road),
    }

    /// Errors that may occur during building.
    #[derive(Debug)]
    pub enum BuildingError {
        Road(EdgeInsertationError),
        Settlement(),
        City(),
        RoadLimit(),
        SettlementLimit(),
        CityLimit(),
        InitRoad(Path),
        InitSettlement(Intersection),
    }

    /* Buildable impls */

    // impl<T: Buildable> Buildable for &T {}

    // impl Buildable for Settlement {}
    // impl Buildable for Road {}
    // impl Buildable for City {}

    /* Occupying impls */

    impl<T: Occupying> Occupying for &T {
        fn occupancy(&self) -> IntersectionOccupancy {
            <T as Occupying>::occupancy(&self)
        }
    }

    /// Settlement occupies a single intersection.
    impl Occupying for Establishment {
        fn occupancy(&self) -> IntersectionOccupancy {
            IntersectionOccupancy::from([self.pos()])
        }
    }

    /// Road occupies both intersections of its path.
    impl Occupying for Road {
        fn occupancy(&self) -> IntersectionOccupancy {
            self.pos().intersections_iter().collect()
        }
    }

    /* HasPos impls */

    impl HasPos for Establishment {
        type Pos = Intersection;
        fn pos(&self) -> Self::Pos {
            self.pos
        }
    }

    impl HasPos for Road {
        type Pos = Path;
        fn pos(&self) -> Self::Pos {
            self.pos.clone()
        }
    }

    /* HasCost impls */

    impl HasCost for Build {
        fn cost(&self) -> ResourceCollection {
            match self {
                Build::Establishment(establishment) => establishment.stage.cost(),
                Build::Road(road) => road.cost(),
            }
        }
    }

    /// Standard Catan settlement cost.
    impl HasCost for EstablishmentType {
        fn cost(&self) -> ResourceCollection {
            match self {
                Self::Settlement => ResourceCollection {
                    brick: 1,
                    wood: 1,
                    wheat: 1,
                    sheep: 1,
                    ore: 0,
                },
                Self::City => ResourceCollection {
                    ore: 3,
                    wheat: 2,
                    ..Default::default()
                },
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

    pub struct BuildDataOccupancy<'a> {
        pub(crate) container: &'a BoardBuildData,
    }

    impl<'a> BuildDataOccupancy<'a> {
        pub fn builds_occupancy<Players>(&self, ids: Players) -> IntersectionOccupancy
        where
            Players: IntoIterator<Item = PlayerId> + Clone,
        {
            ids.into_iter()
                .flat_map(|id| {
                    let player = &self.container.players()[id];

                    player.establishments.iter().map(|s| s.pos)
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
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Data
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Core build storage structures.
pub mod data {
    use super::*;

    #[derive(Debug, Default, Clone, Serialize, Deserialize)]
    pub struct PlayerBuildData {
        pub establishments: BTreeSet<Establishment>,
        pub roads: graph::RoadGraph,
    }

    impl PlayerBuildData {
        pub const ROAD_LIMIT: usize = 15;
        pub const SETTLEMENT_LIMIT: usize = 5;
        pub const CITY_LIMIT: usize = 4;

        pub fn generic_occupancy<Builds, BuildItem>(builds: Builds) -> IntersectionOccupancy
        where
            Builds: Iterator<Item = BuildItem>,
            BuildItem: Occupying,
        {
            builds.map(|b| b.occupancy()).flatten().collect()
        }

        pub fn roads_count(&self) -> usize {
            self.roads.edges().len()
        }

        pub fn settlements_count(&self) -> usize {
            self.establishments
                .iter()
                .filter(|establishment| establishment.stage == EstablishmentType::Settlement)
                .count()
        }

        pub fn cities_count(&self) -> usize {
            self.establishments
                .iter()
                .filter(|establishment| establishment.stage == EstablishmentType::City)
                .count()
        }

        pub fn builds_occupancy(&self) -> IntersectionOccupancy {
            Self::generic_occupancy(self.establishments.iter())
                .into_iter()
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

    #[derive(Debug, Default, Clone, Serialize, Deserialize)]
    pub struct BoardBuildData {
        players: Vec<PlayerBuildData>,
        longest_road: Option<PlayerId>,
    }

    impl BoardBuildData {
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
                        establishments: player.establishments.into_iter().collect(),
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
        pub fn occupancy(&self) -> BuildDataOccupancy<'_> {
            BuildDataOccupancy { container: self }
        }

        #[inline]
        pub fn query(&self) -> BuildDataQuery<'_> {
            BuildDataQuery { container: self }
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
        pub fn by_player(&self, id: PlayerId) -> &PlayerBuildData {
            &self.players[id]
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
            build: Build,
        ) -> Result<(), BuildingError> {
            let occ = self.occupancy();

            let checker = CollisionChecker {
                other_occupancy: &occ
                    .occupancy((0..self.players.len()).filter(|id| id != &player_id)),
                this_occupancy: &occ.occupancy([player_id]),
            };

            match build {
                Build::Road(road) => {
                    if self.players[player_id].roads_count() >= PlayerBuildData::ROAD_LIMIT {
                        return Err(BuildingError::RoadLimit());
                    }
                    match self.players[player_id].roads.extend(&road.pos, &checker) {
                        Ok(_) => {
                            self.update_longest_road(player_id);
                            Ok(())
                        }
                        Err(err) => Err(BuildingError::Road(err)),
                    }
                }

                Build::Establishment(establishment) => match establishment.stage {
                    EstablishmentType::Settlement => match checker.can_place(&establishment) {
                        true if self.players[player_id].settlements_count()
                            >= PlayerBuildData::SETTLEMENT_LIMIT =>
                        {
                            Err(BuildingError::SettlementLimit())
                        }
                        true => Ok({
                            debug_assert!(
                                self.players[player_id].establishments.insert(establishment),
                                "checker malfunction"
                            );
                        }),
                        false => Err(BuildingError::Settlement()), // invalid placement for a settlement
                    },
                    EstablishmentType::City => match self.players[player_id]
                        .establishments
                        .contains(&Establishment {
                            pos: establishment.pos,
                            stage: EstablishmentType::Settlement,
                        }) {
                        true if self.players[player_id].cities_count()
                            >= PlayerBuildData::CITY_LIMIT =>
                        {
                            Err(BuildingError::CityLimit())
                        }
                        true => Ok({
                            debug_assert!(
                                self.players[player_id]
                                    .establishments
                                    .remove(&Establishment {
                                        pos: establishment.pos,
                                        stage: EstablishmentType::Settlement,
                                    }),
                                "set handling logic error"
                            );

                            assert!(
                                self.players[player_id].establishments.insert(establishment),
                                "set handling logic error"
                            );
                        }),
                        false => Err(BuildingError::City()), // no settlement to upgrade into a city
                    },
                },
            }
        }

        fn update_longest_road(&mut self, candidate: PlayerId) {
            let candidate_len = self.players[candidate].roads.find_longest_trail_length();
            if candidate_len < 5 {
                return;
            }

            match self.longest_road {
                Some(owner) => {
                    let owner_len = self.players[owner].roads.find_longest_trail_length();
                    if candidate_len > owner_len {
                        self.longest_road = Some(candidate);
                    }
                }
                None => {
                    let tied = self
                        .players
                        .iter()
                        .enumerate()
                        .filter(|(id, player)| {
                            *id != candidate
                                && player.roads.find_longest_trail_length() == candidate_len
                        })
                        .any(|_| true);
                    if !tied {
                        self.longest_road = Some(candidate);
                    }
                }
            }
        }

        pub fn try_init_place(
            &mut self,
            player_id: PlayerId,
            road: Road,
            establishment: Establishment,
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
                .is_disjoint(&checker.building_deadzone(establishment.pos));

            if !settlement_ok {
                return Err(BuildingError::InitSettlement(establishment.pos));
            }

            if self.players[player_id].settlements_count() >= PlayerBuildData::SETTLEMENT_LIMIT {
                return Err(BuildingError::SettlementLimit());
            }
            if self.players[player_id].roads_count() >= PlayerBuildData::ROAD_LIMIT {
                return Err(BuildingError::RoadLimit());
            }

            self[player_id].establishments.insert(establishment);

            let road_ok = road
                .pos
                .intersections_iter()
                .any(|v| v == establishment.pos)
                && !checker
                    .full_occupancy()
                    .roads_occupancy
                    .paths
                    .contains(&road.pos);

            if !road_ok {
                log::error!("invalid initial road placement");
                return Err(BuildingError::InitRoad(road.pos));
            }

            self[player_id].roads.add_edge(&road.pos);
            self.update_longest_road(player_id);

            Ok(())
        }
    }

    impl Index<PlayerId> for BoardBuildData {
        type Output = PlayerBuildData;

        fn index(&self, index: PlayerId) -> &Self::Output {
            &self.players[index]
        }
    }

    impl IndexMut<PlayerId> for BoardBuildData {
        fn index_mut(&mut self, index: PlayerId) -> &mut Self::Output {
            &mut self.players[index]
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Query
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Read-only query utilities over the build data.
pub mod query {
    use super::*;

    pub struct BuildDataQuery<'a> {
        pub(crate) container: &'a BoardBuildData,
    }

    impl<'a> BuildDataQuery<'a> {
        pub fn builds_on_hex(&self, hex: Hex) -> BTreeMap<PlayerId, BuildCollection> {
            self.container
                .players()
                .iter()
                .enumerate()
                .filter_map(|(player_id, player)| {
                    let establishments = player
                        .establishments
                        .iter()
                        .copied()
                        .filter(|c| c.pos.as_set().contains(&hex))
                        .collect::<Vec<_>>();

                    let roads = player
                        .roads
                        .iter()
                        .filter(|r| r.pos.as_set().contains(&hex))
                        .collect::<Vec<_>>();

                    if establishments.is_empty() && roads.is_empty() {
                        None
                    } else {
                        Some((
                            player_id,
                            BuildCollection {
                                establishments,
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
                    establishments: player.establishments.iter().copied().collect(),
                    roads: player.roads.iter().collect(),
                })
                .collect()
        }

        pub fn possible_initial_placements(
            &self,
            field: &BoardLayout,
            player_id: PlayerId,
        ) -> Vec<(Establishment, Road)> {
            let occ = self.container.occupancy();

            let checker = CollisionChecker {
                other_occupancy: &occ
                    .occupancy((0..self.container.players().len()).filter(|id| id != &player_id)),
                this_occupancy: &occ.occupancy([player_id]),
            };

            let intersections = field
                .arrangement
                .intersections()
                .into_iter()
                .collect::<Vec<_>>();

            // plan:
            // build_deadzone = full_occupancy.builds_occupancy.map(|v| v.deadzone()).union()
            // intersections = intersections.substract(build_deadzone)
            //
            // path_deadzone = occ.occupancy_full().roads_occupancy.paths()
            // poissible_placements = intersections.flat_map(|v| v.paths().substract(path_deadzone).map(|p| (v, p)))

            let build_deadzone = occ
                .occupancy_full()
                .builds_occupancy
                .iter()
                .flat_map(|v| checker.building_deadzone(*v))
                .collect::<BTreeSet<_>>();

            let available_intersections = intersections
                .into_iter()
                .filter(|v| !build_deadzone.contains(v));

            let path_deadzone = &occ.occupancy_full().roads_occupancy.paths;
            let valid_paths = field.arrangement.path_set();

            // log::debug!("build_deadzone: {:?}", build_deadzone);

            let possible_placements = available_intersections.flat_map(|v| {
                let paths = v
                    .paths()
                    .into_iter()
                    .filter(|p| !path_deadzone.contains(p) && valid_paths.contains(p));
                paths.map(move |p| (v, p))
            });

            possible_placements
                .map(|(v, p)| {
                    (
                        Establishment {
                            pos: v,
                            stage: EstablishmentType::Settlement,
                        },
                        Road { pos: p },
                    )
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::Hex;

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn path(h1: Hex, h2: Hex) -> Path {
        Path::try_from((h1, h2)).unwrap()
    }

    fn settlement(pos: Intersection) -> Establishment {
        Establishment {
            pos,
            stage: EstablishmentType::Settlement,
        }
    }

    fn city(pos: Intersection) -> Establishment {
        Establishment {
            pos,
            stage: EstablishmentType::City,
        }
    }

    #[test]
    fn road_limit_blocks_sixteenth_road() {
        let center = h(0, 0);
        let roads = center
            .paths_arr()
            .into_iter()
            .cycle()
            .take(PlayerBuildData::ROAD_LIMIT)
            .enumerate()
            .map(|(idx, pos)| Road {
                pos: if idx < 6 {
                    pos
                } else {
                    path(h(idx as i32, 0), h(idx as i32 + 1, 0))
                },
            })
            .collect();
        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![],
            roads,
        }]);

        let err = builds
            .try_build(
                0,
                Build::Road(Road {
                    pos: center.paths_arr()[0],
                }),
            )
            .expect_err("sixteenth road should exceed player inventory");

        assert!(matches!(err, BuildingError::RoadLimit()));
    }

    #[test]
    fn city_upgrade_returns_settlement_to_inventory() {
        let vertices = h(0, 0).vertices_arr();
        let mut establishments = vertices
            .into_iter()
            .take(PlayerBuildData::SETTLEMENT_LIMIT)
            .map(settlement)
            .collect::<Vec<_>>();
        establishments.extend(vertices.into_iter().take(3).map(city));

        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments,
            roads: vec![],
        }]);

        builds
            .try_build(0, Build::Establishment(city(vertices[3])))
            .expect("fourth city should be available even with five settlements on board");

        assert_eq!(
            builds[0].settlements_count(),
            PlayerBuildData::SETTLEMENT_LIMIT - 1
        );
        assert_eq!(builds[0].cities_count(), PlayerBuildData::CITY_LIMIT);
    }

    #[test]
    fn city_limit_blocks_fifth_city() {
        let vertices = h(0, 0).vertices_arr();
        let mut establishments = vertices
            .into_iter()
            .take(PlayerBuildData::CITY_LIMIT)
            .map(city)
            .collect::<Vec<_>>();
        establishments.push(settlement(vertices[4]));

        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments,
            roads: vec![],
        }]);

        let err = builds
            .try_build(0, Build::Establishment(city(vertices[4])))
            .expect_err("fifth city should exceed player inventory");

        assert!(matches!(err, BuildingError::CityLimit()));
    }

    #[test]
    fn building_fifth_road_awards_longest_road() {
        let center = h(0, 0);
        let mut paths = center.paths_arr().into_iter();
        let roads = paths
            .by_ref()
            .take(4)
            .map(|pos| Road { pos })
            .collect::<Vec<_>>();
        let fifth = paths.next().unwrap();
        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![],
            roads,
        }]);

        builds
            .try_build(0, Build::Road(Road { pos: fifth }))
            .expect("fifth connected road should be legal");

        assert_eq!(builds.longest_road(), Some(0));
    }
}
