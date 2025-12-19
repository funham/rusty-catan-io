use std::collections::BTreeSet;

use crate::topology::hex::*;
use crate::topology::intersection::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Path(Hex, Hex);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeDual(Hex, Hex);

#[derive(Debug)]
pub enum EdgeConstructError {
    NotAdjacentHexes,
    NotNeighboringVertices,
}

impl TryFrom<(Hex, Hex)> for Path {
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

impl TryFrom<(Intersection, Intersection)> for Path {
    type Error = EdgeConstructError;

    fn try_from(value: (Intersection, Intersection)) -> Result<Self, Self::Error> {
        let inter = value
            .0
            .as_set()
            .intersection(&value.1.as_set())
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

impl TryFrom<(Intersection, Intersection)> for EdgeDual {
    type Error = EdgeDualConstructError;

    fn try_from(value: (Intersection, Intersection)) -> Result<Self, Self::Error> {
        let inter = value
            .0
            .as_set()
            .symmetric_difference(&value.1.as_set())
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
    pub fn canon(&self) -> Path {
        let n0 = self.0.neighbors().collect::<BTreeSet<_>>();
        let n1 = self.1.neighbors().collect();

        let inter = n0.intersection(&n1).cloned().collect::<BTreeSet<Hex>>();

        Path::try_from((
            inter.first().unwrap().clone(),
            inter.last().unwrap().clone(),
        ))
        .unwrap()
    }
}

impl Path {
    pub fn as_set(&self) -> BTreeSet<Hex> {
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

    pub fn intersections(&self) -> (Intersection, Intersection) {
        let n0 = self.0.neighbors().collect::<BTreeSet<_>>();
        let n1 = self.1.neighbors().collect::<BTreeSet<_>>();
        let dual = self.dual();

        let [h1, h2] = <[&Hex; 2]>::try_from(n0.intersection(&n1).collect::<Vec<_>>()).unwrap();

        let h1 = h1.clone();
        let h2 = h2.clone();

        (
            Intersection::try_from((dual.0, h1, h2)).unwrap(),
            Intersection::try_from((dual.1, h1, h2)).unwrap(),
        )
    }

    pub fn intersections_iter(&self) -> impl Iterator<Item = Intersection> {
        let vs = self.intersections();
        [vs.0, vs.1].into_iter()
    }

    /// Err if `v` is not a part of a path
    pub fn opposite(&self, v: Intersection) -> Result<Intersection, ()> {
        match self.intersections() {
            (v1, v2) if v1 == v => Ok(v2),
            (v1, v2) if v2 == v => Ok(v1),
            _ => Err(()),
        }
    }
    
    pub fn opposite_or_panic(&self, v: Intersection) -> Intersection {
        self.opposite(v).expect("too cocky")
    }
}
