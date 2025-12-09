use std::collections::BTreeMap;

use crate::gameplay::{hex::*, player::*, resource::*};
use crate::topology::*;

type HexArrangement = BTreeMap<Hex, HexInfo>;
type PortArrangement = BTreeMap<Vertex, PortType>;

pub struct Field {
    hexes: HexArrangement,  // (q, r) |-> HexInfo
    ports: PortArrangement, // v |-> PortType
    field_radius: usize,
    player_builds: Vec<PlayerBuildData>,
    robber_pos: Hex,
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

impl Field {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1)
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let robber_pos = Self::find_desert_pos(&param.hex_arrangement);

        Self {
            hexes: param.hex_arrangement,
            ports: param.port_arrangement,
            field_radius: param.field_radius,
            player_builds: Vec::new(),
            robber_pos,
        }
    }

    fn find_desert_pos(hexes: &HexArrangement) -> Hex {
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
