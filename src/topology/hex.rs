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

impl std::ops::Add for Hex {
    type Output = Hex;

    fn add(self, rhs: Self) -> Self::Output {
        Hex {
            q: self.q + rhs.q,
            r: self.r + rhs.r,
        }
    }
}

impl std::ops::Mul<i32> for Hex {
    type Output = Hex;

    fn mul(self, c: i32) -> Self::Output {
        Hex {
            q: self.q * c,
            r: self.r * c,
        }
    }
}

impl Hex {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub const fn get_s(&self) -> i32 {
        -self.q - self.r
    }

    pub const fn norm(&self) -> u32 {
        self.distance(&Self::new(0, 0))
    }

    /// Returns the six neighboring hexes in the order:
    /// East, Northeast, Northwest, West, Southwest, Southeast
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::topology::Hex;
    ///
    /// let center = Hex::new(0, 0);
    /// let neighbors = center.neighbors();
    ///
    /// // East
    /// assert_eq!(neighbors[0], Hex::new(1, 0));
    /// // Northeast
    /// assert_eq!(neighbors[1], Hex::new(1, -1));
    /// // Northwest
    /// assert_eq!(neighbors[2], Hex::new(0, -1));
    /// // West
    /// assert_eq!(neighbors[3], Hex::new(-1, 0));
    /// // Southwest
    /// assert_eq!(neighbors[4], Hex::new(-1, 1));
    /// // Southeast
    /// assert_eq!(neighbors[5], Hex::new(0, 1));
    ///
    /// // Test with non-zero starting point
    /// let hex = Hex::new(3, 2);
    /// let neighbors = hex.neighbors();
    ///
    /// assert_eq!(neighbors[0], Hex::new(4, 2));
    /// assert_eq!(neighbors[1], Hex::new(4, 1));
    /// assert_eq!(neighbors[2], Hex::new(3, 1));
    /// assert_eq!(neighbors[3], Hex::new(2, 2));
    /// assert_eq!(neighbors[4], Hex::new(2, 3));
    /// assert_eq!(neighbors[5], Hex::new(3, 3));
    ///
    /// // Verify we get exactly 6 neighbors
    /// assert_eq!(neighbors.len(), 6);
    ///
    /// // Verify all neighbors are distinct
    /// for i in 0..neighbors.len() {
    ///     for j in 0..neighbors.len() {
    ///         if i != j {
    ///             assert_ne!(neighbors[i], neighbors[j]);
    ///         }
    ///     }
    /// }
    /// ```
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

    pub const fn distance(&self, other: &Self) -> u32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        let ds = self.get_s() - other.get_s();

        (dq.abs() + dr.abs() + ds.abs()) as u32 / 2
    }

    pub const fn are_neighbors(&self, other: &Self) -> bool {
        self.distance(other) == 1
    }

    pub const fn directions() -> [Hex; 6] {
        Hex { q: 0, r: 0 }.neighbors()
    }

    pub const fn ring_size(ring: u32) -> u32 {
        ring * 6
    }

    // pub const fn spiral_index(&self) -> u32 {
    //     let mut dir = Self::directions();
    //     let mut d2 = [Hex { q: 0, r: 0 }; 6];

    //     let mut i = 0;
    //     while i < 6 {
    //         d2[i] = dir[i] * 6;
    //         i += 1;
    //     }

    //     // East, Northeast, Northwest, West, Southwest, Southeast
    //     let (pos, side) = match (self.q, self.r, self.get_s()) {
    //         (_, _, s) if dir[0].get_s() == s => (self.q, 0),
    //         (_, r, _) if dir[1].r == r => (-self.get_s(), 1),
    //         (q, _, _) if dir[2].q == q => (self.r, 2),
    //         (_, _, s) if dir[3].get_s() == s => (-self.q, 3),
    //         (_, r, _) if dir[4].r == r => (self.get_s(), 4),
    //         (q, _, _) if dir[5].q == q => (-self.r, 5),
    //         _ => unreachable!(),
    //     };

    //     1 + 3 * self.norm() * (self.norm() - 1) + side * 6 + pos as u32
    // }
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
