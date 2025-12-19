use itertools::Itertools;
use std::collections::BTreeSet;

use crate::topology::path::*;
use crate::topology::hex::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Intersection(Hex, Hex, Hex);

#[derive(Debug)]
pub enum VertexConstructError {
    NotAdjacentHexes,
}

impl TryFrom<(Hex, Hex, Hex)> for Intersection {
    type Error = VertexConstructError;

    fn try_from(value: (Hex, Hex, Hex)) -> Result<Self, Self::Error> {
        let value = [value.0, value.1, value.2]
            .into_iter()
            .sorted()
            .collect::<Vec<_>>();

        assert!(value.len() == 3);

        let adjacent = value[0].are_neighbors(&value[1])
            && value[2].are_neighbors(&value[2])
            && value[1].are_neighbors(&value[2]);

        if adjacent {
            Ok(Self {
                0: value[0],
                1: value[1],
                2: value[2],
            })
        } else {
            Err(VertexConstructError::NotAdjacentHexes)
        }
    }
}

impl Intersection {
    pub fn as_set(&self) -> BTreeSet<Hex> {
        BTreeSet::from([self.0, self.1, self.2])
    }

    /// all edges incidential to the vertex
    pub fn paths(&self) -> impl Iterator<Item = Path> {
        [self.0, self.1, self.2]
            .into_iter()
            .permutations(2)
            .map(|p| Path::try_from((p[0], p[1])).unwrap())
    }

    pub fn neighbors(&self) -> impl Iterator<Item = Intersection> {
        self.paths().map(|e| {
            let vertices = e
                .dual()
                .set()
                .difference(&self.as_set().union(&e.as_set()).cloned().collect())
                .sorted()
                .cloned()
                .collect::<Vec<_>>();

            assert!(vertices.len() == 3);

            Intersection::try_from((vertices[0], vertices[1], vertices[2])).unwrap()
        })
    }
}
