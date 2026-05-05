use std::collections::BTreeSet;

use crate::{
    algorithm,
    gameplay::{
        field::state::BuildCollection,
        game::state::GameState,
        primitives::{
            PortKind,
            build::{Build, EstablishmentType},
            player::PlayerId,
        },
    },
    topology::{Intersection, Path},
};

#[derive(Debug, Clone)]
pub struct GameIndex {
    pub all_builds: Vec<BuildCollection>,
    pub longest_road_lengths: Vec<u16>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
    pub ports_aquired: Vec<BTreeSet<PortKind>>,
}

impl GameIndex {
    pub fn rebuild(state: &GameState) -> Self {
        let longest_road_lengths = Self::longest_road_lengths(state);
        let longest_road_owner =
            Self::longest_road_owner(state.builds.longest_road(), &longest_road_lengths);

        Self {
            all_builds: state.builds.query().all_builds(),
            longest_road_lengths,
            longest_road_owner,
            largest_army_owner: state.players.best_army(),
            ports_aquired: Self::get_ports_aquired(state),
        }
    }

    fn get_ports_aquired(state: &GameState) -> Vec<BTreeSet<PortKind>> {
        algorithm::get_ports_aquired(state.board.index().ports_intersection, &state.builds)
    }

    pub fn refresh_after_build(&mut self, state: &GameState, player_id: PlayerId, build: Build) {
        self.all_builds = state.builds.query().all_builds();

        match build {
            Build::Road(_) => {
                self.refresh_longest_road_length(state, player_id);
            }
            Build::Establishment(establishment) => {
                if establishment.stage == EstablishmentType::Settlement {
                    self.ports_aquired = Self::get_ports_aquired(state);
                    for opponent in 0..state.players.count() {
                        if opponent != player_id
                            && Self::player_has_road_touching(state, opponent, establishment.pos)
                        {
                            self.refresh_longest_road_length(state, opponent);
                        }
                    }
                }
            }
        }

        self.longest_road_owner =
            Self::longest_road_owner(self.longest_road_owner, &self.longest_road_lengths);
    }

    fn longest_road_lengths(state: &GameState) -> Vec<u16> {
        (0..state.players.count())
            .map(|player_id| {
                let blockers = Self::opponent_establishments(state, player_id);
                state.builds[player_id]
                    .roads
                    .find_longest_trail_length_with_blockers(&blockers) as u16
            })
            .collect()
    }

    fn opponent_establishments(state: &GameState, player_id: PlayerId) -> BTreeSet<Intersection> {
        state
            .builds
            .players_indexed()
            .filter(|(other_id, _)| *other_id != player_id)
            .flat_map(|(_, player)| player.establishments.iter().map(|build| build.pos))
            .collect()
    }

    fn refresh_longest_road_length(&mut self, state: &GameState, player_id: PlayerId) {
        let blockers = Self::opponent_establishments(state, player_id);
        self.longest_road_lengths[player_id] = state.builds[player_id]
            .roads
            .find_longest_trail_length_with_blockers(&blockers)
            as u16;
    }

    fn player_has_road_touching(
        state: &GameState,
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
        path.intersections().contains(&intersection)
    }

    fn longest_road_owner(
        current_owner: Option<PlayerId>,
        longest_road_lengths: &[u16],
    ) -> Option<PlayerId> {
        const MIN_LONGEST_ROAD: u16 = 5;

        if let Some(owner) = current_owner {
            let owner_len = longest_road_lengths[owner];
            if owner_len >= MIN_LONGEST_ROAD
                && longest_road_lengths
                    .iter()
                    .enumerate()
                    .all(|(id, &len)| id == owner || len <= owner_len)
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
            .map(|(id, _)| id);

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
            game::init::GameInitializationState,
            primitives::build::{BoardBuildData, Establishment, EstablishmentType, Road},
        },
        topology::{Hex, Path},
    };

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn path(h1: Hex, h2: Hex) -> Path {
        Path::try_from((h1, h2)).unwrap()
    }

    #[test]
    fn indexed_longest_road_respects_opponent_settlement_blockers() {
        let mut state = GameInitializationState::default().finish();
        let blocker = path(h(0, 0), h(1, 0)).intersections()[0];
        let player_roads = h(0, 0)
            .neighbors()
            .into_iter()
            .map(|neighbor| Road {
                pos: path(h(0, 0), neighbor),
            })
            .collect();

        state.builds = BoardBuildData::from_build_collections(vec![
            BuildCollection {
                establishments: vec![],
                roads: player_roads,
            },
            BuildCollection {
                establishments: vec![Establishment {
                    pos: blocker,
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
        assert_eq!(index.longest_road_owner, Some(0));
    }

    #[test]
    fn incremental_settlement_refresh_matches_full_rebuild_for_longest_road_blocker() {
        let mut state = GameInitializationState::default().finish();
        let blocker = path(h(0, 0), h(1, 0)).intersections()[0];
        let player_roads = h(0, 0)
            .neighbors()
            .into_iter()
            .map(|neighbor| Road {
                pos: path(h(0, 0), neighbor),
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
            pos: blocker,
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
        assert_eq!(incremental.ports_aquired, rebuilt.ports_aquired);
    }
}
