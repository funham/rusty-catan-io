use crate::hex_coord;
use std::collections::BTreeSet;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

use crate::hex_coord::*;
use crate::resource::*;

pub type HexId = usize;
pub type VertexId = usize;
pub type EdgeId = usize;

pub struct Hex {
    pub res: Option<Resource>, // None for the Desert
    pub num: u8, // assigned dice number
}

pub struct Vertex {
    pub hexes: BTreeSet<HexCoord>,
    pub nb: BTreeSet<VertexId>,
    pub port: Option<Port>,
}

pub struct Edge {
    pub from: VertexId,
    pub to: VertexId,
}

impl Hex {
    pub fn new(res: &Option<Resource>, num: u8) -> Self {
        Self {
            res: *res,
            num,
        }
    }
}

impl Hash for Edge {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();

        h1.write_usize(self.from);
        h1.write_usize(self.to);

        h2.write_usize(self.to);
        h2.write_usize(self.from);

        h1.finish().min(h2.finish()).hash(state);
    }
}

pub struct EdgeContext {
    pub left: Option<HexCoord>, // None for the ocean
    pub right: Option<HexCoord>,
    pub front: Option<HexCoord>,
    pub back: Option<HexCoord>,
}
