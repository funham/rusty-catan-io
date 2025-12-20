use std::collections::BTreeMap;

use crate::{
    gameplay::{
        player::PlayerId,
        primitives::{HexInfo, PortType},
    },
    math::dice::DiceVal,
    topology::{Hex, Path},
};

pub mod init;
pub mod state;

type HexArrangement = BTreeMap<Hex, HexInfo>;
type PortArrangement = BTreeMap<Path, PortType>;
type HexesByNum = BTreeMap<DiceVal, Vec<Hex>>;
type PortsByPlayer = BTreeMap<PlayerId, Vec<PortType>>;
