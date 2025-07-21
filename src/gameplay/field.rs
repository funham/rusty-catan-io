use std::collections::BTreeMap;

use crate::gameplay::{hex::*, player::*, resource::*};
use crate::topology::*;

type HexArrangement = BTreeMap<Hex, HexInfo>;

pub struct Field {
    hexes: HexArrangement, // (q, r) |-> HexInfo
    players: Box<[PlayerData]>,
    player_count: usize,
    field_radius: usize,
}

pub struct FieldBuildParam {
    field_radius: usize,
    hex_arrangement: HexArrangement,
}

pub enum FieldBuildError {
    WrongAmountOfHexesProvided,
}

impl FieldBuildParam {
    pub fn try_new(
        field_radius: usize,
        hex_arrangement: HexArrangement,
    ) -> Result<Self, FieldBuildError> {
        if hex_arrangement.len() != Field::field_size_by_radius(field_radius) {
            return Err(FieldBuildError::WrongAmountOfHexesProvided);
        }

        Ok(Self {
            field_radius,
            hex_arrangement,
        })
    }
}

impl Field {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1)
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let players = Self::make_players();
        let player_count = players.len();
        let hexes = param.hex_arrangement;
        let field_radius = param.field_radius;

        Self {
            hexes,
            players,
            player_count,
            field_radius,
        }
    }

    fn make_players() -> Box<[PlayerData]> {
        todo!()
    }
}
