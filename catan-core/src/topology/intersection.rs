use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;

use crate::common::FixedSet;
use crate::topology::hex::*;
use crate::topology::path::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Intersection(FixedSet<Hex, 3>);

impl Serialize for Intersection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hexes: [Hex; 3] = self.0.into();
        hexes.serialize(serializer)
    }
}

impl fmt::Debug for Intersection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hs: [Hex; 3] = self.0.into();

        // f.debug_struct("Intersection")
        //     .field("q0", &hs[0].q)
        //     .field("r0", &hs[0].r)
        //     .field("q1", &hs[1].q)
        //     .field("r1", &hs[1].r)
        //     .field("q2", &hs[2].q)
        //     .field("r2", &hs[2].r)
        //     .finish()

        let mut dbg = f.debug_struct("Intersection");

        for i in 0..3 {
            dbg.field(&format!("{}", i), &Into::<(i32, i32)>::into(hs[i]));
        }

        dbg.finish()
    }
}

impl<'de> Deserialize<'de> for Intersection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hexes = <[Hex; 3]>::deserialize(deserializer)?;
        Self::try_from(hexes).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug)]
pub enum VertexConstructError {
    NotAdjacentHexes,
}

impl std::fmt::Display for VertexConstructError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
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
        if Self::are_adjacent_hexes(value) {
            Ok(Self::from_adjacent_hexes(value))
        } else {
            Err(VertexConstructError::NotAdjacentHexes)
        }
    }
}

impl Intersection {
    pub(crate) fn from_adjacent_hexes(value: [Hex; 3]) -> Self {
        debug_assert!(Self::are_adjacent_hexes(value));
        Self {
            0: FixedSet::try_from(value).expect("adjacent intersection hexes should be unique"),
        }
    }

    fn are_adjacent_hexes(value: [Hex; 3]) -> bool {
        value[0].are_neighbors(&value[1])
            && value[0].are_neighbors(&value[2])
            && value[1].are_neighbors(&value[2])
    }

    pub fn as_arr(&self) -> [Hex; 3] {
        self.0.into()
    }

    pub fn as_set(&self) -> BTreeSet<Hex> {
        self.0.into()
    }

    /// all edges incidential to the vertex
    pub fn paths(&self) -> FixedSet<Path, 3> {
        let [a, b, c] = self.as_arr();
        FixedSet::try_from([
            Path::try_from((a, b)).unwrap(),
            Path::try_from((a, c)).unwrap(),
            Path::try_from((b, c)).unwrap(),
        ])
        .unwrap()
    }

    pub fn neighbors(&self) -> FixedSet<Intersection, 3> {
        let [a, b, c] = self.as_arr();
        FixedSet::try_from([
            neighbor_across_path(a, b, c),
            neighbor_across_path(a, c, b),
            neighbor_across_path(b, c, a),
        ])
        .unwrap()
    }
}

fn neighbor_across_path(path_a: Hex, path_b: Hex, current_third: Hex) -> Intersection {
    let dual = Path::try_from((path_a, path_b)).unwrap().dual().as_arr();
    let other = if dual[0] == current_third {
        dual[1]
    } else if dual[1] == current_third {
        dual[0]
    } else {
        unreachable!("intersection hex must be one of the path dual hexes")
    };

    Intersection::from_adjacent_hexes([path_a, path_b, other])
}

#[cfg(test)]
mod test {
    use super::*;

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    #[test]
    fn intersection_works() {
        let v = Intersection::try_from((h(0, 0), h(1, 0), h(1, -1))).unwrap();
        let u = Intersection::try_from((h(0, 0), h(1, -1), h(1, 0))).unwrap();

        assert_eq!(v, u);
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
