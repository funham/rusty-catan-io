use super::resource::Resource;
use crate::math::dice::TileNum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortKind {
    Special(Resource),
    Universal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Tile {
    Resource { resource: Resource, number: TileNum },
    River { number: TileNum },
    Desert,
}
