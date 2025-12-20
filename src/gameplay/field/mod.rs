use std::collections::BTreeMap;

use crate::{
    gameplay::primitives::{HexInfo, PortKind, player::PlayerId},
    math::dice::DiceVal,
    topology::{Hex, Path},
};

pub mod init;
pub mod state;

type HexArrangement = BTreeMap<Hex, HexInfo>;
type PortArrangement = BTreeMap<Path, PortKind>;
type HexesByNum = BTreeMap<DiceVal, Vec<Hex>>;
type PortsByPlayer = BTreeMap<PlayerId, Vec<PortKind>>;
