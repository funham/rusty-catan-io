use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::common::FixedSet;
use crate::topology::hex::*;
use crate::topology::intersection::*;

pub mod repr {
    pub trait Representation: Clone + Copy + std::fmt::Debug + Ord + Eq {}
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Canon;
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Dual;

    impl Representation for Canon {}
    impl Representation for Dual {}
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Path<Repr: repr::Representation = repr::Canon>(FixedSet<Hex, 2>, PhantomData<Repr>);

#[derive(Debug)]
pub enum EdgeConstructError {
    NotAdjacentHexes,
    NotNeighboringVertices,
}

impl TryFrom<(Hex, Hex)> for Path {
    type Error = EdgeConstructError;

    fn try_from(value: (Hex, Hex)) -> Result<Self, Self::Error> {
        let (h1, h2) = value;
        if h1.distance(&h2) == 1 {
            Ok(Self {
                0: FixedSet::try_from([h1, h2]).unwrap(),
                1: PhantomData::default(),
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

        let inter = match <[Hex; 2] as TryFrom<Vec<Hex>>>::try_from(inter) {
            Ok(x) => x,
            Err(_) => return Err(EdgeConstructError::NotNeighboringVertices),
        };

        Ok(Self {
            0: FixedSet::try_from([inter[0], inter[1]]).unwrap(),
            1: PhantomData::default(),
        })
    }
}

#[derive(Debug)]
pub enum EdgeDualConstructError {
    NotAdjacentHexes,
    NotNeighboringVertices,
}

impl TryFrom<(Intersection, Intersection)> for Path<repr::Dual> {
    type Error = EdgeDualConstructError;

    fn try_from(value: (Intersection, Intersection)) -> Result<Self, Self::Error> {
        let inter = value
            .0
            .as_set()
            .symmetric_difference(&value.1.as_set())
            .cloned()
            .collect::<Vec<_>>();

        match inter.as_slice() {
            [a, b] => Ok(Self {
                0: [*a, *b].try_into().unwrap(),
                1: PhantomData::default(),
            }),
            _ => Err(EdgeDualConstructError::NotNeighboringVertices),
        }
    }
}

impl TryFrom<(Hex, Hex)> for Path<repr::Dual> {
    type Error = EdgeDualConstructError;

    fn try_from(value: (Hex, Hex)) -> Result<Self, Self::Error> {
        let (h1, h2) = value;

        let nb1 = h1.neighbors_set();
        let nb2 = h2.neighbors_set();
        let intersection = nb1.intersection(&nb2).copied().collect::<Vec<_>>();

        match intersection.as_slice() {
            [_, _] => Ok(Self(
                FixedSet::try_from([h1, h2]).unwrap(),
                PhantomData::default(),
            )),
            _ => Err(EdgeDualConstructError::NotAdjacentHexes),
        }
    }
}

impl Path<repr::Dual> {
    pub fn as_set(&self) -> BTreeSet<Hex> {
        self.0.into()
    }

    pub fn as_arr(&self) -> [Hex; 2] {
        self.0.into()
    }

    pub fn canon(&self) -> Path {
        let [h1, h2] = self.0.into();
        let n0 = h1.neighbors_set();
        let n1 = h2.neighbors_set();

        let inter = n0.intersection(&n1).cloned().collect::<BTreeSet<Hex>>();

        Path::try_from((
            inter.first().unwrap().clone(),
            inter.last().unwrap().clone(),
        ))
        .unwrap()
    }
}

impl Path<repr::Canon> {
    pub fn as_set(&self) -> BTreeSet<Hex> {
        let (h1, h2) = self.as_pair();
        BTreeSet::from([h1, h2])
    }

    pub fn as_pair(&self) -> (Hex, Hex) {
        let [h1, h2] = self.as_arr();
        (h1, h2)
    }

    pub fn as_arr(&self) -> [Hex; 2] {
        self.0.clone().into()
    }

    pub fn dual(&self) -> Path<repr::Dual> {
        let n0 = self.as_pair().0.neighbors_set();
        let n1 = self.as_pair().1.neighbors_set();

        let inter = n0.intersection(&n1).cloned().collect::<BTreeSet<Hex>>();

        assert_eq!(inter.len(), 2);

        Path::<repr::Dual>::try_from((
            inter.first().unwrap().clone(),
            inter.last().unwrap().clone(),
        ))
        .unwrap()
    }

    pub fn intersections(&self) -> [Intersection; 2] {
        let [d1, d2] = self.dual().as_arr();
        let (h1, h2) = self.as_pair();

        [
            Intersection::try_from((d1, h1, h2)).unwrap(),
            Intersection::try_from((d2, h1, h2)).unwrap(),
        ]
    }

    pub fn intersections_iter(&self) -> impl Iterator<Item = Intersection> {
        self.intersections().into_iter()
    }

    /// Err if `v` is not a part of a path
    pub fn opposite(&self, v: Intersection) -> Result<Intersection, ()> {
        match self.intersections() {
            [v1, v2] if v1 == v => Ok(v2),
            [v1, v2] if v2 == v => Ok(v1),
            _ => Err(()),
        }
    }

    pub fn opposite_or_panic(&self, v: Intersection) -> Intersection {
        self.opposite(v).expect("too cocky")
    }
}

#[cfg(test)]
mod tests {
    use crate::topology::repr::Canon;

    use super::*;

    // Helper to create hexes
    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    #[test]
    fn it_works() {
        Path::<Canon>::try_from((h(0, 1), h(0, 0))).unwrap();
    }

    #[test]
    fn intersections_works() {
        let _ = Path::<Canon>::try_from((h(0, 1), h(0, 0))).unwrap();
    }
}
