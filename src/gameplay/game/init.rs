use crate::gameplay::{
    field::init::GameInitField,
    turn::{BackAndForthCycle, GameTurn},
};

pub struct GameInitializationState {
    pub field: GameInitField,
    pub turn: GameTurn<BackAndForthCycle>,
}

impl GameInitializationState {}
