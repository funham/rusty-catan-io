use crate::{
    algorithm,
    common::SmallSet,
    gameplay::{
        constants::capacities::PLAYER_PORTS_INLINE,
        game::state::{GameState, TableState},
        primitives::{
            PortKind,
            build::{
                Build, Establishment, EstablishmentType, PathSet, PlayerBuildData,
                road_extension_candidates_from_frontier,
            },
            dev_card::DevCardUsage,
            player::{PlayerId, player_ids},
        },
    },
    topology::{Intersection, Path},
};
use smallvec::SmallVec;

type PlayerIndexed<T> = SmallVec<[T; 8]>;
type IntersectionSet = SmallSet<Intersection, 64>;
type OpponentBlockers = SmallSet<Intersection, 32>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameIndex {
    pub longest_road_lengths: Vec<u16>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
    pub ports_acquired: Vec<SmallSet<PortKind, PLAYER_PORTS_INLINE>>,
    pub occupied_roads: PathSet,
    pub occupied_establishments: IntersectionSet,
    pub deadzone_intersections: IntersectionSet,
    pub opponent_establishments: PlayerIndexed<OpponentBlockers>,
    pub road_frontiers: PlayerIndexed<IntersectionSet>,
    pub legal_road_candidates: PlayerIndexed<PathSet>,
}

impl GameIndex {
    pub fn rebuild(state: &GameState) -> Self {
        Self::rebuild_table(&state.table)
    }

    pub fn rebuild_table(state: &TableState) -> Self {
        let longest_road_lengths = Self::longest_road_lengths(state);
        let longest_road_owner =
            Self::longest_road_owner(state.builds.longest_road(), &longest_road_lengths);
        let occupied_roads = Self::occupied_roads(state);
        let occupied_establishments = Self::occupied_establishments(state);
        let deadzone_intersections = Self::deadzone_intersections(&occupied_establishments);
        let opponent_establishments = Self::opponent_establishments_by_player(state);
        let road_frontiers = Self::road_frontiers(state, &opponent_establishments);
        let legal_road_candidates =
            Self::legal_road_candidates(state, &occupied_roads, &road_frontiers);

        Self {
            longest_road_lengths,
            longest_road_owner,
            largest_army_owner: state.players.best_army(),
            ports_acquired: Self::get_ports_acquired(state),
            occupied_roads,
            occupied_establishments,
            deadzone_intersections,
            opponent_establishments,
            road_frontiers,
            legal_road_candidates,
        }
    }

    fn get_ports_acquired(state: &TableState) -> Vec<SmallSet<PortKind, PLAYER_PORTS_INLINE>> {
        algorithm::get_ports_acquired(state.board.ports_intersection(), &state.builds)
    }

    fn occupied_roads(state: &TableState) -> PathSet {
        state
            .builds
            .players()
            .iter()
            .flat_map(|player| player.roads.edges().iter().copied())
            .collect()
    }

    fn occupied_establishments(state: &TableState) -> IntersectionSet {
        state
            .builds
            .players()
            .iter()
            .flat_map(|player| player.establishments.iter().map(|est| est.vtx))
            .collect()
    }

    fn deadzone_intersections(occupied: &IntersectionSet) -> IntersectionSet {
        let mut deadzone = IntersectionSet::new();
        for &intersection in occupied {
            deadzone.insert(intersection);
            for neighbor in intersection.neighbors_arr() {
                deadzone.insert(neighbor);
            }
        }
        deadzone
    }

    fn opponent_establishments_by_player(state: &TableState) -> PlayerIndexed<OpponentBlockers> {
        player_ids(state.players.count())
            .map(|player_id| Self::opponent_establishments(state, player_id))
            .collect()
    }

    fn road_frontiers(
        state: &TableState,
        opponent_establishments: &[OpponentBlockers],
    ) -> PlayerIndexed<IntersectionSet> {
        player_ids(state.players.count())
            .map(|player_id| {
                let mut frontier = IntersectionSet::new();
                for road in state.builds[player_id].roads.edges() {
                    for intersection in road.intersections() {
                        if !opponent_establishments[player_id.index()].contains(&intersection) {
                            frontier.insert(intersection);
                        }
                    }
                }
                frontier
            })
            .collect()
    }

    fn legal_road_candidates(
        state: &TableState,
        occupied_roads: &PathSet,
        road_frontiers: &[IntersectionSet],
    ) -> PlayerIndexed<PathSet> {
        player_ids(state.players.count())
            .map(|player_id| {
                if state.builds[player_id].roads_count() >= PlayerBuildData::ROAD_LIMIT {
                    return PathSet::new();
                }

                road_extension_candidates_from_frontier(
                    &road_frontiers[player_id.index()],
                    occupied_roads,
                    |intersection| state.board.incident_paths(intersection),
                )
            })
            .collect()
    }

    fn refresh_build_legality_cache(&mut self, state: &TableState) {
        self.occupied_roads = Self::occupied_roads(state);
        self.occupied_establishments = Self::occupied_establishments(state);
        self.deadzone_intersections = Self::deadzone_intersections(&self.occupied_establishments);
        self.opponent_establishments = Self::opponent_establishments_by_player(state);
        self.road_frontiers = Self::road_frontiers(state, &self.opponent_establishments);
        self.legal_road_candidates =
            Self::legal_road_candidates(state, &self.occupied_roads, &self.road_frontiers);
    }

    pub fn legal_road_candidates_with_extra_roads(
        &self,
        state: &TableState,
        player_id: PlayerId,
        extra_roads: &[Path],
    ) -> PathSet {
        if state.builds[player_id].roads_count() + extra_roads.len() >= PlayerBuildData::ROAD_LIMIT
        {
            return PathSet::new();
        }

        let extra_roads_set = extra_roads.iter().copied().collect::<PathSet>();
        let occupied = self.occupied_roads.union(&extra_roads_set);
        let mut frontier = self.road_frontiers[player_id.index()].clone();
        for road in extra_roads_set.iter() {
            for intersection in road.intersections() {
                if !self.opponent_establishments[player_id.index()].contains(&intersection) {
                    frontier.insert(intersection);
                }
            }
        }

        road_extension_candidates_from_frontier(&frontier, &occupied, |intersection| {
            state.board.incident_paths(intersection)
        })
    }

    pub fn refresh_after_build(
        &mut self,
        state: &GameState,
        player_id: impl Into<PlayerId>,
        build: Build,
    ) {
        let player_id = player_id.into();
        match build {
            Build::Road(_) => {
                self.refresh_longest_road_length(state, player_id);
            }
            Build::Establishment(establishment) => {
                if establishment.stage == EstablishmentType::Settlement {
                    self.refresh_player_ports_after_settlement(state, player_id, establishment);
                    self.refresh_opponents_blocked_by_settlement(state, player_id, establishment);
                }
            }
        }

        self.refresh_longest_road_owner();
        self.refresh_build_legality_cache(state);
    }

    pub fn refresh_after_roadbuild(
        &mut self,
        state: &GameState,
        player_id: impl Into<PlayerId>,
        _roads: [Path; 2],
    ) {
        let player_id = player_id.into();
        self.refresh_longest_road_length(state, player_id);
        self.longest_road_owner =
            Self::longest_road_owner(state.builds.longest_road(), &self.longest_road_lengths);
        self.refresh_build_legality_cache(state);
    }

    pub fn refresh_after_dev_card(
        &mut self,
        state: &GameState,
        player_id: impl Into<PlayerId>,
        usage: &DevCardUsage,
    ) {
        let player_id = player_id.into();
        match usage {
            DevCardUsage::Knight { .. } => {}
            DevCardUsage::RoadBuild(roads) => {
                self.refresh_after_roadbuild(state, player_id, *roads);
            }
            DevCardUsage::YearOfPlenty(_) | DevCardUsage::Monopoly(_) => {}
        }
        self.largest_army_owner = state.players.best_army();
        self.longest_road_owner =
            Self::longest_road_owner(state.builds.longest_road(), &self.longest_road_lengths);
    }

    fn longest_road_lengths(state: &TableState) -> Vec<u16> {
        player_ids(state.players.count())
            .map(|player_id| {
                let blockers = Self::opponent_establishments(state, player_id);
                state.builds[player_id]
                    .roads
                    .find_longest_trail_length_with_blockers(&blockers) as u16
            })
            .collect()
    }

    fn opponent_establishments(
        state: &TableState,
        player_id: PlayerId,
    ) -> SmallSet<Intersection, 32> {
        state
            .builds
            .players_indexed()
            .filter(|(other_id, _)| *other_id != player_id)
            .flat_map(|(_, player)| player.establishments.iter().map(|build| build.vtx))
            .collect()
    }

    fn refresh_longest_road_length(&mut self, state: &TableState, player_id: PlayerId) {
        let blockers = Self::opponent_establishments(state, player_id);
        self.longest_road_lengths[player_id.index()] = state.builds[player_id]
            .roads
            .find_longest_trail_length_with_blockers(&blockers)
            as u16;
    }

    fn refresh_player_ports_after_settlement(
        &mut self,
        state: &TableState,
        player_id: PlayerId,
        settlement: Establishment,
    ) {
        if let Some(port) = state.board.ports_intersection().get(&settlement.vtx) {
            self.ports_acquired[player_id.index()].insert(*port);
        }
    }

    fn refresh_opponents_blocked_by_settlement(
        &mut self,
        state: &TableState,
        player_id: PlayerId,
        settlement: Establishment,
    ) {
        for opponent in player_ids(state.players.count()) {
            if opponent != player_id
                && Self::player_has_road_touching(state, opponent, settlement.vtx)
            {
                self.refresh_longest_road_length(state, opponent);
            }
        }
    }

    fn refresh_longest_road_owner(&mut self) {
        self.longest_road_owner =
            Self::longest_road_owner(self.longest_road_owner, &self.longest_road_lengths);
    }

    fn player_has_road_touching(
        state: &TableState,
        player_id: PlayerId,
        intersection: Intersection,
    ) -> bool {
        state.builds[player_id]
            .roads
            .edges()
            .iter()
            .any(|road| Self::path_contains(*road, intersection))
    }

    fn path_contains(path: Path, intersection: Intersection) -> bool {
        let [a, b] = path.as_arr();
        let intersection_hexes = intersection.as_arr();
        intersection_hexes.contains(&a) && intersection_hexes.contains(&b)
    }

    fn longest_road_owner(
        current_owner: Option<PlayerId>,
        longest_road_lengths: &[u16],
    ) -> Option<PlayerId> {
        const MIN_LONGEST_ROAD: u16 = 5;

        if let Some(owner) = current_owner {
            let owner_len = longest_road_lengths[owner.index()];
            if owner_len >= MIN_LONGEST_ROAD
                && longest_road_lengths
                    .iter()
                    .enumerate()
                    .all(|(id, &len)| id == owner.index() || len <= owner_len)
            {
                return Some(owner);
            }
        }

        let best_len = longest_road_lengths.iter().copied().max().unwrap_or(0);
        if best_len < MIN_LONGEST_ROAD {
            return None;
        }

        let mut best_players = longest_road_lengths
            .iter()
            .enumerate()
            .filter(|&(_, &len)| len == best_len)
            .map(|(id, _)| PlayerId::try_from(id).expect("player count should fit in u8"));

        let best = best_players.next()?;
        if best_players.next().is_none() {
            Some(best)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gameplay::{
            field::state::BuildCollection,
            game::state::SetupGameState,
            primitives::{
                build::{BoardBuildData, Establishment, EstablishmentType, Road},
                dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
                resource::Resource,
            },
        },
        topology::{Hex, Path},
    };

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn path(h1: Hex, h2: Hex) -> Path {
        Path::try_from((h1, h2)).unwrap()
    }

    fn empty_build_collections() -> Vec<BuildCollection> {
        vec![BuildCollection::default(); 4]
    }

    const P0: PlayerId = PlayerId::new(0);

    fn assert_matches_rebuild(index: &GameIndex, state: &GameState) {
        assert_eq!(index, &GameIndex::rebuild(state));
    }

    #[test]
    fn indexed_longest_road_respects_opponent_settlement_blockers() {
        let mut state = SetupGameState::default().finish();
        let blocker = path(h(0, 0), h(1, 0)).intersections()[0];
        let player_roads = h(0, 0)
            .neighbors()
            .into_iter()
            .map(|neighbor| Road {
                path: path(h(0, 0), neighbor),
            })
            .collect();

        state.builds = BoardBuildData::from_build_collections(vec![
            BuildCollection {
                establishments: vec![],
                roads: player_roads,
            },
            BuildCollection {
                establishments: vec![Establishment {
                    vtx: blocker,
                    stage: EstablishmentType::Settlement,
                }],
                roads: vec![],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
        ]);

        let index = GameIndex::rebuild(&state);

        assert_eq!(index.longest_road_lengths[0], 5);
        assert_eq!(index.longest_road_owner, Some(P0));
    }

    #[test]
    fn incremental_settlement_refresh_matches_full_rebuild_for_longest_road_blocker() {
        let mut state = SetupGameState::default().finish();
        let blocker = path(h(0, 0), h(1, 0)).intersections()[0];
        let player_roads = h(0, 0)
            .neighbors()
            .into_iter()
            .map(|neighbor| Road {
                path: path(h(0, 0), neighbor),
            })
            .collect::<Vec<_>>();

        state.builds = BoardBuildData::from_build_collections(vec![
            BuildCollection {
                establishments: vec![],
                roads: player_roads.clone(),
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
        ]);
        let mut incremental = GameIndex::rebuild(&state);

        let settlement = Establishment {
            vtx: blocker,
            stage: EstablishmentType::Settlement,
        };
        state.builds = BoardBuildData::from_build_collections(vec![
            BuildCollection {
                establishments: vec![],
                roads: player_roads,
            },
            BuildCollection {
                establishments: vec![settlement],
                roads: vec![],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
            BuildCollection {
                establishments: vec![],
                roads: vec![],
            },
        ]);

        incremental.refresh_after_build(&state, 1, Build::Establishment(settlement));
        let rebuilt = GameIndex::rebuild(&state);

        assert_eq!(
            incremental.longest_road_lengths,
            rebuilt.longest_road_lengths
        );
        assert_eq!(incremental.longest_road_owner, rebuilt.longest_road_owner);
        assert_eq!(incremental.ports_acquired, rebuilt.ports_acquired);
    }

    #[test]
    fn incremental_road_refresh_matches_full_rebuild() {
        let mut state = SetupGameState::default().finish();
        let mut incremental = GameIndex::rebuild(&state);
        let road = Road {
            path: path(h(0, 0), h(1, 0)),
        };
        let mut builds = empty_build_collections();
        builds[0].roads.push(road);
        state.builds = BoardBuildData::from_build_collections(builds);

        incremental.refresh_after_build(&state, 0, Build::Road(road));

        assert_matches_rebuild(&incremental, &state);
    }

    #[test]
    fn indexed_road_candidates_match_direct_generation_after_rebuild() {
        let mut state = SetupGameState::default().finish();
        let road = Road {
            path: path(h(0, 0), h(1, 0)),
        };
        let mut builds = empty_build_collections();
        builds[0].roads.push(road);
        state.builds = BoardBuildData::from_build_collections(builds);

        let index = GameIndex::rebuild(&state);
        let direct = state
            .builds
            .road_extension_candidates(P0, state.board.paths());

        assert_eq!(index.legal_road_candidates[P0.index()], direct);
    }

    #[test]
    fn indexed_road_candidates_match_direct_generation_after_incremental_road() {
        let mut state = SetupGameState::default().finish();
        let mut index = GameIndex::rebuild(&state);
        let road = Road {
            path: path(h(0, 0), h(1, 0)),
        };
        let mut builds = empty_build_collections();
        builds[0].roads.push(road);
        state.builds = BoardBuildData::from_build_collections(builds);

        index.refresh_after_build(&state, P0, Build::Road(road));

        assert_matches_rebuild(&index, &state);
        assert_eq!(
            index.legal_road_candidates[P0.index()],
            state
                .builds
                .road_extension_candidates(P0, state.board.paths())
        );
    }

    #[test]
    fn indexed_deadzone_contains_settlement_and_neighbors() {
        let mut state = SetupGameState::default().finish();
        let pos = path(h(0, 0), h(1, 0)).intersections()[0];
        let settlement = Establishment {
            vtx: pos,
            stage: EstablishmentType::Settlement,
        };
        let mut builds = empty_build_collections();
        builds[0].establishments.push(settlement);
        state.builds = BoardBuildData::from_build_collections(builds);

        let index = GameIndex::rebuild(&state);

        assert!(index.deadzone_intersections.contains(&pos));
        for neighbor in pos.neighbors_arr() {
            assert!(index.deadzone_intersections.contains(&neighbor));
        }
    }

    #[test]
    fn incremental_settlement_refresh_updates_ports_and_matches_full_rebuild() {
        let mut state = SetupGameState::default().finish();
        let mut incremental = GameIndex::rebuild(&state);
        let pos = *state
            .board
            .ports_intersection()
            .keys()
            .next()
            .expect("default board has ports");
        let settlement = Establishment {
            vtx: pos,
            stage: EstablishmentType::Settlement,
        };
        let mut builds = empty_build_collections();
        builds[0].establishments.push(settlement);
        state.builds = BoardBuildData::from_build_collections(builds);

        incremental.refresh_after_build(&state, 0, Build::Establishment(settlement));

        assert_matches_rebuild(&incremental, &state);
        assert!(!incremental.ports_acquired[0].is_empty());
    }

    #[test]
    fn incremental_city_refresh_matches_full_rebuild() {
        let mut state = SetupGameState::default().finish();
        let pos = path(h(0, 0), h(1, 0)).intersections()[0];
        let settlement = Establishment {
            vtx: pos,
            stage: EstablishmentType::Settlement,
        };
        let mut builds = empty_build_collections();
        builds[0].establishments.push(settlement);
        state.builds = BoardBuildData::from_build_collections(builds);
        let mut incremental = GameIndex::rebuild(&state);

        let city = Establishment {
            vtx: pos,
            stage: EstablishmentType::City,
        };
        let mut builds = empty_build_collections();
        builds[0].establishments.push(city);
        state.builds = BoardBuildData::from_build_collections(builds);

        incremental.refresh_after_build(&state, 0, Build::Establishment(city));

        assert_matches_rebuild(&incremental, &state);
    }

    #[test]
    fn incremental_roadbuild_refresh_matches_full_rebuild() {
        let mut state = SetupGameState::default().finish();
        let mut incremental = GameIndex::rebuild(&state);
        let roads = [path(h(0, 0), h(1, 0)), path(h(1, 0), h(1, -1))];
        let mut builds = empty_build_collections();
        builds[0].roads = roads.into_iter().map(|pos| Road { path: pos }).collect();
        state.builds = BoardBuildData::from_build_collections(builds);

        incremental.refresh_after_roadbuild(&state, 0, roads);

        assert_matches_rebuild(&incremental, &state);
    }

    #[test]
    fn roadbuild_refresh_uses_build_state_owner_seed_like_full_rebuild() {
        let mut state = SetupGameState::default().finish();
        let roads = [
            path(h(0, 0), h(1, 0)),
            path(h(0, 0), h(1, -1)),
            path(h(0, 0), h(0, -1)),
            path(h(0, 0), h(-1, 0)),
            path(h(0, 0), h(-1, 1)),
        ];
        let mut builds = empty_build_collections();
        builds[0].roads = roads.into_iter().map(|pos| Road { path: pos }).collect();
        state.builds = BoardBuildData::from_build_collections(builds);

        let mut incremental = GameIndex::rebuild(&state);
        incremental.longest_road_owner = None;
        incremental.refresh_after_roadbuild(&state, 0, [roads[0], roads[1]]);

        assert_matches_rebuild(&incremental, &state);
    }

    #[test]
    fn knight_refresh_updates_largest_army_only() {
        let mut state = SetupGameState::default().finish();
        let mut incremental = GameIndex::rebuild(&state);
        let before_roads = incremental.longest_road_lengths.clone();
        let before_ports = incremental.ports_acquired.clone();

        for _ in 0..3 {
            state
                .players
                .get_mut(0)
                .dev_cards_add(DevCardKind::Usable(UsableDevCard::Knight));
        }
        state.players.get_mut(0).dev_cards_reset_queue();
        for _ in 0..3 {
            state
                .players
                .get_mut(0)
                .dev_cards_move_to_used(UsableDevCard::Knight)
                .unwrap();
        }

        incremental.refresh_after_dev_card(
            &state,
            0,
            &DevCardUsage::Knight {
                rob_hex: h(0, 0),
                robbed_id: None,
            },
        );

        assert_eq!(incremental.largest_army_owner, Some(P0));
        assert_eq!(incremental.longest_road_lengths, before_roads);
        assert_eq!(incremental.ports_acquired, before_ports);
        assert_matches_rebuild(&incremental, &state);
    }

    #[test]
    fn dev_card_refresh_resyncs_longest_road_owner_like_full_rebuild() {
        let mut state = SetupGameState::default().finish();
        let player_zero_roads = h(0, 0)
            .neighbors()
            .into_iter()
            .take(5)
            .map(|neighbor| Road {
                path: path(h(0, 0), neighbor),
            })
            .collect::<Vec<_>>();
        let player_three_roads = h(2, 0)
            .neighbors()
            .into_iter()
            .take(5)
            .map(|neighbor| Road {
                path: path(h(2, 0), neighbor),
            })
            .collect::<Vec<_>>();
        let mut builds = empty_build_collections();
        builds[0].roads = player_zero_roads;
        builds[3].roads = player_three_roads;
        state.builds = BoardBuildData::from_build_collections(builds);

        let mut incremental = GameIndex::rebuild(&state);
        incremental.longest_road_owner = Some(P0);

        incremental.refresh_after_dev_card(
            &state,
            1,
            &DevCardUsage::Knight {
                rob_hex: h(0, 0),
                robbed_id: Some(P0),
            },
        );

        assert_matches_rebuild(&incremental, &state);
        assert_eq!(incremental.longest_road_owner, None);
    }

    #[test]
    fn resource_dev_card_refreshes_leave_index_unchanged() {
        let state = SetupGameState::default().finish();
        let mut plenty = GameIndex::rebuild(&state);
        let mut monopoly = plenty.clone();

        plenty.refresh_after_dev_card(
            &state,
            0,
            &DevCardUsage::YearOfPlenty([Resource::Brick, Resource::Wood]),
        );
        monopoly.refresh_after_dev_card(&state, 0, &DevCardUsage::Monopoly(Resource::Ore));

        assert_eq!(plenty, GameIndex::rebuild(&state));
        assert_eq!(monopoly, GameIndex::rebuild(&state));
    }
}
