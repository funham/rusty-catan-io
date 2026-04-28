use crate::{
    algorithm,
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
        (start_id..self.state.players.count())
            .chain(0..start_id)
            .collect::<Vec<_>>()
    }

    pub fn is_player_on_hex(&self, player_id: PlayerId, hex: Hex) -> bool {
        algorithm::is_player_on_hex(hex, self.state.builds.by_player(player_id))
    }

    pub fn players_on_hex(&self, hex: Hex) -> Vec<PlayerId> {
        algorithm::players_on_hex(hex, self.state.builds.players().iter())
            .into_iter()
            .collect()
    }

    pub fn count_max_tract_length(&self, player_id: PlayerId) -> u16 {
        self.index.longest_road_lengths[player_id]
    }

    pub fn check_win_condition(&self) -> Option<PlayerId> {
        const VP_TO_WIN: u16 = 10;

        for player_id in self.player_ids_starting_from(0) {
            let build_dev_card_vp = self.count_dev_card_build_vp(player_id);
            let road_vp = if self.longest_road_owner() == Some(player_id) {
                2
            } else {
                0
            };
            let army_vp = if self.largest_army_owner() == Some(player_id) {
                3
            } else {
                0
            };

            if build_dev_card_vp + road_vp + army_vp >= VP_TO_WIN {
                return Some(player_id);
            }
        }

        None
    }

    pub fn count_dev_card_build_vp(&self, player_id: PlayerId) -> u16 {
        let mut score = self.state.players.get(player_id).dev_cards().victory_pts;

        score += self.state.builds[player_id]
            .establishments
            .iter()
            .map(|est| match est.stage {
                EstablishmentType::Settlement => 1,
                EstablishmentType::City => 2,
            })
            .sum::<u16>();

        score
    }
}
