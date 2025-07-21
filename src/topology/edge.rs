use std::collections::BTreeSet;

use crate::topology::hex::*;
use crate::topology::vertex::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge(Hex, Hex);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeDual(Hex, Hex);

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
