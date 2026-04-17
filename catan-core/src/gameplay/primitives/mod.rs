pub mod bank;
pub mod build;
pub mod dev_card;
pub mod player;
pub mod resource;
pub mod trade;
pub mod turn;

use self::resource::Resource;
use crate::math::dice::DiceVal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortKind {
    Special(Resource),
    Universal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Tile {
    Resource { resource: Resource, number: DiceVal },
    River { number: DiceVal },
    Desert,
}
