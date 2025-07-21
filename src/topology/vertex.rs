use itertools::Itertools;
use std::collections::BTreeSet;

use crate::topology::hex::*;
use crate::topology::edge::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vertex(Hex, Hex, Hex);

#[derive(Debug)]
pub enum VertexConstructError {
    NotAdjacentHexes,
}

impl TryFrom<(Hex, Hex, Hex)> for Vertex {
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

impl Vertex {
    pub fn set(&self) -> BTreeSet<Hex> {
        BTreeSet::from([self.0, self.1, self.2])
    }

    pub fn edges(&self) -> impl Iterator<Item = Edge> {
        [self.0, self.1, self.2]
            .into_iter()
            .permutations(2)
            .map(|p| Edge::try_from((p[0], p[1])).unwrap())
    }

    pub fn neighbors(&self) -> impl Iterator<Item = Vertex> {
        self.edges().map(|e| {
            let vertices = e
                .dual()
                .set()
                .difference(&self.set().union(&e.set()).cloned().collect())
                .sorted()
                .cloned()
                .collect::<Vec<_>>();

            assert!(vertices.len() == 3);

            Vertex::try_from((vertices[0], vertices[1], vertices[2])).unwrap()
        })
    }
}
