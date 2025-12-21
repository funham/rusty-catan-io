use itertools::Itertools;
use std::collections::BTreeSet;

use crate::topology::hex::*;
use crate::topology::path::*;

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

        let value = match <[Hex; 3] as TryFrom<Vec<Hex>>>::try_from(value) {
            Ok(x) => x,
            Err(_) => return Err(VertexConstructError::NotAdjacentHexes),
        };

        let adjacent = (0..value.len())
            .combinations(2)
            .all(|i| value[i[0]].are_neighbors(&value[i[1]]));

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

impl TryFrom<[Hex; 3]> for Intersection {
    type Error = Vec<Hex>;

    fn try_from(value: [Hex; 3]) -> Result<Self, Self::Error> {
        match Self::try_from((value[0], value[1], value[2])) {
            Ok(x) => Ok(x),
            Err(_) => Err(value.into_iter().collect()),
        }
    }
}

impl Intersection {
    pub fn as_set(&self) -> BTreeSet<Hex> {
        BTreeSet::from([self.0, self.1, self.2])
    }

    /// all edges incidential to the vertex
    pub fn paths(&self) -> [Path; 3] {
        let collected = [self.0, self.1, self.2]
            .into_iter()
            .combinations(2)
            .map(|p| Path::try_from((p[0], p[1])).unwrap())
            .collect::<Vec<_>>();

        <[Path; 3] as TryFrom<Vec<Path>>>::try_from(collected).unwrap()
    }

    pub fn neighbors(&self) -> [Intersection; 3] {
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

                let v = <[Hex; 3] as TryFrom<Vec<Hex>>>::try_from(v)
                    .unwrap()
                    .try_into();

                v.unwrap()
            })
            .collect();
        <[Intersection; 3] as TryFrom<Vec<Intersection>>>::try_from(collected).unwrap()
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
