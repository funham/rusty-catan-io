use crate::gameplay::{
    field::init::GameInitField,
    primitives::turn::{BackAndForthCycle, GameTurn},
};

pub struct GameInitializationState {
    pub field: GameInitField,
    pub turn: GameTurn<BackAndForthCycle>,
}

impl GameInitializationState {
    pub fn new(field_size: usize) -> Self {
        todo!()
    }
}
