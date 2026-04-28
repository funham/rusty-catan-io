use std::{collections::BTreeSet, hash::Hash, sync::OnceLock};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::topology::{Intersection, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
        Self::Output {
            q: self.q + rhs.q,
            r: self.r + rhs.r,
        }
    }
}

impl std::ops::Sub for Hex {
    type Output = Hex;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            q: self.q - rhs.q,
            r: self.r - rhs.r,
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

    pub const fn norm(&self) -> usize {
        self.distance(&Self::new(0, 0))
    }

    /// Returns the six neighboring hexes in the order:
    /// East, Northeast, Northwest, West, Southwest, Southeast
    ///
    /// # Examples
    ///
    /// ```
    /// use catan_core::topology::Hex;
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

    pub fn vertices(&self) -> impl Iterator<Item = Intersection> + use<'_> {
        // assumptions: neighbors are listed in counter or clockwise order starting with the North-East
        self.neighbors()
            .into_iter()
            .chain(self.neighbors().into_iter().take(1))
            .tuple_windows()
            .map(|(h1, h2)| {
                Intersection::try_from((h1, h2, *self))
                    .expect("topology::hex::vertices: assumptions broken")
            })
    }

    pub fn vertices_arr(&self) -> [Intersection; 6] {
        self.vertices()
            .collect::<Vec<_>>()
            .try_into()
            .expect("Hexagon has 6 vertices. Duh.")
    }

    pub fn paths_arr(&self) -> [Path; 6] {
        TryInto::<[Path; 6]>::try_into(
            self.neighbors()
                .iter()
                .map(|h| Path::try_from((*self, *h)).unwrap())
                .collect::<Vec<_>>(),
        )
        .expect("6 paths around a hex. Duh.")
    }

    pub const fn distance(&self, other: &Self) -> usize {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        let ds = self.get_s() - other.get_s();

        (dq.abs() + dr.abs() + ds.abs()) as usize / 2
    }

    pub const fn are_neighbors(&self, other: &Self) -> bool {
        self.distance(other) == 1
    }

    pub const fn directions() -> [Hex; 6] {
        Hex { q: 0, r: 0 }.neighbors()
    }

    pub fn direction(index: usize) -> Hex {
        static CACHE: OnceLock<[Hex; 6]> = OnceLock::new();
        CACHE.get_or_init(|| Self::directions())[index]
    }

    pub const fn index(&self) -> HexIndex {
        HexIndex { hex: *self }
    }
}

pub enum SignedAxis {
    QP, // Q+
    QN, // Q-
    RP,
    RN,
    SP,
    SN,
}

/// Q: [_], R: [/], S: [\\]
pub enum Axis {
    Q,
    R,
    S,
}

impl SignedAxis {
    pub fn from_dir(dir_index: usize) -> SignedAxis {
        assert!((0..6).contains(&dir_index));

        match dir_index {
            0 => SignedAxis::RP,
            1 => SignedAxis::SP,
            2 => SignedAxis::QP,
            3 => SignedAxis::RN,
            4 => SignedAxis::SN,
            5 => SignedAxis::QN,
            _ => unreachable!(),
        }
    }

    pub const fn dir_index(&self) -> usize {
        match self {
            SignedAxis::QP => 2,
            SignedAxis::QN => 5,
            SignedAxis::RP => 0,
            SignedAxis::RN => 3,
            SignedAxis::SP => 1,
            SignedAxis::SN => 4,
        }
    }

    pub const fn unorient(&self) -> Axis {
        match self {
            SignedAxis::QP => Axis::Q,
            SignedAxis::QN => Axis::Q,
            SignedAxis::RP => Axis::R,
            SignedAxis::RN => Axis::R,
            SignedAxis::SP => Axis::S,
            SignedAxis::SN => Axis::S,
        }
    }

    pub fn dir(&self) -> Hex {
        Hex::direction(self.dir_index())
    }
}

impl Axis {
    pub fn from_dir(dir_index: usize) -> Axis {
        SignedAxis::from_dir(dir_index).unorient()
    }

    pub fn from_path(path: Path) -> Axis {
        let (h1, h2) = path.as_pair();
        let dir_index = Hex::directions()
            .iter()
            .position(|d| *d == (h1 - h2))
            .expect("must be one of those");

        Self::from_dir(dir_index)
    }

    pub const fn dir_index(&self) -> usize {
        self.orient(true).dir_index()
    }

    pub fn dir(&self) -> Hex {
        self.orient(true).dir()
    }

    pub const fn orient(&self, positive: bool) -> SignedAxis {
        match self {
            Axis::Q => {
                if positive {
                    SignedAxis::QP
                } else {
                    SignedAxis::QN
                }
            }
            Axis::R => {
                if positive {
                    SignedAxis::RP
                } else {
                    SignedAxis::RN
                }
            }
            Axis::S => {
                if positive {
                    SignedAxis::SP
                } else {
                    SignedAxis::SN
                }
            }
        }
    }
}

pub struct HexIndex {
    pub hex: Hex,
}

impl HexIndex {
    pub const fn ring_size(radius: usize) -> usize {
        match radius {
            0 => 1,
            radius => radius * 6,
        }
    }

    pub fn hex_ring(center: Hex, radius: usize) -> Vec<Hex> {
        if radius == 0 {
            return vec![center];
        }

        let mut results = Vec::new();
        let mut hex = center + Hex::directions()[4] * radius as i32;

        for i in 0..6 {
            for _ in 0..radius {
                results.push(hex);
                hex = hex.neighbors()[i];
            }
        }

        results
    }

    pub fn spiral() -> impl Iterator<Item = Hex> {
        (0..).map(|i| Self::spiral_to_hex(i))
    }

    pub fn spiral_to_hex(index: usize) -> Hex {
        let center = Hex::new(0, 0);
        let radius = Self::spiral_to_radius(index);
        let ring_start = Self::spiral_start_of_ring(radius);

        Self::hex_ring(center, radius)[index - ring_start]
    }

    pub fn hex_to_spiral(hex: Hex) -> usize {
        let center = Hex::new(0, 0);
        let radius = hex.distance(&center) as usize;
        let ring_hexes = Self::hex_ring(center, radius);
        for i in 0..ring_hexes.len() {
            if hex == ring_hexes[i] {
                return i + Self::spiral_start_of_ring(radius);
            }
        }
        unreachable!("trust me bro")
    }

    pub fn to_spiral(&self) -> usize {
        Self::hex_to_spiral(self.hex)
    }

    pub const fn spiral_start_of_ring(radius: usize) -> usize {
        match radius {
            0 => 0,
            radius => 1 + 3 * radius * (radius - 1),
        }
    }

    pub const fn spiral_to_radius(index: usize) -> usize {
        match index {
            0 => 0,
            index => ((12 * index - 3).isqrt() + 3) / 6,
        }
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
    use super::*;

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn v(h1: Hex, h2: Hex, h3: Hex) -> Intersection {
        Intersection::try_from((h1, h2, h3)).unwrap()
    }

    #[test]
    fn hex_vertices() {
        let h1 = Hex::new(0, 0);
        let vs = h1.vertices().collect::<BTreeSet<_>>();
        assert_eq!(
            vs,
            BTreeSet::from([
                v(h(0, 0), h(-1, 0), h(0, -1)),
                v(h(0, 0), h(1, -1), h(0, -1)),
                v(h(0, 0), h(1, -1), h(1, 0)),
                v(h(0, 0), h(1, 0), h(0, 1)),
                v(h(0, 0), h(0, 1), h(-1, 1)),
                v(h(0, 0), h(-1, 1), h(-1, 0)),
            ])
        );
    }

    #[test]
    fn hex_index_ring_basic() {
        assert_eq!(HexIndex::ring_size(0), 1);
        assert_eq!(HexIndex::ring_size(1), 6);
        assert_eq!(HexIndex::ring_size(2), 12);
        assert_eq!(
            HexIndex::hex_ring(h(32, -12), 1)
                .into_iter()
                .collect::<BTreeSet<_>>(),
            h(32, -12).neighbors_set()
        );
    }

    #[test]
    fn hex_index_ring_advanced() {
        let radius: i32 = 3;

        let mut ring_brute = Vec::new();
        for q in -radius * 2..=radius * 2 {
            for r in -radius * 2..=radius * 2 {
                if h(q, r).norm() == radius as usize {
                    ring_brute.push(h(q, r));
                }
            }
        }

        assert_eq!(
            ring_brute.iter().sorted().collect::<Vec<_>>(),
            HexIndex::hex_ring(h(0, 0), radius as usize)
                .iter()
                .sorted()
                .collect::<Vec<_>>()
        );

        let center = h(123, -434);

        let radius: i32 = 5;

        let mut ring_brute = Vec::new();
        for q in center.q - radius * 2..=center.q + radius * 2 {
            for r in center.r - radius * 2..=center.r + radius * 2 {
                if h(q, r).distance(&center) == radius as usize {
                    ring_brute.push(h(q, r));
                }
            }
        }

        assert_eq!(
            ring_brute.iter().sorted().collect::<Vec<_>>(),
            HexIndex::hex_ring(center, radius as usize)
                .iter()
                .sorted()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn spiral_index() {
        // spiral to hex
        assert_eq!(HexIndex::spiral_to_hex(0), h(0, 0));
        assert_eq!(HexIndex::spiral_to_hex(1), h(-1, 1));
        assert_eq!(HexIndex::spiral_to_hex(2), h(0, 1));
        assert_eq!(HexIndex::spiral_to_hex(3), h(1, 0));
        assert_eq!(HexIndex::spiral_to_hex(4), h(1, -1));
        assert_eq!(HexIndex::spiral_to_hex(5), h(0, -1));
        assert_eq!(HexIndex::spiral_to_hex(6), h(-1, 0));
        assert_eq!(HexIndex::spiral_to_hex(7), h(-2, 2));

        // hex to spiral
        for i in 0..69 {
            let hex = HexIndex::spiral_to_hex(i);
            assert_eq!(i, HexIndex::hex_to_spiral(hex));
        }
    }
}
