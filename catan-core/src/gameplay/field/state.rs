use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{BoardArrangement, index::FieldIndex};
use crate::common::SmallSet;
use crate::gameplay::primitives::{
    PortKind,
    build::{Establishment, Road},
};
use crate::math::dice::TileNum;
use crate::topology::*;

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
        let index = FieldIndex::new(&raw.arrangement);

        Ok(Self {
            n_players: raw.n_players,
            arrangement: raw.arrangement,
            index,
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
        let n_players = 4;
        let arrangement = super::ser::standard_4p_arrangement();

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
    index: FieldIndex,
}

impl BoardLayout {
    pub const fn field_size_by_radius(radius: usize) -> usize {
        HexIndex::spiral_start_of_ring(radius + 1)
    }

    pub fn new(param: FieldBuildParam) -> Self {
        let index = FieldIndex::new(&param.arrangement);

        Self {
            n_players: param.n_players,
            arrangement: param.arrangement,
            index,
        }
    }

    pub fn desert_pos(&self) -> Hex {
        self.index.desert_pos
    }

    pub fn hexes_by_num(&self, num: TileNum) -> &SmallSet<Hex, 4> {
        &self.index.hex_by_num[num]
    }

    pub fn index(&self) -> &FieldIndex {
        &self.index
    }

    pub fn ports_intersection(&self) -> &BTreeMap<Intersection, PortKind> {
        &self.index.ports_intersection
    }

    pub fn intersections(&self) -> &[Intersection] {
        self.index.intersections()
    }

    pub fn paths(&self) -> &[Path] {
        self.index.paths()
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

    #[test]
    fn board_layout_field_size_uses_hex_spiral_index() {
        for radius in 0..=5 {
            assert_eq!(
                BoardLayout::field_size_by_radius(radius),
                HexIndex::spiral_start_of_ring(radius + 1)
            );
        }
    }
}
