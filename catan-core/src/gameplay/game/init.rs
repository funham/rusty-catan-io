use crate::gameplay::{
    field::state::{FieldBuildParam, FieldState},
    game::state::GameState,
    primitives::{
        bank::Bank,
        build::BuildDataContainer,
        player::PlayerDataContainer,
        turn::{BackAndForthCycle, GameTurn},
    },
};

pub struct GameInitializationState {
    pub field: FieldState,
    pub turn: GameTurn<BackAndForthCycle>,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BuildDataContainer,
}

impl Default for GameInitializationState {
    fn default() -> Self {
        Self {
            field: todo!(),
            turn: todo!(),
            bank: Default::default(),
            players: Default::default(),
            builds: Default::default(),
        }
    }
}

impl GameInitializationState {
    pub fn new(field_build_param: FieldBuildParam) -> Self {
        Self {
            turn: GameTurn::new(field_build_param.n_players as u8),
            field: FieldState::new(field_build_param),
            bank: Bank::default(),
            players: PlayerDataContainer::default(),
            builds: BuildDataContainer::default(),
        }
    }

    pub fn promote(self) -> GameState {
        todo!()
    }
}
