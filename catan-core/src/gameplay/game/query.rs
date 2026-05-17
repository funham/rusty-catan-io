use crate::{
    algorithm, constants,
    gameplay::{
        game::{index::GameIndex, state::GameState},
        primitives::{build::EstablishmentType, player::PlayerId},
    },
    topology::Hex,
};

#[derive(Debug, Clone, Copy)]
pub struct GameQuery<'a> {
    state: &'a GameState,
    index: &'a GameIndex,
}

impl<'a> GameQuery<'a> {
    pub fn new(state: &'a GameState, index: &'a GameIndex) -> Self {
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

    pub fn check_win_condition(&self) -> Option<PlayerId> {
        for player_id in self.player_ids_starting_from(PlayerId::new(0)) {
            let vp_sum = {
                let has_longest_road = self.longest_road_owner() == Some(player_id);
                let has_largest_army = self.largest_army_owner() == Some(player_id);

                let build_vp = self.count_build_vp(player_id);
                let dev_card_vp = self.count_dev_card_vp(player_id);
                let road_vp = has_longest_road
                    .then_some(constants::LONGEST_ROAD_VP)
                    .unwrap_or(0);
                let army_vp = has_largest_army
                    .then_some(constants::LARGEST_ARMY_VP)
                    .unwrap_or(0);

                build_vp + dev_card_vp + road_vp + army_vp
            };

            if vp_sum >= constants::VP_TO_WIN {
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
                EstablishmentType::Settlement => constants::SETTLEMENT_VP,
                EstablishmentType::City => constants::CITY_VP,
            })
            .sum::<u16>()
    }
}
