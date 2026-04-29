use std::{collections::BTreeSet, usize};

use serde::{Deserialize, Serialize};

use super::{BoardArrangement, HexesByNum};
use crate::gameplay::primitives::{
    PortKind, Tile,
    build::{Establishment, Road},
    player::PlayerId,
};
use crate::math::dice::DiceVal;
use crate::topology::*;

// TODO: move to FieldIndex maybe?
#[derive(Debug, Clone)]
struct Cache {
    desert_pos: Hex,
    hex_by_num: HexesByNum,
    ports_by_player: Vec<BTreeSet<PortKind>>, // may be moved to PlayerData later
}

#[derive(Debug, Clone)]
pub struct BoardLayout {
    pub n_players: usize,
    pub arrangement: BoardArrangement,
    cache_: Cache,
}

impl Cache {
    fn new(n_players: usize, hexes: &BoardArrangement) -> Self {
        let desert_pos = Self::find_desert_pos(hexes);
        let hex_by_num = Self::get_hex_by_num(hexes);
        let ports_by_player = Self::get_ports_by_player(n_players);

        Self {
            desert_pos,
            hex_by_num,
            ports_by_player,
        }
    }

    fn get_ports_by_player(n_players: usize) -> Vec<BTreeSet<PortKind>> {
        vec![BTreeSet::default(); n_players]
    }

    fn get_hex_by_num(arrangement: &BoardArrangement) -> HexesByNum {
        let mut hex_by_num = HexesByNum::default();
        for num in DiceVal::list() {
            hex_by_num[num] = arrangement
                .hex_enum_iter()
                .filter_map(|(pos, hex)| {
                    let x = match hex {
                        Tile::Resource {
                            resource: _,
                            number,
                        } => Some(number),
                        Tile::River { number } => Some(number),
                        Tile::Desert => None,
                    };
                    (x? == num).then_some(pos)
                })
                .collect()
        }

        assert!(
            hex_by_num[DiceVal::seven()].is_empty(),
            "no hexes should be assigned with 7"
        );

        hex_by_num
    }

    fn find_desert_pos(hexes: &BoardArrangement) -> Hex {
        hexes
            .hex_enum_iter()
            .filter_map(|(k, v)| match v {
                Tile::Desert => Some(k),
                _ => None,
            })
            .next()
            .unwrap()
            .clone()
    }
}

#[derive(Serialize, Deserialize)]
struct BoardLayoutSerde {
    n_players: usize,
    arrangement: BoardArrangement,
}

impl Serialize for BoardLayout {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        BoardLayoutSerde {
            n_players: self.n_players,
            arrangement: self.arrangement.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BoardLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = BoardLayoutSerde::deserialize(deserializer)?;
        let cache_ = Cache::new(raw.n_players, &raw.arrangement);

        Ok(Self {
            n_players: raw.n_players,
            arrangement: raw.arrangement,
            cache_,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoardState {
    pub robber_pos: Hex,
}

#[derive(Debug)]
pub struct FieldBuildParam {
    pub n_players: usize,
    pub arrangement: BoardArrangement,
}

impl Default for FieldBuildParam {
    fn default() -> Self {
        let default_arrangement_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("default-hex-arrangement.json");
        let n_players = 4;
        let arrangement = super::ser::arrangement_from_json(&default_arrangement_path)
            .expect("default field arrangement should be readable");

        Self {
            n_players,
            arrangement,
        }
    }
}

#[derive(Debug)]
pub enum FieldBuildError {
    WrongAmountOfHexesProvided,
}

impl FieldBuildParam {
    pub fn try_new(
        n_players: usize,
        field_radius: usize,
        hex_arrangement: BoardArrangement,
    ) -> Result<Self, FieldBuildError> {
        if hex_arrangement.len() != BoardLayout::field_size_by_radius(field_radius) {
            return Err(FieldBuildError::WrongAmountOfHexesProvided);
        }

        Ok(Self {
            n_players,
            arrangement: hex_arrangement,
        })
    }
}

pub enum FieldPromotingError {
    NotEnoughBuilds,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildCollection {
    pub establishments: Vec<Establishment>,
    pub roads: Vec<Road>,
}

impl BoardLayout {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1) // TODO: use `HexIndex`` instead
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let cache = Cache::new(param.n_players, &param.arrangement);

        Self {
            n_players: param.n_players,
            arrangement: param.arrangement,
            cache_: cache,
        }
    }

    pub fn desert_pos(&self) -> Hex {
        self.cache_.desert_pos
    }

    pub fn hexes_by_num(&self, num: DiceVal) -> &BTreeSet<Hex> {
        &self.cache_.hex_by_num[num]
    }

    pub fn ports_aquired(&self, player_id: PlayerId) -> &BTreeSet<PortKind> {
        &self.cache_.ports_by_player[player_id]
    }
}

impl BoardState {
    pub fn new(layout: &BoardLayout) -> Self {
        Self {
            robber_pos: layout.desert_pos(),
        }
    }
}
