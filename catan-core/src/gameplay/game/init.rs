use crate::gameplay::{
    field::{
        HexArrangement,
        state::{FieldBuildParam, FieldState},
    },
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
}
