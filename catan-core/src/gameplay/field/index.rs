use std::collections::BTreeMap;

use crate::{
    gameplay::{
        field::{BoardArrangement, HexesByNum},
        primitives::{PortKind, Tile},
    },
    math::dice::TileNum,
    topology::{Hex, Intersection, Path},
};

#[derive(Debug, Clone)]
pub struct FieldIndex {
    pub desert_pos: Hex,
    pub hex_by_num: HexesByNum,
    pub ports_intersection: BTreeMap<Intersection, PortKind>,
    intersections: Vec<Intersection>,
    paths: Vec<Path>,
}

impl FieldIndex {
    pub(crate) fn new(board: &BoardArrangement) -> Self {
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

    pub fn intersections(&self) -> &[Intersection] {
        &self.intersections
    }

    pub fn paths(&self) -> &[Path] {
        &self.paths
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
    }
}
