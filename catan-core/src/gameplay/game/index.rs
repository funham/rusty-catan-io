use crate::gameplay::{
    field::state::BuildCollection, game::state::GameState, primitives::player::PlayerId,
};

#[derive(Debug, Clone)]
pub struct GameIndex {
    pub all_builds: Vec<BuildCollection>,
    pub longest_road_lengths: Vec<u16>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
}

impl GameIndex {
    pub fn rebuild(state: &GameState) -> Self {
        Self {
            all_builds: state.builds.query().all_builds(),
            longest_road_lengths: (0..state.players.count())
                .map(|id| state.builds[id].roads.find_longest_trail_length() as u16)
                .collect(),
            longest_road_owner: state.builds.longest_road(),
            largest_army_owner: state.players.best_army(),
        }
    }
}
