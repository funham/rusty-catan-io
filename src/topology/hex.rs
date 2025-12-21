use std::{collections::BTreeSet, hash::Hash};

use itertools::Itertools;

use crate::topology::Intersection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

impl From<(i32, i32)> for Hex {
    fn from(value: (i32, i32)) -> Self {
        Self {
            q: value.0,
            r: value.1,
        }
    }
}

impl Into<(i32, i32)> for Hex {
    fn into(self) -> (i32, i32) {
        (self.q, self.r)
    }
}

impl Into<(i32, i32, i32)> for Hex {
    fn into(self) -> (i32, i32, i32) {
        (self.q, self.r, self.get_s())
    }
}

impl Hex {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub const fn get_s(&self) -> i32 {
        -self.q - self.r
    }

    pub const fn len(&self) -> i32 {
        self.distance(&Self::new(0, 0))
    }

    pub const fn neighbors(&self) -> [Hex; 6] {
        let (q, r) = (self.q, self.r);
        [
            Self::new(q + 1, r + 0),
            Self::new(q + 1, r - 1),
            Self::new(q + 0, r - 1),
            Self::new(q - 1, r + 0),
            Self::new(q - 1, r + 1),
            Self::new(q + 0, r + 1),
        ]
    }

    pub fn neighbors_set(&self) -> BTreeSet<Hex> {
        self.neighbors().into_iter().collect()
    }

    pub fn vertices(&self) -> impl Iterator<Item = Intersection> {
        // assumptions: neighbors are listed in counter or clockwise order
        self.neighbors()
            .into_iter()
            .chain(self.neighbors())
            .take(1)
            .tuple_windows()
            .map(|(h1, h2)| {
                Intersection::try_from((h1, h2, *self))
                    .expect("topology::hex::vertices: assumptions broken")
            })
    }

    pub const fn distance(&self, other: &Self) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        let ds = self.get_s() - other.get_s();

        (dq.abs() + dr.abs() + ds.abs()) / 2
    }

    pub const fn are_neighbors(&self, other: &Self) -> bool {
        self.distance(other) == 1
    }
}

impl std::fmt::Display for Hex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "q:{}, r:{}, s:{}", self.q, self.r, self.get_s())
    }
}

impl Hash for Hex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.q.hash(state);
        self.r.hash(state);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
