use itertools::Itertools;
use std::collections::BTreeSet;

use crate::common::FixedSet;
use crate::topology::hex::*;
use crate::topology::path::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Intersection(FixedSet<Hex, 3>);

#[derive(Debug)]
pub enum VertexConstructError {
    NotAdjacentHexes,
}

impl TryFrom<(Hex, Hex, Hex)> for Intersection {
    type Error = VertexConstructError;

    fn try_from(value: (Hex, Hex, Hex)) -> Result<Self, Self::Error> {
        Self::try_from([value.0, value.1, value.2])
    }
}

impl TryFrom<[Hex; 3]> for Intersection {
    type Error = VertexConstructError;

    fn try_from(value: [Hex; 3]) -> Result<Self, Self::Error> {
        let adjacent = (0..value.len())
            .combinations(2)
            .all(|i| value[i[0]].are_neighbors(&value[i[1]]));

        if adjacent && let Ok(x) = value.try_into() {
            Ok(Self { 0: x })
        } else {
            Err(VertexConstructError::NotAdjacentHexes)
        }
    }
}

impl Intersection {
    pub fn as_set(&self) -> BTreeSet<Hex> {
        self.0.into()
    }

    /// all edges incidential to the vertex
    pub fn paths(&self) -> FixedSet<Path, 3> {
        let collected = self
            .0
            .into_iter()
            .combinations(2)
            .map(|p| Path::try_from((p[0], p[1])).unwrap())
            .collect::<Vec<_>>();

        match collected.as_slice() {
            [a, b, c] => FixedSet::try_from([*a, *b, *c]).unwrap(),
            _ => unreachable!(),
        }
    }

    pub fn neighbors(&self) -> FixedSet<Intersection, 3> {
        let collected = self
            .paths()
            .into_iter()
            .map(|p| {
                let v = p
                    .dual()
                    .set()
                    .difference(&self.as_set())
                    .chain(p.as_arr().each_ref())
                    .copied()
                    .collect::<Vec<_>>();

                let a = <[Hex; 3] as TryFrom<Vec<Hex>>>::try_from(v).unwrap();
                Intersection::try_from(a).unwrap()
            })
            .collect::<Vec<_>>();

        match collected.as_slice() {
            [a, b, c] => [*a, *b, *c].try_into().unwrap(),
            _ => unreachable!(
                "somehow wrong amount of neighbors for a vertex (must always be 3; collected: {:?}",
                collected
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    #[test]
    fn paths_work() {
        let v = Intersection::try_from((h(0, 0), h(1, 0), h(0, 1))).unwrap();
        let paths = v.paths().into_iter().collect::<BTreeSet<_>>();

        assert!(paths.contains(&Path::try_from((h(0, 0), h(1, 0))).unwrap()));
        assert!(paths.contains(&Path::try_from((h(0, 0), h(0, 1))).unwrap()));
        assert!(paths.contains(&Path::try_from((h(0, 1), h(1, 0))).unwrap()));
    }
}
