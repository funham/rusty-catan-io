use std::{
    collections::BTreeSet,
    ffi::os_str::Display,
    fmt::write,
    hash::{Hash, Hasher},
};

use itertools::Itertools;

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

impl Hex {
    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub fn get_s(&self) -> i32 {
        -self.q - self.r
    }

    pub fn len(&self) -> i32 {
        self.distance(&Self::new(0, 0))
    }

    pub fn neighbors(&self) -> impl Iterator<Item = Hex> {
        (-1..=1)
            .permutations(2)
            .map(|delta| Self::new(delta[0], delta[1]))
    }

    pub fn distance(&self, other: &Self) -> i32 {
        (self.q - other.q).abs()
            + (self.r - other.r).abs()
            + (self.get_s() - other.get_s()).abs() / 2
    }

    pub fn are_neighbors(&self, other: &Self) -> bool {
        self.distance(other) == 1
    }
}

impl std::fmt::Display for Hex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "q:{}, r:{}, s:{}", self.q, self.r, self.get_s())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
