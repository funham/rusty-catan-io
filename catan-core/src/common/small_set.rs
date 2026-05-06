use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SmallSet<T, const N: usize> {
    data: SmallVec<[T; N]>,
}

impl<T, const N: usize> Default for SmallSet<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> SmallSet<T, N> {
    pub fn new() -> Self {
        Self {
            data: SmallVec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn as_slice(&self) -> &[T] {
        self.data.as_slice()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn spilled(&self) -> bool {
        self.data.spilled()
    }
}

impl<T: Ord, const N: usize> SmallSet<T, N> {
    pub fn contains(&self, value: &T) -> bool {
        self.data.binary_search(value).is_ok()
    }

    pub fn insert(&mut self, value: T) -> bool {
        match self.data.binary_search(&value) {
            Ok(_) => false,
            Err(pos) => {
                self.data.insert(pos, value);
                true
            }
        }
    }

    pub fn remove(&mut self, value: &T) -> bool {
        match self.data.binary_search(value) {
            Ok(pos) => {
                self.data.remove(pos);
                true
            }
            Err(_) => false,
        }
    }

    pub fn is_disjoint<const M: usize>(&self, other: &SmallSet<T, M>) -> bool {
        let mut left = 0;
        let mut right = 0;
        while left < self.data.len() && right < other.data.len() {
            match self.data[left].cmp(&other.data[right]) {
                std::cmp::Ordering::Less => left += 1,
                std::cmp::Ordering::Equal => return false,
                std::cmp::Ordering::Greater => right += 1,
            }
        }
        true
    }
}

impl<T: Ord + Clone, const N: usize> SmallSet<T, N> {
    pub fn union<const M: usize>(&self, other: &SmallSet<T, M>) -> SmallSet<T, N> {
        let mut result = SmallSet::new();
        let mut left = 0;
        let mut right = 0;

        while left < self.data.len() && right < other.data.len() {
            match self.data[left].cmp(&other.data[right]) {
                std::cmp::Ordering::Less => {
                    result.data.push(self.data[left].clone());
                    left += 1;
                }
                std::cmp::Ordering::Equal => {
                    result.data.push(self.data[left].clone());
                    left += 1;
                    right += 1;
                }
                std::cmp::Ordering::Greater => {
                    result.data.push(other.data[right].clone());
                    right += 1;
                }
            }
        }

        result.data.extend(self.data[left..].iter().cloned());
        result.data.extend(other.data[right..].iter().cloned());
        result
    }

    pub fn intersection<const M: usize>(&self, other: &SmallSet<T, M>) -> SmallSet<T, N> {
        self.iter()
            .filter(|value| other.contains(value))
            .cloned()
            .collect()
    }

    pub fn difference<const M: usize>(&self, other: &SmallSet<T, M>) -> SmallSet<T, N> {
        self.iter()
            .filter(|value| !other.contains(value))
            .cloned()
            .collect()
    }

    pub fn symmetric_difference<const M: usize>(&self, other: &SmallSet<T, M>) -> SmallSet<T, N> {
        self.difference(other).union(&other.difference(self))
    }
}

impl<T: Ord, const N: usize> Extend<T> for SmallSet<T, N> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.insert(value);
        }
    }
}

impl<T: Ord, const N: usize> FromIterator<T> for SmallSet<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut data: SmallVec<[T; N]> = iter.into_iter().collect();
        data.sort_unstable();
        data.dedup();
        Self { data }
    }
}

impl<T: Ord, const N: usize, const M: usize> From<[T; M]> for SmallSet<T, N> {
    fn from(value: [T; M]) -> Self {
        value.into_iter().collect()
    }
}

impl<T: Ord, const N: usize> From<BTreeSet<T>> for SmallSet<T, N> {
    fn from(value: BTreeSet<T>) -> Self {
        Self {
            data: value.into_iter().collect(),
        }
    }
}

impl<T: Ord, const N: usize> From<SmallSet<T, N>> for BTreeSet<T> {
    fn from(value: SmallSet<T, N>) -> Self {
        value.into_iter().collect()
    }
}

impl<T, const N: usize> IntoIterator for SmallSet<T, N> {
    type Item = T;
    type IntoIter = smallvec::IntoIter<[T; N]>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a SmallSet<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

impl<T: Serialize, const N: usize> Serialize for SmallSet<T, N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.data.serialize(serializer)
    }
}

impl<'de, T: Ord + Deserialize<'de>, const N: usize> Deserialize<'de> for SmallSet<T, N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<T>::deserialize(deserializer)?;
        Ok(values.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn collect_sorts_and_deduplicates() {
        let set = [3, 1, 2, 3, 1].into_iter().collect::<SmallSet<_, 2>>();

        assert_eq!(set.as_slice(), &[1, 2, 3]);
        assert!(set.spilled());
    }

    #[test]
    fn insert_remove_and_contains_preserve_set_semantics() {
        let mut set = SmallSet::<_, 4>::new();

        assert!(set.insert(2));
        assert!(set.insert(1));
        assert!(!set.insert(2));
        assert_eq!(set.as_slice(), &[1, 2]);
        assert!(set.contains(&1));

        assert!(set.remove(&1));
        assert!(!set.remove(&1));
        assert_eq!(set.as_slice(), &[2]);
    }

    #[test]
    fn set_algebra_matches_sorted_set_behavior() {
        let left = SmallSet::<_, 4>::from([1, 2, 4]);
        let right = SmallSet::<_, 4>::from([2, 3, 4]);

        assert_eq!(left.union(&right).as_slice(), &[1, 2, 3, 4]);
        assert_eq!(left.intersection(&right).as_slice(), &[2, 4]);
        assert_eq!(left.difference(&right).as_slice(), &[1]);
        assert_eq!(left.symmetric_difference(&right).as_slice(), &[1, 3]);
        assert!(!left.is_disjoint(&right));
        assert!(SmallSet::<_, 4>::from([8]).is_disjoint(&right));
    }

    #[test]
    fn converts_to_and_from_btree_set() {
        let source = BTreeSet::from([3, 1, 2]);
        let small = SmallSet::<_, 2>::from(source.clone());
        let restored: BTreeSet<_> = small.into();

        assert_eq!(restored, source);
    }

    #[test]
    fn into_iterator_yields_sorted_unique_values() {
        let values = SmallSet::<_, 4>::from([2, 1, 2, 3])
            .into_iter()
            .collect::<Vec<_>>();

        assert_eq!(values, vec![1, 2, 3]);
    }

    #[test]
    fn serde_uses_plain_sequence_shape() {
        let set = SmallSet::<_, 2>::from([3, 1, 2, 1]);

        assert_eq!(serde_json::to_string(&set).unwrap(), "[1,2,3]");

        let restored: SmallSet<i32, 2> = serde_json::from_str("[3,1,2,1]").unwrap();
        assert_eq!(restored.as_slice(), &[1, 2, 3]);
    }
}
