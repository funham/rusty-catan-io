use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Index, IndexMut},
};

use crate::{
    gameplay::primitives::{HexInfo, PortKind},
    math::dice::DiceVal,
    topology::{Hex, Path},
};

pub mod state;

pub type HexArrangement = BTreeMap<Hex, HexInfo>;
pub type PortArrangement = BTreeMap<Path, PortKind>;
pub type PortsByPlayer = Vec<BTreeSet<PortKind>>;

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
