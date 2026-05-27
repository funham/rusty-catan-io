use std::collections::BTreeMap;

use crate::{
    common::SmallSet,
    gameplay::{
        field::{BoardArrangement, HexesByNum},
        primitives::{PortKind, Tile, build::PathSet},
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
    path_set: PathSet,
    incident_paths: Vec<(Intersection, SmallSet<Path, 3>)>,
}

impl FieldIndex {
    pub(crate) fn new(board: &BoardArrangement) -> Self {
        let desert_pos = Self::find_desert_pos(board);
        let hex_by_num = Self::get_hex_by_num(board);
        let intersections: Vec<Intersection> = board.intersections().into_iter().collect();
        let path_set: PathSet = board.path_set().into_iter().collect();
        let paths: Vec<Path> = path_set.iter().copied().collect();
        let incident_paths = Self::build_incident_paths(&intersections, &path_set);
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
            path_set,
            incident_paths,
        }
    }

    pub fn intersections(&self) -> &[Intersection] {
        &self.intersections
    }

    pub fn paths(&self) -> &[Path] {
        &self.paths
    }

    pub fn path_set(&self) -> &PathSet {
        &self.path_set
    }

    pub fn incident_paths(&self, intersection: Intersection) -> SmallSet<Path, 3> {
        self.incident_paths
            .binary_search_by_key(&intersection, |(pos, _)| *pos)
            .map(|index| self.incident_paths[index].1.clone())
            .unwrap_or_default()
    }

    fn build_incident_paths(
        intersections: &[Intersection],
        board_paths: &PathSet,
    ) -> Vec<(Intersection, SmallSet<Path, 3>)> {
        let mut result = Vec::with_capacity(intersections.len());
        for &intersection in intersections {
            let paths = intersection
                .paths_arr()
                .into_iter()
                .filter(|path| board_paths.contains(path))
                .collect::<SmallSet<Path, 3>>();
            result.push((intersection, paths));
        }
        result.sort_unstable_by_key(|(intersection, _)| *intersection);
        result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::field::ser::standard_4p_arrangement;

    #[test]
    fn path_set_matches_paths_slice() {
        let index = FieldIndex::new(&standard_4p_arrangement());
        let from_slice = index.paths().iter().copied().collect::<PathSet>();

        assert_eq!(index.path_set(), &from_slice);
    }

    #[test]
    fn incident_paths_are_valid_board_paths_touching_intersection() {
        let index = FieldIndex::new(&standard_4p_arrangement());

        for &intersection in index.intersections() {
            let incident = index.incident_paths(intersection);
            assert!(incident.len() <= 3);
            for path in incident {
                assert!(index.path_set().contains(&path));
                assert!(path.intersections().contains(&intersection));
            }
        }
    }
}
