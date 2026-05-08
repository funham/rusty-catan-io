//! Build system state, queries, and occupancy logic.
//!
//! This module defines the structures used to store player builds
//! (settlements, cities, roads), compute board occupancy, and query
//! build information during gameplay.
//!
//! The file is structured into internal modules to keep responsibilities
//! clearly separated while avoiding unnecessary file fragmentation.

use std::{
    collections::BTreeMap,
    ops::{Index, IndexMut},
};

use serde::{Deserialize, Serialize};

use crate::{
    common::SmallSet,
    gameplay::{
        constants::capacities::PLAYER_ESTABLISHMENTS_INLINE,
        field::state::{BoardLayout, BuildCollection},
        primitives::player::PlayerId,
    },
    topology::{
        HasPos, Hex, Intersection, Path,
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
/// required for placement logic.
pub mod builds {
    use super::*;

    /// Set of intersections currently occupied by builds or roads.
    /// Sorted small set optimized for the fixed-size Catan intersection domain.
    pub type IntersectionOccupancy = SmallSet<Intersection, 64>;

    /// Trait for objects that occupy intersections on the board.
    /// Used by placement logic.
    pub trait OccupyIntersection {
        fn occupancy(&self) -> IntersectionOccupancy;
    }

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
        pub vtx: Intersection,
        pub stage: EstablishmentType,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    pub struct Road {
        pub path: Path,
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

    /* Occupying impls */

    impl<T: OccupyIntersection> OccupyIntersection for &T {
        fn occupancy(&self) -> IntersectionOccupancy {
            <T as OccupyIntersection>::occupancy(&self)
        }
    }

    /// Settlement occupies a single intersection.
    impl OccupyIntersection for Establishment {
        fn occupancy(&self) -> IntersectionOccupancy {
            IntersectionOccupancy::from([self.vtx])
        }
    }

    /// Road occupies both intersections of its path.
    impl OccupyIntersection for Road {
        fn occupancy(&self) -> IntersectionOccupancy {
            self.path.intersections_iter().collect()
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Occupancy
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Structures representing board occupancy used during placement checks.
pub mod occupancy {
    use super::*;

    pub type PathSet = SmallSet<Path, 72>;

    #[derive(Debug, Default)]
    pub struct PathOccupancy {
        pub occupancy: IntersectionOccupancy,
        pub paths: PathSet,
    }

    impl PathOccupancy {
        /// Union of two road occupancy sets.
        pub fn union(&self, other: &Self) -> Self {
            Self {
                occupancy: self.occupancy.union(&other.occupancy),
                paths: self.paths.union(&other.paths),
            }
        }
    }

    /// Combined occupancy structure used in placement checks.
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
                builds_occupancy: self.builds_occupancy.union(&other.builds_occupancy),
                roads_occupancy: PathOccupancy {
                    occupancy: self
                        .roads_occupancy
                        .occupancy
                        .union(&other.roads_occupancy.occupancy),
                    paths: self
                        .roads_occupancy
                        .paths
                        .union(&other.roads_occupancy.paths),
                },
            }
        }
    }

    /// Allows retrieving correct occupancy type depending on build type.
    pub trait OccupancyGetter: OccupyIntersection {
        type OccupancyType;
        fn get<'a>(x: &'a AggregateOccupancy) -> &'a Self::OccupancyType;
    }

    impl OccupancyGetter for Road {
        type OccupancyType = PathOccupancy;
        fn get<'a>(x: &'a AggregateOccupancy) -> &'a Self::OccupancyType {
            &x.roads_occupancy
        }
    }

    impl<T: OccupyIntersection + HasPos<Pos = Intersection>> OccupancyGetter for T {
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

                    player.establishments.iter().map(|s| s.vtx)
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
        pub establishments: SmallSet<Establishment, PLAYER_ESTABLISHMENTS_INLINE>,
        pub roads: graph::RoadGraph,
    }

    impl PlayerBuildData {
        pub const ROAD_LIMIT: usize = 15;
        pub const SETTLEMENT_LIMIT: usize = 5;
        pub const CITY_LIMIT: usize = 5;

        pub fn generic_occupancy<Builds, BuildItem>(builds: Builds) -> IntersectionOccupancy
        where
            Builds: Iterator<Item = BuildItem>,
            BuildItem: OccupyIntersection,
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
                paths: self.roads.edges().iter().copied().collect(),
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
                            player.roads.into_iter().map(|road| road.path),
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
            self.can_build(player_id, build)?;

            match build {
                Build::Road(road) => {
                    self.players[player_id]
                        .roads
                        .insert_validated_edge(&road.path);
                    self.update_longest_road(player_id);
                    Ok(())
                }

                Build::Establishment(establishment) => match establishment.stage {
                    EstablishmentType::Settlement => {
                        if self.players[player_id].establishments.insert(establishment) {
                            Ok(())
                        } else {
                            Err(BuildingError::Settlement())
                        }
                    }
                    EstablishmentType::City => {
                        let settlement = Establishment {
                            vtx: establishment.vtx,
                            stage: EstablishmentType::Settlement,
                        };

                        if !self.players[player_id].establishments.remove(&settlement) {
                            return Err(BuildingError::City());
                        }

                        if self.players[player_id].establishments.insert(establishment) {
                            Ok(())
                        } else {
                            Err(BuildingError::City())
                        }
                    }
                },
            }
        }

        pub fn can_build(&self, player_id: PlayerId, build: Build) -> Result<(), BuildingError> {
            match build {
                Build::Road(road) => self.can_place_road(player_id, road.path),
                Build::Establishment(establishment) => match establishment.stage {
                    EstablishmentType::Settlement => {
                        self.can_place_settlement(player_id, establishment.vtx)
                    }
                    EstablishmentType::City => self.can_place_city(player_id, establishment.vtx),
                },
            }
        }

        pub fn can_place_road(&self, player_id: PlayerId, path: Path) -> Result<(), BuildingError> {
            if self.players[player_id].roads_count() >= PlayerBuildData::ROAD_LIMIT {
                return Err(BuildingError::RoadLimit());
            }

            if self.is_road_occupied(path) {
                return Err(BuildingError::Road(EdgeInsertationError));
            }

            if self.player_has_road_touching_path_at_unblocked_intersection(player_id, path) {
                Ok(())
            } else {
                Err(BuildingError::Road(EdgeInsertationError))
            }
        }

        pub fn can_place_road_with_extra_roads(
            &self,
            player_id: PlayerId,
            path: Path,
            extra_roads: &[Path],
        ) -> Result<(), BuildingError> {
            self.can_place_road_with_extra_roads_iter(player_id, path, extra_roads.iter().copied())
        }

        pub fn can_place_road_with_extra_roads_iter<ExtraRoads>(
            &self,
            player_id: PlayerId,
            path: Path,
            extra_roads: ExtraRoads,
        ) -> Result<(), BuildingError>
        where
            ExtraRoads: IntoIterator<Item = Path> + Clone,
        {
            let extra_count = extra_roads.clone().into_iter().count();
            if self.players[player_id].roads_count() + extra_count >= PlayerBuildData::ROAD_LIMIT {
                return Err(BuildingError::RoadLimit());
            }

            if self.is_road_occupied(path) || extra_roads.clone().into_iter().any(|p| p == path) {
                return Err(BuildingError::Road(EdgeInsertationError));
            }

            if self.player_has_road_touching_path_at_unblocked_intersection(player_id, path)
                || extra_roads
                    .clone()
                    .into_iter()
                    .any(|extra| self.roads_touch_at_unblocked_intersection(player_id, path, extra))
            {
                Ok(())
            } else {
                Err(BuildingError::Road(EdgeInsertationError))
            }
        }

        pub fn road_extension_candidates(
            &self,
            player_id: PlayerId,
            board_paths: &[Path],
        ) -> PathSet {
            self.road_extension_candidates_with_extra_roads(player_id, [], board_paths)
        }

        pub fn road_extension_candidates_with_extra_roads<ExtraRoads>(
            &self,
            player_id: PlayerId,
            extra_roads: ExtraRoads,
            board_paths: &[Path],
        ) -> PathSet
        where
            ExtraRoads: IntoIterator<Item = Path> + Clone,
        {
            if player_id >= self.players.len()
                || self.players[player_id].roads_count() + extra_roads.clone().into_iter().count()
                    >= PlayerBuildData::ROAD_LIMIT
            {
                return PathSet::new();
            }

            let extra_roads_set = extra_roads.clone().into_iter().collect::<PathSet>();
            let mut frontier = SmallSet::<Intersection, 64>::new();

            for road in self.players[player_id].roads.edges().iter().copied() {
                for intersection in road.intersections() {
                    if !self.opponent_has_establishment_at(player_id, intersection) {
                        frontier.insert(intersection);
                    }
                }
            }

            for road in extra_roads_set.iter().copied() {
                for intersection in road.intersections() {
                    if !self.opponent_has_establishment_at(player_id, intersection) {
                        frontier.insert(intersection);
                    }
                }
            }

            let board_paths = board_paths.iter().copied().collect::<PathSet>();
            let mut candidates = PathSet::new();

            for intersection in frontier {
                for candidate in intersection.paths_arr() {
                    if !board_paths.contains(&candidate)
                        || self.is_road_occupied(candidate)
                        || extra_roads_set.contains(&candidate)
                    {
                        continue;
                    }

                    if self
                        .can_place_road_with_extra_roads_iter(
                            player_id,
                            candidate,
                            extra_roads_set.clone(),
                        )
                        .is_ok()
                    {
                        candidates.insert(candidate);
                    }
                }
            }

            candidates
        }

        pub fn can_place_settlement(
            &self,
            player_id: PlayerId,
            pos: Intersection,
        ) -> Result<(), BuildingError> {
            self.can_place_settlement_with_extra_roads_iter(player_id, pos, [])
        }

        pub fn can_place_settlement_with_extra_roads_iter<ExtraRoads>(
            &self,
            player_id: PlayerId,
            pos: Intersection,
            extra_roads: ExtraRoads,
        ) -> Result<(), BuildingError>
        where
            ExtraRoads: IntoIterator<Item = Path>,
        {
            if !(self.player_has_road_at_intersection(player_id, pos)
                || extra_roads
                    .into_iter()
                    .any(|path| path_contains_intersection(path, pos)))
                || self.has_establishment_in_deadzone(pos)
            {
                return Err(BuildingError::Settlement());
            }
            if self.players[player_id].settlements_count() >= PlayerBuildData::SETTLEMENT_LIMIT {
                return Err(BuildingError::SettlementLimit());
            }

            Ok(())
        }

        pub fn can_place_city(
            &self,
            player_id: PlayerId,
            pos: Intersection,
        ) -> Result<(), BuildingError> {
            let settlement = Establishment {
                vtx: pos,
                stage: EstablishmentType::Settlement,
            };
            let city = Establishment {
                vtx: pos,
                stage: EstablishmentType::City,
            };

            if !self.players[player_id].establishments.contains(&settlement) {
                return Err(BuildingError::City());
            }
            if self.players[player_id].cities_count() >= PlayerBuildData::CITY_LIMIT {
                return Err(BuildingError::CityLimit());
            }
            if self.players[player_id].establishments.contains(&city) {
                return Err(BuildingError::City());
            }

            Ok(())
        }

        pub(crate) fn is_road_occupied(&self, path: Path) -> bool {
            self.players
                .iter()
                .any(|player| player.roads.contains_edge(path))
        }

        fn player_has_road_at_intersection(
            &self,
            player_id: PlayerId,
            intersection: Intersection,
        ) -> bool {
            self.players[player_id].roads.touches(intersection)
        }

        fn player_has_road_touching_path_at_unblocked_intersection(
            &self,
            player_id: PlayerId,
            path: Path,
        ) -> bool {
            path.intersections().into_iter().any(|intersection| {
                self.players[player_id].roads.touches(intersection)
                    && !self.opponent_has_establishment_at(player_id, intersection)
            })
        }

        fn roads_touch_at_unblocked_intersection(
            &self,
            player_id: PlayerId,
            candidate: Path,
            connected: Path,
        ) -> bool {
            let Some([a, b, c]) = touching_intersection_hexes(candidate, connected) else {
                return false;
            };

            !self.opponent_has_establishment_on_hexes(player_id, [a, b, c])
        }

        fn opponent_has_establishment_at(
            &self,
            player_id: PlayerId,
            intersection: Intersection,
        ) -> bool {
            self.players_indexed()
                .filter(|(other_id, _)| *other_id != player_id)
                .any(|(_, player)| {
                    player
                        .establishments
                        .iter()
                        .any(|establishment| establishment.vtx == intersection)
                })
        }

        fn opponent_has_establishment_on_hexes(
            &self,
            player_id: PlayerId,
            hexes: [Hex; 3],
        ) -> bool {
            self.players_indexed()
                .filter(|(other_id, _)| *other_id != player_id)
                .any(|(_, player)| {
                    player.establishments.iter().any(|establishment| {
                        let establishment_hexes = establishment.vtx.as_arr();
                        hexes.iter().all(|hex| establishment_hexes.contains(hex))
                    })
                })
        }

        pub(crate) fn has_establishment_in_deadzone(&self, pos: Intersection) -> bool {
            self.players.iter().any(|player| {
                player
                    .establishments
                    .iter()
                    .any(|establishment| intersections_same_or_adjacent(establishment.vtx, pos))
            })
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
            if self.has_establishment_in_deadzone(establishment.vtx) {
                return Err(BuildingError::InitSettlement(establishment.vtx));
            }

            if self.players[player_id].settlements_count() >= PlayerBuildData::SETTLEMENT_LIMIT {
                return Err(BuildingError::SettlementLimit());
            }
            if self.players[player_id].roads_count() >= PlayerBuildData::ROAD_LIMIT {
                return Err(BuildingError::RoadLimit());
            }

            self[player_id].establishments.insert(establishment);

            let road_ok = road
                .path
                .intersections_iter()
                .any(|v| v == establishment.vtx)
                && !self.is_road_occupied(road.path);

            if !road_ok {
                log::error!("invalid initial road placement");
                return Err(BuildingError::InitRoad(road.path));
            }

            self[player_id].roads.insert_validated_edge(&road.path);
            self.update_longest_road(player_id);

            Ok(())
        }
    }

    fn path_contains_intersection(path: Path, intersection: Intersection) -> bool {
        let [a, b] = path.as_arr();
        let intersection_hexes = intersection.as_arr();
        intersection_hexes.contains(&a) && intersection_hexes.contains(&b)
    }

    fn touching_intersection_hexes(candidate: Path, connected: Path) -> Option<[Hex; 3]> {
        let [a, b] = candidate.as_arr();
        let [c, d] = connected.as_arr();

        let third = if c == a || c == b {
            d
        } else if d == a || d == b {
            c
        } else {
            return None;
        };

        (third.are_neighbors(&a) && third.are_neighbors(&b)).then_some([a, b, third])
    }

    fn intersections_same_or_adjacent(a: Intersection, b: Intersection) -> bool {
        let b_hexes = b.as_arr();
        a.as_arr()
            .into_iter()
            .filter(|hex| b_hexes.contains(hex))
            .take(2)
            .count()
            == 2
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
                        .filter(|c| c.vtx.as_set().contains(&hex))
                        .collect::<Vec<_>>();

                    let roads = player
                        .roads
                        .iter()
                        .filter(|r| r.path.as_set().contains(&hex))
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
            _player_id: PlayerId,
        ) -> Vec<(Establishment, Road)> {
            let intersections = field
                .arrangement
                .intersections()
                .into_iter()
                .collect::<Vec<_>>();

            let available_intersections = intersections
                .into_iter()
                .filter(|v| !self.container.has_establishment_in_deadzone(*v));

            let valid_paths = field.arrangement.path_set();

            let possible_placements = available_intersections.flat_map(|v| {
                let paths = v
                    .paths_arr()
                    .into_iter()
                    .filter(|p| !self.container.is_road_occupied(*p) && valid_paths.contains(p));
                paths.map(move |p| (v, p))
            });

            possible_placements
                .map(|(v, p)| {
                    (
                        Establishment {
                            vtx: v,
                            stage: EstablishmentType::Settlement,
                        },
                        Road { path: p },
                    )
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::field::state::{BoardLayout, FieldBuildParam};
    use crate::topology::Hex;

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn path(h1: Hex, h2: Hex) -> Path {
        Path::try_from((h1, h2)).unwrap()
    }

    fn settlement(pos: Intersection) -> Establishment {
        Establishment {
            vtx: pos,
            stage: EstablishmentType::Settlement,
        }
    }

    fn city(pos: Intersection) -> Establishment {
        Establishment {
            vtx: pos,
            stage: EstablishmentType::City,
        }
    }

    fn default_board_paths() -> Vec<Path> {
        BoardLayout::new(FieldBuildParam::default())
            .paths()
            .to_vec()
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
                path: if idx < 6 {
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
                    path: center.paths_arr()[0],
                }),
            )
            .expect_err("sixteenth road should exceed player inventory");

        assert!(matches!(err, BuildingError::RoadLimit()));
    }

    #[test]
    fn road_cannot_be_built_on_own_existing_road() {
        let existing = path(h(0, 0), h(1, 0));
        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![],
            roads: vec![Road { path: existing }],
        }]);

        let err = builds
            .try_build(0, Build::Road(Road { path: existing }))
            .expect_err("road path should already be occupied");

        assert!(matches!(err, BuildingError::Road(_)));
        assert_eq!(builds[0].roads_count(), 1);
    }

    #[test]
    fn road_cannot_be_built_on_opponent_existing_road() {
        let existing = path(h(0, 0), h(1, 0));
        let connecting = path(h(1, 0), h(1, 1));
        let mut builds = BoardBuildData::from_build_collections(vec![
            BuildCollection {
                establishments: vec![],
                roads: vec![Road { path: connecting }],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![Road { path: existing }],
            },
        ]);

        let err = builds
            .try_build(0, Build::Road(Road { path: existing }))
            .expect_err("opponent road path should already be occupied");

        assert!(matches!(err, BuildingError::Road(_)));
        assert_eq!(builds[0].roads_count(), 1);
        assert_eq!(builds[1].roads_count(), 1);
    }

    #[test]
    fn road_extension_candidates_are_limited_to_unblocked_frontier() {
        let existing = path(h(0, 0), h(1, 0));
        let builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![],
            roads: vec![Road { path: existing }],
        }]);
        let board_paths = default_board_paths();

        let candidates = builds.road_extension_candidates(0, &board_paths);

        assert!(!candidates.contains(&existing));
        for intersection in existing.intersections() {
            for candidate in intersection.paths_arr() {
                if candidate != existing && board_paths.contains(&candidate) {
                    assert!(candidates.contains(&candidate));
                }
            }
        }
    }

    #[test]
    fn road_extension_candidates_do_not_extend_through_opponent_establishment() {
        let existing = path(h(0, 0), h(1, 0));
        let blocked = existing.intersections()[0];
        let open = existing.intersections()[1];
        let builds = BoardBuildData::from_build_collections(vec![
            BuildCollection {
                establishments: vec![],
                roads: vec![Road { path: existing }],
            },
            BuildCollection {
                establishments: vec![settlement(blocked)],
                roads: vec![],
            },
        ]);
        let board_paths = default_board_paths();

        let candidates = builds.road_extension_candidates(0, &board_paths);

        for candidate in blocked.paths_arr() {
            if candidate != existing {
                assert!(!candidates.contains(&candidate));
            }
        }
        assert!(
            open.paths_arr()
                .into_iter()
                .any(|candidate| candidate != existing && candidates.contains(&candidate))
        );
    }

    #[test]
    fn settlement_build_is_persisted() {
        let road = Road {
            path: path(h(0, 0), h(1, 0)),
        };
        let settlement = settlement(road.path.intersections()[0]);
        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![],
            roads: vec![road],
        }]);

        builds
            .try_build(0, Build::Establishment(settlement))
            .expect("connected settlement should be legal");

        assert!(builds[0].establishments.contains(&settlement));
        assert_eq!(builds[0].settlements_count(), 1);
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
        assert_eq!(builds[0].cities_count(), 4);
        assert!(!builds[0].establishments.contains(&settlement(vertices[3])));
        assert!(builds[0].establishments.contains(&city(vertices[3])));
    }

    #[test]
    fn city_upgrade_cannot_be_repeated() {
        let pos = h(0, 0).vertices_arr()[0];
        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![settlement(pos)],
            roads: vec![],
        }]);

        builds
            .try_build(0, Build::Establishment(city(pos)))
            .expect("first city upgrade should be legal");
        let err = builds
            .try_build(0, Build::Establishment(city(pos)))
            .expect_err("city upgrade should require an existing settlement");

        assert!(matches!(err, BuildingError::City()));
        assert_eq!(builds[0].settlements_count(), 0);
        assert_eq!(builds[0].cities_count(), 1);
    }

    #[test]
    fn city_limit_allows_fifth_city() {
        let vertices = h(0, 0).vertices_arr();
        let mut establishments = vertices
            .into_iter()
            .take(PlayerBuildData::CITY_LIMIT - 1)
            .map(city)
            .collect::<Vec<_>>();
        establishments.push(settlement(vertices[4]));

        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments,
            roads: vec![],
        }]);

        builds
            .try_build(0, Build::Establishment(city(vertices[4])))
            .expect("fifth city should be legal and win the game at controller level");

        assert_eq!(builds[0].cities_count(), PlayerBuildData::CITY_LIMIT);
    }

    #[test]
    fn building_fifth_road_awards_longest_road() {
        let center = h(0, 0);
        let mut paths = center.paths_arr().into_iter();
        let roads = paths
            .by_ref()
            .take(4)
            .map(|pos| Road { path: pos })
            .collect::<Vec<_>>();
        let fifth = paths.next().unwrap();
        let mut builds = BoardBuildData::from_build_collections(vec![BuildCollection {
            establishments: vec![],
            roads,
        }]);

        builds
            .try_build(0, Build::Road(Road { path: fifth }))
            .expect("fifth connected road should be legal");

        assert_eq!(builds.longest_road(), Some(0));
    }
}
