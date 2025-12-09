use std::collections::BTreeSet;

use crate::gameplay::field::Field;
use crate::gameplay::player_move::{Move, MoveRascals};
use crate::topology::*;

pub struct Build {
    pos: Vertex,
    btype: BuildType,
}

pub enum BuildType {
    Settlement,
    City,
}

pub struct Road {
    pos: Edge,
}

pub type PlayerId = usize;

pub struct PlayerBuildData {
    builds: BTreeSet<Build>,
    roads: BTreeSet<Road>,
}

