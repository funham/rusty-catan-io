use std::collections::{BTreeMap, BTreeSet};

use super::{HexArrangement, HexesByNum, PortArrangement, PortsByPlayer};
use crate::gameplay::player::*;
use crate::gameplay::primitives::{City, HexType, Road, Settlement};
use crate::math::dice::DiceVal;
use crate::topology::*;

#[derive(Debug)]
struct FieldCache {
    desert_pos: Hex,
    hex_by_num: HexesByNum,
    ports_by_player: PortsByPlayer, // may be moved to PlayerData later
}

#[derive(Debug)]
pub struct Field {
    pub field_radius: usize,
    pub hexes: HexArrangement,        // (q, r) -> HexInfo
    pub ports: PortArrangement,       // e -> PortType
    pub builds: Vec<PlayerBuildData>, // id -> BuildData
    pub robber_pos: Hex,
    cache_: FieldCache,
}

impl FieldCache {
    fn new(hexes: &HexArrangement, ports: &PortArrangement) -> Self {
        todo!()
    }
}

pub struct FieldBuildParam {
    field_radius: usize,
    hex_arrangement: HexArrangement,
    port_arrangement: PortArrangement,
}

pub enum FieldBuildError {
    WrongAmountOfHexesProvided,
}

impl FieldBuildParam {
    pub fn try_new(
        field_radius: usize,
        hex_arrangement: HexArrangement,
        port_arrangement: PortArrangement,
    ) -> Result<Self, FieldBuildError> {
        if hex_arrangement.len() != Field::field_size_by_radius(field_radius) {
            return Err(FieldBuildError::WrongAmountOfHexesProvided);
        }

        Ok(Self {
            field_radius,
            hex_arrangement,
            port_arrangement,
        })
    }
}

pub enum FieldPromotingError {
    NotEnoughBuilds,
}

pub struct BuildCollection {
    pub settlements: Vec<Settlement>,
    pub cities: Vec<City>,
    pub roads: Vec<Road>,
}

impl Field {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1)
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let desert_pos = Self::find_desert_pos_static(&param.hex_arrangement);

        let cache = FieldCache::new(&param.hex_arrangement, &param.port_arrangement);

        Self {
            hexes: param.hex_arrangement,
            ports: param.port_arrangement,
            field_radius: param.field_radius,
            builds: Vec::new(),
            robber_pos: desert_pos,
            cache_: cache,
        }
    }

    pub fn get_desert_pos(&self) -> Hex {
        self.cache_.desert_pos
    }

    pub fn hexes_by_num(&self, num: DiceVal) -> BTreeSet<Hex> {
        todo!()
    }

    pub fn builds_on_hex(&self, hex: Hex) -> BTreeMap<PlayerId, BuildCollection> {
        todo!()
    }

    fn find_desert_pos_static(hexes: &HexArrangement) -> Hex {
        hexes
            .iter()
            .filter_map(|(k, v)| match v.hex_type {
                HexType::Desert => Some(k),
                _ => None,
            })
            .next()
            .unwrap()
            .clone()
    }
}
