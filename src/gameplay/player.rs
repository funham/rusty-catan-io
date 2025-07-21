use std::collections::BTreeSet;

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

pub struct Player {
    builds: BTreeSet<Build>,
    roads: BTreeSet<Road>,
}
