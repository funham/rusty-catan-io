use std::{collections::BTreeSet, usize};

use super::{HexArrangement, HexesByNum, PortArrangement, PortsByPlayer};
use crate::gameplay::primitives::{
    HexResource, PortKind,
    build::{City, Road, Settlement},
    player::PlayerId,
};
use crate::math::dice::DiceVal;
use crate::topology::*;

#[derive(Debug)]
struct FieldCache {
    desert_pos: Hex,
    hex_by_num: HexesByNum,
    ports_by_player: PortsByPlayer, // may be moved to PlayerData later
}

#[derive(Debug)]
pub struct FieldState {
    pub n_players: usize,
    pub field_radius: usize,
    pub hexes: HexArrangement,  // (q, r) -> HexInfo
    pub ports: PortArrangement, // e -> PortType
    pub robber_pos: Hex,
    cache_: FieldCache,
}

impl FieldCache {
    fn new(n_players: usize, hexes: &HexArrangement) -> Self {
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

    fn get_hex_by_num(hexes: &HexArrangement) -> HexesByNum {
        let mut hex_by_num = HexesByNum::default();
        for num in DiceVal::list() {
            hex_by_num[num] = hexes
                .iter()
                .filter_map(|(pos, info)| (info.number == num).then_some(*pos))
                .collect();
        }

        assert!(
            hex_by_num[DiceVal::seven()].is_empty(),
            "no hexes should be assigned with 7"
        );

        hex_by_num
    }

    fn find_desert_pos(hexes: &HexArrangement) -> Hex {
        hexes
            .iter()
            .filter_map(|(k, v)| match v.hex_resource {
                HexResource::Desert => Some(k),
                _ => None,
            })
            .next()
            .unwrap()
            .clone()
    }
}

#[derive(Debug)]
pub struct FieldBuildParam {
    pub n_players: usize,
    pub field_radius: usize,
    pub hex_arrangement: HexArrangement,
    pub port_arrangement: PortArrangement,
}

#[derive(Debug)]
pub enum FieldBuildError {
    WrongAmountOfHexesProvided,
}

impl FieldBuildParam {
    pub fn try_new(
        n_players: usize,
        field_radius: usize,
        hex_arrangement: HexArrangement,
        port_arrangement: PortArrangement,
    ) -> Result<Self, FieldBuildError> {
        if hex_arrangement.len() != FieldState::field_size_by_radius(field_radius) {
            return Err(FieldBuildError::WrongAmountOfHexesProvided);
        }

        Ok(Self {
            n_players,
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

impl FieldState {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1)
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let cache = FieldCache::new(param.n_players, &param.hex_arrangement);

        Self {
            n_players: param.n_players,
            hexes: param.hex_arrangement,
            ports: param.port_arrangement,
            field_radius: param.field_radius,
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
