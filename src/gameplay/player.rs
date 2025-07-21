use std::collections::BTreeSet;

use crate::gameplay::field::Field;
use crate::gameplay::player_move::{Move, MoveRascals};
use crate::topology::*;

pub struct Build {
    pos: Vertex,
    btype: BuildType,
}

pub enum BuildType {
    SETTLEMENT,
    CITY,
}

pub struct Road {
    pos: Edge,
}

pub type PlayerId = usize;

pub struct PlayerData {
    builds: BTreeSet<Build>,
    roads: BTreeSet<Road>,
}

pub trait GameActor {
    fn make_move(&self, field: &Field) -> Move;
    fn move_rascals(&self, field: &Field) -> MoveRascals;
}
