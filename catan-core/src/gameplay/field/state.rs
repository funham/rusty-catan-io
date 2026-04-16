use std::{collections::BTreeSet, usize};

use serde::{Deserialize, Serialize};

use super::{FieldArrangement, HexesByNum, PortsByPlayer};
use crate::gameplay::primitives::{
    HexInfo, PortKind,
    build::{City, Road, Settlement},
    player::PlayerId,
};
use crate::math::dice::DiceVal;
use crate::topology::*;

#[derive(Debug, Clone)]
struct FieldCache {
    desert_pos: Hex,
    hex_by_num: HexesByNum,
    ports_by_player: PortsByPlayer, // may be moved to PlayerData later
}

#[derive(Debug, Clone)]
pub struct FieldState {
    pub n_players: usize,
    pub arrangement: FieldArrangement,
    pub robber_pos: Hex,
    cache_: FieldCache,
}

impl FieldCache {
    fn new(n_players: usize, hexes: &FieldArrangement) -> Self {
        let desert_pos = Self::find_desert_pos(hexes);
        let hex_by_num = Self::get_hex_by_num(hexes);
        let ports_by_player = Self::get_ports_by_player(n_players);

        Self {
            desert_pos,
            hex_by_num,
            ports_by_player,
        }
    }

    fn get_ports_by_player(n_players: usize) -> PortsByPlayer {
        vec![BTreeSet::default(); n_players]
    }

    fn get_hex_by_num(arrangement: &FieldArrangement) -> HexesByNum {
        let mut hex_by_num = HexesByNum::default();
        for num in DiceVal::list() {
            hex_by_num[num] = arrangement
                .hex_enum_iter()
                .filter_map(|(pos, hex)| {
                    let x = match hex {
                        HexInfo::Resource {
                            resource: _,
                            number,
                        } => Some(number),
                        HexInfo::River { number } => Some(number),
                        HexInfo::Desert => None,
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

    fn find_desert_pos(hexes: &FieldArrangement) -> Hex {
        hexes
            .hex_enum_iter()
            .filter_map(|(k, v)| match v {
                HexInfo::Desert => Some(k),
                _ => None,
            })
            .next()
            .unwrap()
            .clone()
    }
}

#[derive(Serialize, Deserialize)]
struct FieldStateSerde {
    n_players: usize,
    arrangement: FieldArrangement,
    robber_pos: Hex,
}

impl Serialize for FieldState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        FieldStateSerde {
            n_players: self.n_players,
            arrangement: self.arrangement.clone(),
            robber_pos: self.robber_pos,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FieldState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = FieldStateSerde::deserialize(deserializer)?;
        let cache_ = FieldCache::new(raw.n_players, &raw.arrangement);

        Ok(Self {
            n_players: raw.n_players,
            arrangement: raw.arrangement,
            robber_pos: raw.robber_pos,
            cache_,
        })
    }
}

#[derive(Debug)]
pub struct FieldBuildParam {
    pub n_players: usize,
    pub arrangement: FieldArrangement,
}

impl Default for FieldBuildParam {
    fn default() -> Self {
        let default_arrangement_path =
            std::path::Path::new("catan-core/data/default-hex-arrangement.json");
        let n_players = 4;
        let arrangement = super::ser::arrangement_from_json(default_arrangement_path)
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
        hex_arrangement: FieldArrangement,
    ) -> Result<Self, FieldBuildError> {
        if hex_arrangement.len() != FieldState::field_size_by_radius(field_radius) {
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

pub struct BuildCollection {
    pub settlements: Vec<Settlement>,
    pub cities: Vec<City>,
    pub roads: Vec<Road>,
}

impl FieldState {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1)
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let cache = FieldCache::new(param.n_players, &param.arrangement);

        Self {
            n_players: param.n_players,
            arrangement: param.arrangement,
            robber_pos: cache.desert_pos,
            cache_: cache,
        }
    }

    pub fn get_desert_pos(&self) -> Hex {
        self.cache_.desert_pos
    }

    pub fn hexes_by_num(&self, num: DiceVal) -> &BTreeSet<Hex> {
        &self.cache_.hex_by_num[num]
    }

    pub fn ports_aquired(&self, player_id: PlayerId) -> &BTreeSet<PortKind> {
        &self.cache_.ports_by_player[player_id]
    }
}
