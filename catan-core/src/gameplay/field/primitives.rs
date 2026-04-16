use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Index, IndexMut},
};

use crate::{
    gameplay::primitives::{HexInfo, PortKind},
    math::dice::DiceVal,
    topology::{Hex, Path},
};

pub type PortsByPlayer = Vec<BTreeSet<PortKind>>;

#[derive(Debug, Clone)]
pub struct FieldArrangement {
    pub field_radius: u8,
    hex_info: Vec<HexInfo>,
    ports_info: BTreeMap<Path, PortKind>,
}

#[derive(Debug)]
pub enum HexArrangementError {
    InconsistentSizes(String),
}

impl FieldArrangement {
    pub fn new(
        field_radius: u8,
        hex_info: Vec<HexInfo>,
        ports_info: BTreeMap<Path, PortKind>,
    ) -> Result<Self, HexArrangementError> {
        if Hex::cum_ring_size(field_radius) as usize != hex_info.len() {
            return Err(HexArrangementError::InconsistentSizes("".to_string()));
        }

        Ok(Self {
            field_radius,
            hex_info,
            ports_info,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = HexInfo> {
        self.hex_info.iter().copied()
    }

    pub fn enum_iter(&self) -> impl Iterator<Item = (usize, HexInfo)> {
        self.hex_info.iter().copied().enumerate()
    }

    pub fn hex_enum_iter(&self) -> impl Iterator<Item = (Hex, HexInfo)> {
        (0..)
            .map(|index| Hex::from_spiral(index))
            .zip(self.hex_info.iter().copied())
    }

    pub fn ports(&self) -> &BTreeMap<Path, PortKind> {
        &self.ports_info
    }

    pub fn len(&self) -> usize {
        self.hex_info.len()
    }
}

impl Index<usize> for FieldArrangement {
    type Output = HexInfo;

    fn index(&self, index: usize) -> &Self::Output {
        &self.hex_info[index]
    }
}

impl Index<Hex> for FieldArrangement {
    type Output = HexInfo;

    fn index(&self, index: Hex) -> &Self::Output {
        &self[index.to_spiral()]
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
