use crate::gameplay::{
    field::state::{FieldBuildParam, FieldState},
    game::state::GameState,
    primitives::{
        bank::Bank,
        build::BoardBuildData,
        player::PlayerDataContainer,
        turn::{BackAndForthCycle, GameTurn},
    },
};

#[derive(Clone)]
pub struct GameInitializationState {
    pub field: FieldState,
    pub turn: GameTurn<BackAndForthCycle>,
    pub bank: Bank,
    pub players: PlayerDataContainer,
    pub builds: BoardBuildData,
}

impl Default for GameInitializationState {
    fn default() -> Self {
        Self::new(FieldBuildParam::default())
    }
}

impl GameInitializationState {
    pub fn new(field_build_param: FieldBuildParam) -> Self {
        let field = FieldState::new(field_build_param);
        Self {
            turn: GameTurn::new(field.n_players as u8),
            players: PlayerDataContainer::new(field.n_players),
            builds: BoardBuildData::new(field.n_players),
            field,
            bank: Default::default(),
        }
    }

    pub fn finish(self) -> GameState {
        GameState {
            field: self.field,
            turn: self.turn.into_regular(),
            bank: self.bank,
            players: self.players,
            builds: self.builds,
        }
    }
}
