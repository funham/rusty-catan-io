use std::marker::PhantomData;

use serde::Deserialize;
use serde::Serialize;

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

impl Serialize for Path {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_arr().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Path {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hexes = <[Hex; 2]>::deserialize(deserializer)?;
        Self::try_from((hexes[0], hexes[1])).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug)]
pub enum EdgeConstructError {
    NotAdjacentHexes,
    NotNeighboringVertices,
}

impl std::fmt::Display for EdgeConstructError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl TryFrom<(Hex, Hex)> for Path {
    type Error = EdgeConstructError;

    fn try_from(value: (Hex, Hex)) -> Result<Self, Self::Error> {
        let (h1, h2) = value;
        if h1.distance(&h2) == 1 {
            Ok(Self::from_adjacent_hexes(h1, h2))
        } else {
            Err(EdgeConstructError::NotAdjacentHexes)
        }
    }
}

impl TryFrom<(Intersection, Intersection)> for Path {
    type Error = EdgeConstructError;

    fn try_from(value: (Intersection, Intersection)) -> Result<Self, Self::Error> {
        let Some(inter) = common_hexes_3(value.0.as_arr(), value.1.as_arr()) else {
            return Err(EdgeConstructError::NotNeighboringVertices);
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
        match symmetric_difference_hexes_3(value.0.as_arr(), value.1.as_arr()) {
            Some([a, b]) => Ok(Self {
                0: [a, b].try_into().unwrap(),
                1: PhantomData::default(),
            }),
            None => Err(EdgeDualConstructError::NotNeighboringVertices),
        }
    }
}

impl TryFrom<(Hex, Hex)> for Path<repr::Dual> {
    type Error = EdgeDualConstructError;

    fn try_from(value: (Hex, Hex)) -> Result<Self, Self::Error> {
        let (h1, h2) = value;

        match common_neighbors(h1, h2) {
            Some(_) => Ok(Self(
                FixedSet::try_from([h1, h2]).unwrap(),
                PhantomData::default(),
            )),
            _ => Err(EdgeDualConstructError::NotAdjacentHexes),
        }
    }
}

impl Path<repr::Dual> {
    pub fn as_set(&self) -> FixedSet<Hex, 2> {
        self.0
    }

    pub fn as_arr(&self) -> [Hex; 2] {
        self.0.into()
    }

    pub fn canon(&self) -> Path {
        let [h1, h2] = self.0.into();
        let [c1, c2] = common_neighbors(h1, h2).unwrap();

        Path::from_adjacent_hexes(c1, c2)
    }
}

impl Path<repr::Canon> {
    pub(crate) fn from_adjacent_hexes(h1: Hex, h2: Hex) -> Self {
        debug_assert_eq!(h1.distance(&h2), 1);
        Self {
            0: FixedSet::try_from([h1, h2]).expect("adjacent path hexes should be unique"),
            1: PhantomData::default(),
        }
    }

    pub fn as_set(&self) -> FixedSet<Hex, 2> {
        self.0
    }

    pub fn as_pair(&self) -> (Hex, Hex) {
        let [h1, h2] = self.as_arr();
        (h1, h2)
    }

    pub fn as_arr(&self) -> [Hex; 2] {
        self.0.clone().into()
    }

    pub fn axis(&self) -> Axis {
        Axis::from_path(*self)
    }

    pub fn dual(&self) -> Path<repr::Dual> {
        let (h1, h2) = self.as_pair();
        let [d1, d2] = common_neighbors(h1, h2).unwrap();

        Path::<repr::Dual>(
            FixedSet::try_from([d1, d2]).expect("path dual hexes should be unique"),
            PhantomData::default(),
        )
    }

    pub fn intersections(&self) -> [Intersection; 2] {
        let [d1, d2] = self.dual().as_arr();
        let (h1, h2) = self.as_pair();

        [
            Intersection::from_adjacent_hexes([d1, h1, h2]),
            Intersection::from_adjacent_hexes([d2, h1, h2]),
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

fn common_neighbors(h1: Hex, h2: Hex) -> Option<[Hex; 2]> {
    let mut common = [Hex::new(0, 0); 2];
    let mut len = 0;

    for candidate in h1.neighbors() {
        if candidate.are_neighbors(&h2) {
            if len == common.len() {
                return None;
            }
            common[len] = candidate;
            len += 1;
        }
    }

    (len == common.len()).then_some(common)
}

fn common_hexes_3(a: [Hex; 3], b: [Hex; 3]) -> Option<[Hex; 2]> {
    let mut common = [Hex::new(0, 0); 2];
    let mut len = 0;

    for candidate in a {
        if b.contains(&candidate) {
            if len == common.len() {
                return None;
            }
            common[len] = candidate;
            len += 1;
        }
    }

    (len == common.len()).then_some(common)
}

fn symmetric_difference_hexes_3(a: [Hex; 3], b: [Hex; 3]) -> Option<[Hex; 2]> {
    let mut diff = [Hex::new(0, 0); 2];
    let mut len = 0;

    for candidate in a {
        if !b.contains(&candidate) {
            if len == diff.len() {
                return None;
            }
            diff[len] = candidate;
            len += 1;
        }
    }
    for candidate in b {
        if !a.contains(&candidate) {
            if len == diff.len() {
                return None;
            }
            diff[len] = candidate;
            len += 1;
        }
    }

    (len == diff.len()).then_some(diff)
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
