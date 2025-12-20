use super::{HexArrangement, PortArrangement};

use crate::{
    gameplay::field::state::{FieldState, FieldPromotingError},
    topology::{Hex, Path},
};

type GameInitPlayerBuilds = ((Hex, Path), (Hex, Path));

pub struct GameInitField {
    pub field_radius: usize,
    pub hexes: HexArrangement,             // (q, r) -> HexInfo
    pub ports: PortArrangement,            // e -> PortType
    pub builds: Vec<GameInitPlayerBuilds>, // id -> InitialBuids
}

impl GameInitField {
    pub fn new() -> Self {
        todo!()
    }

    pub fn promote() -> Result<FieldState, FieldPromotingError> {
        todo!()
    }
}
