use std::{collections::BTreeMap, usize};

use serde::{Deserialize, Serialize};

use super::{BoardArrangement, HexesByNum};
use crate::common::SmallSet;
use crate::gameplay::primitives::{
    PortKind, Tile,
    build::{Establishment, Road},
};
use crate::math::dice::TileNum;
use crate::topology::*;

// TODO: move to FieldIndex maybe?
#[derive(Debug, Clone)]
pub struct BoardIndex {
    pub desert_pos: Hex,
    pub hex_by_num: HexesByNum,
    pub ports_intersection: BTreeMap<Intersection, PortKind>,
    intersections: Vec<Intersection>,
    paths: Vec<Path>,
}

impl BoardIndex {
    fn new(board: &BoardArrangement) -> Self {
        let desert_pos = Self::find_desert_pos(board);
        let hex_by_num = Self::get_hex_by_num(board);
        let intersections = board.intersections().into_iter().collect();
        let paths = board.path_set().into_iter().collect();
        let ports_intersection = board
            .ports()
            .iter()
            .flat_map(|(pos, port)| {
                pos.intersections()
                    .into_iter()
                    .zip(std::iter::repeat(port).cloned())
            })
            .collect::<BTreeMap<_, _>>();

        Self {
            desert_pos,
            hex_by_num,
            ports_intersection,
            intersections,
            paths,
        }
    }

    fn get_hex_by_num(arrangement: &BoardArrangement) -> HexesByNum {
        let mut hex_by_num = HexesByNum::default();
        for num in TileNum::iter() {
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
        let cache_ = BoardIndex::new(&raw.arrangement);

        Ok(Self {
            n_players: raw.n_players,
            arrangement: raw.arrangement,
            index: cache_,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCollection {
    pub establishments: Vec<Establishment>,
    pub roads: Vec<Road>,
}

#[derive(Debug, Clone)]
pub struct BoardLayout {
    pub n_players: usize,
    pub arrangement: BoardArrangement,
    index: BoardIndex,
}

impl BoardLayout {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        1 + 3 * radius * (radius + 1) // TODO: use `HexIndex`` instead
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let cache = BoardIndex::new(&param.arrangement);

        Self {
            n_players: param.n_players,
            arrangement: param.arrangement,
            index: cache,
        }
    }

    pub fn desert_pos(&self) -> Hex {
        self.index.desert_pos
    }

    pub fn hexes_by_num(&self, num: TileNum) -> &SmallSet<Hex, 4> {
        &self.index.hex_by_num[num]
    }

    pub fn index(&self) -> BoardIndex {
        self.index.clone()
    }

    pub fn ports_intersection(&self) -> &BTreeMap<Intersection, PortKind> {
        &self.index.ports_intersection
    }

    pub fn intersections(&self) -> &[Intersection] {
        &self.index.intersections
    }

    pub fn paths(&self) -> &[Path] {
        &self.index.paths
    }
}

impl BoardState {
    pub fn new(layout: &BoardLayout) -> Self {
        Self {
            robber_pos: layout.desert_pos(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn board_layout_cached_topology_matches_arrangement_generation() {
        let layout = BoardLayout::new(FieldBuildParam::default());

        assert_eq!(
            layout
                .intersections()
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            layout
                .arrangement
                .intersections()
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            layout.paths().iter().copied().collect::<BTreeSet<_>>(),
            layout.arrangement.path_set().into_iter().collect()
        );
    }
}
