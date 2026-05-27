use crate::{
    algorithm, constants,
    gameplay::{
        game::{index::GameIndex, state::TableState},
        primitives::{build::EstablishmentType, build::PathSet, player::PlayerId},
    },
    topology::{Hex, Intersection},
};

#[derive(Debug, Clone, Copy)]
pub struct GameQuery<'a> {
    state: &'a TableState,
    index: &'a GameIndex,
}

impl<'a> GameQuery<'a> {
    pub fn new(state: &'a TableState, index: &'a GameIndex) -> Self {
        Self { state, index }
    }

    pub fn largest_army_owner(&self) -> Option<PlayerId> {
        self.index.largest_army_owner
    }

    pub fn longest_road_owner(&self) -> Option<PlayerId> {
        self.index.longest_road_owner
    }

    pub fn player_ids_starting_from(&self, start_id: PlayerId) -> Vec<PlayerId> {
        algorithm::player_order_from(start_id, self.state.players.count()).collect::<Vec<_>>()
    }

    pub fn is_player_on_hex(&self, player_id: PlayerId, hex: Hex) -> bool {
        algorithm::is_player_on_hex(hex, self.state.builds.by_player(player_id))
    }

    pub fn players_on_hex(&self, hex: Hex) -> impl Iterator<Item = PlayerId> + use<'_> {
        algorithm::players_on_hex(hex, self.state.builds.players().iter())
    }

    pub fn count_max_tract_length(&self, player_id: PlayerId) -> u16 {
        self.index.longest_road_lengths[player_id.index()]
    }

    pub fn legal_road_candidates(&self, player_id: PlayerId) -> &PathSet {
        &self.index.legal_road_candidates[player_id.index()]
    }

    pub fn occupied_roads(&self) -> &PathSet {
        &self.index.occupied_roads
    }

    pub fn deadzone_intersections(&self) -> &[Intersection] {
        self.index.deadzone_intersections.as_slice()
    }

    pub fn has_establishment_in_deadzone(&self, pos: Intersection) -> bool {
        self.index.deadzone_intersections.contains(&pos)
    }

    pub fn has_longest_road(&self, player_id: PlayerId) -> bool {
        self.longest_road_owner() == Some(player_id)
    }

    pub fn has_largest_army(&self, player_id: PlayerId) -> bool {
        self.largest_army_owner() == Some(player_id)
    }

    pub fn longest_road_vp(&self, player_id: PlayerId) -> u16 {
        if self.has_longest_road(player_id) {
            constants::vp::LONGEST_ROAD_VP
        } else {
            0
        }
    }

    pub fn largest_army_vp(&self, player_id: PlayerId) -> u16 {
        if self.has_largest_army(player_id) {
            constants::vp::LARGEST_ARMY_VP
        } else {
            0
        }
    }

    pub fn award_vp(&self, player_id: PlayerId) -> u16 {
        self.largest_army_vp(player_id) + self.longest_road_vp(player_id)
    }

    pub fn check_win_condition(&self) -> Option<PlayerId> {
        for player_id in self.player_ids_starting_from(PlayerId::new(0)) {
            let vp_sum = {
                let build_vp = self.count_build_vp(player_id);
                let dev_card_vp = self.count_dev_card_vp(player_id);
                let road_vp = self.longest_road_vp(player_id);
                let army_vp = self.largest_army_vp(player_id);

                build_vp + dev_card_vp + road_vp + army_vp
            };

            if vp_sum >= constants::vp::VP_TO_WIN {
                return Some(player_id);
            }
        }

        None
    }

    pub fn count_dev_card_vp(&self, player_id: PlayerId) -> u16 {
        self.state.players.get(player_id).dev_cards().victory_pts
    }

    pub fn count_build_vp(&self, player_id: PlayerId) -> u16 {
        self.state.builds[player_id]
            .establishments
            .iter()
            .map(|est| match est.stage {
                EstablishmentType::Settlement => constants::vp::SETTLEMENT_VP,
                EstablishmentType::City => constants::vp::CITY_VP,
            })
            .sum::<u16>()
    }
}
