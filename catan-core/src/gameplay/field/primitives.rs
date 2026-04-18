use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Index, IndexMut},
};

use crate::{
    gameplay::primitives::{PortKind, Tile},
    math::dice::DiceVal,
    topology::{Hex, HexIndex, Intersection, Path},
};

pub type PortsByPlayer = Vec<BTreeSet<PortKind>>;

#[derive(Debug, Clone)]
pub struct FieldArrangement {
    pub field_radius: u8,
    tiles: Vec<Tile>,
    ports_info: BTreeMap<Path, PortKind>,
}

#[derive(Debug)]
pub enum HexArrangementError {
    InconsistentSizes(String),
}

impl FieldArrangement {
    pub fn new(
        field_radius: u8,
        tiles: Vec<Tile>,
        ports_info: BTreeMap<Path, PortKind>,
    ) -> Result<Self, HexArrangementError> {
        if HexIndex::spiral_start_of_ring(field_radius as usize + 1) != tiles.len() {
            return Err(HexArrangementError::InconsistentSizes("".to_string()));
        }

        Ok(Self {
            field_radius,
            tiles,
            ports_info,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = Tile> {
        self.tiles.iter().copied()
    }

    pub fn enum_iter(&self) -> impl Iterator<Item = (usize, Tile)> {
        self.tiles.iter().copied().enumerate()
    }

    pub fn hex_iter(&self) -> impl Iterator<Item = Hex> {
        (0..self.tiles.len()).map(|index| HexIndex::spiral_to_hex(index))
    }

    pub fn hex_enum_iter(&self) -> impl Iterator<Item = (Hex, Tile)> {
        self.hex_iter().zip(self.tiles.iter().copied())
    }

    pub fn intersections(&self) -> impl IntoIterator<Item = Intersection> {
        self.hex_enum_iter()
            .flat_map(|(hex, _)| hex.vertices_arr())
            .collect::<BTreeSet<_>>()
    }

    pub fn ports(&self) -> &BTreeMap<Path, PortKind> {
        &self.ports_info
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }
}

impl Index<usize> for FieldArrangement {
    type Output = Tile;

    fn index(&self, index: usize) -> &Self::Output {
        &self.tiles[index]
    }
}

impl Index<Hex> for FieldArrangement {
    type Output = Tile;

    fn index(&self, index: Hex) -> &Self::Output {
        &self[index.index().to_spiral()]
    }
}

#[derive(Debug, Clone, Default)]
pub struct HexesByNum {
    arr: [BTreeSet<Hex>; 11],
}

impl Index<DiceVal> for HexesByNum {
    type Output = BTreeSet<Hex>;

    fn index(&self, index: DiceVal) -> &Self::Output {
        let num: u8 = index.into();
        let min: u8 = DiceVal::min().into();
        let index = num - min;
        &self.arr[index as usize]
    }
}

impl IndexMut<DiceVal> for HexesByNum {
    fn index_mut(&mut self, index: DiceVal) -> &mut Self::Output {
        let num: u8 = index.into();
        let min: u8 = DiceVal::min().into();
        let index = num - min;
        &mut self.arr[index as usize]
    }
}
