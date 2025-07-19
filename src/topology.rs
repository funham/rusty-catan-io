use itertools::Itertools;

use std::collections::BTreeSet;

use crate::hex::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vertex(Hex, Hex, Hex);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge(Hex, Hex);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeDual(Hex, Hex);

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
#[derive(Debug)]
pub enum EdgeConstructError {
    NotAdjacentHexes,
    NotNeighboringVertices,
}

impl TryFrom<(Hex, Hex)> for Edge {
    type Error = EdgeConstructError;

    fn try_from(value: (Hex, Hex)) -> Result<Self, Self::Error> {
        if value.0.distance(&value.1) == 1 {
            Ok(Self {
                0: value.0.min(value.1),
                1: value.0.max(value.1),
            })
        } else {
            Err(EdgeConstructError::NotAdjacentHexes)
        }
    }
}

impl TryFrom<(Vertex, Vertex)> for Edge {
    type Error = EdgeConstructError;

    fn try_from(value: (Vertex, Vertex)) -> Result<Self, Self::Error> {
        let inter = value
            .0
            .set()
            .intersection(&value.1.set())
            .cloned()
            .collect::<Vec<_>>();

        if inter.len() != 2 {
            return Err(EdgeConstructError::NotNeighboringVertices);
        }

        Ok(Self {
            0: inter.first().unwrap().clone(),
            1: inter.last().unwrap().clone(),
        })
    }
}

#[derive(Debug)]
pub enum EdgeDualConstructError {
    NotAdjacentHexes,
    NotNeighboringVertices,
}

impl TryFrom<(Vertex, Vertex)> for EdgeDual {
    type Error = EdgeDualConstructError;

    fn try_from(value: (Vertex, Vertex)) -> Result<Self, Self::Error> {
        let inter = value
            .0
            .set()
            .symmetric_difference(&value.1.set())
            .cloned()
            .collect::<Vec<_>>();

        if inter.len() != 2 {
            return Err(EdgeDualConstructError::NotNeighboringVertices);
        }

        Ok(Self {
            0: inter.first().unwrap().clone(),
            1: inter.last().unwrap().clone(),
        })
    }
}

impl TryFrom<(Hex, Hex)> for EdgeDual {
    type Error = EdgeDualConstructError;

    fn try_from(value: (Hex, Hex)) -> Result<Self, Self::Error> {
        if value.0.distance(&value.1) == 2 {
            Ok(Self {
                0: value.0.min(value.1),
                1: value.0.max(value.1),
            })
        } else {
            Err(EdgeDualConstructError::NotAdjacentHexes)
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

impl EdgeDual {
    pub fn set(&self) -> BTreeSet<Hex> {
        BTreeSet::from([self.0, self.1])
    }
    pub fn canon(&self) -> Edge {
        let n0 = self.0.neighbors().collect::<BTreeSet<_>>();
        let n1 = self.1.neighbors().collect();

        let inter = n0.intersection(&n1).cloned().collect::<BTreeSet<Hex>>();

        Edge::try_from((
            inter.first().unwrap().clone(),
            inter.last().unwrap().clone(),
        ))
        .unwrap()
    }
}

impl Edge {
    pub fn set(&self) -> BTreeSet<Hex> {
        BTreeSet::from([self.0, self.1])
    }

    pub fn dual(&self) -> EdgeDual {
        let n0 = self.0.neighbors().collect::<BTreeSet<_>>();
        let n1 = self.1.neighbors().collect();

        let inter = n0.intersection(&n1).cloned().collect::<BTreeSet<Hex>>();

        EdgeDual::try_from((
            inter.first().unwrap().clone(),
            inter.last().unwrap().clone(),
        ))
        .unwrap()
    }

    pub fn vertices(&self) -> (Vertex, Vertex) {
        let n0 = self.0.neighbors().collect::<BTreeSet<_>>();
        let n1 = self.1.neighbors().collect::<BTreeSet<_>>();
        let dual = self.dual();

        let [h1, h2] = <[&Hex; 2]>::try_from(n0.intersection(&n1).collect::<Vec<_>>()).unwrap();

        let h1 = h1.clone();
        let h2 = h2.clone();

        (
            Vertex::try_from((dual.0, h1, h2)).unwrap(),
            Vertex::try_from((dual.1, h1, h2)).unwrap(),
        )
    }
}
