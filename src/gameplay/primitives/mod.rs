pub mod bank;
pub mod build;
pub mod dev_card;
pub mod player;
pub mod resource;
pub mod trade;
pub mod turn;

use self::resource::Resource;
use crate::math::dice::DiceVal;

#[derive(Debug, Clone, Copy)]
pub enum PortKind {
    Special(Resource),
    Universal,
}

#[derive(Debug, Clone, Copy)]
pub enum HexResource {
    Some(Resource),
    River,
    Desert,
}

#[derive(Debug, Clone, Copy)]
pub struct HexInfo {
    pub hex_resource: HexResource,
    pub number: DiceVal,
}
