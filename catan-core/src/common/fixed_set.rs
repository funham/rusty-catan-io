use std::collections::BTreeSet;

use super::small_set::SmallSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedSet<T: Ord, const N: usize> {
    data_: [T; N],
}

impl<T: Ord, const N: usize> FixedSet<T, N> {
    pub const fn len(&self) -> usize {
        N
    }

    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data_
    }

    pub fn contains(&self, x: &T) -> bool {
        self.data_.binary_search(x).is_ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data_.iter()
    }

    pub fn is_disjoint<const M: usize>(&self, other: &FixedSet<T, M>) -> bool {
        let mut left = 0;
        let mut right = 0;
        while left < N && right < M {
            match self.data_[left].cmp(&other.data_[right]) {
                std::cmp::Ordering::Less => left += 1,
                std::cmp::Ordering::Equal => return false,
                std::cmp::Ordering::Greater => right += 1,
            }
        }
        true
    }
}

impl<T: Ord + Clone, const N: usize> FixedSet<T, N> {
    pub fn union<const M: usize>(&self, other: &FixedSet<T, M>) -> SmallSet<T, N> {
        self.iter().chain(other.iter()).cloned().collect()
    }

    pub fn intersection<const M: usize>(&self, other: &FixedSet<T, M>) -> SmallSet<T, N> {
        self.iter()
            .filter(|value| other.contains(value))
            .cloned()
            .collect()
    }

    pub fn difference<const M: usize>(&self, other: &FixedSet<T, M>) -> SmallSet<T, N> {
        self.iter()
            .filter(|value| !other.contains(value))
            .cloned()
            .collect()
    }

    pub fn symmetric_difference<const M: usize>(&self, other: &FixedSet<T, M>) -> SmallSet<T, N> {
        self.difference(other).union(&other.difference(self))
    }
}

impl<T: Ord, const N: usize> AsRef<[T]> for FixedSet<T, N> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: Ord, const N: usize> From<FixedSet<T, N>> for SmallSet<T, N> {
    fn from(value: FixedSet<T, N>) -> Self {
        value.into_iter().collect()
    }
}

impl<T: Ord, const N: usize> TryFrom<Vec<T>> for FixedSet<T, N> {
    type Error = Vec<T>;

    fn try_from(mut value: Vec<T>) -> Result<Self, Self::Error> {
        value.sort_unstable();
        if value.len() != N || value.windows(2).any(|items| items[0] == items[1]) {
            return Err(value);
        }

        match <[T; N] as TryFrom<Vec<T>>>::try_from(value) {
            Ok(data_) => Ok(Self { data_ }),
            Err(value) => Err(value),
        }
    }
}

impl<T: Ord, const N: usize> Into<[T; N]> for FixedSet<T, N> {
    fn into(self) -> [T; N] {
        self.data_
    }
}

impl<T: Ord, const N: usize> Into<BTreeSet<T>> for FixedSet<T, N> {
    fn into(self) -> BTreeSet<T> {
        self.data_.into_iter().collect()
    }
}

impl<T: Ord, const N: usize> TryFrom<BTreeSet<T>> for FixedSet<T, N> {
    type Error = BTreeSet<T>;

    fn try_from(value: BTreeSet<T>) -> Result<Self, Self::Error> {
        match value.len() {
            x if x == N => Ok(Self {
                data_: <[T; N] as TryFrom<Vec<T>>>::try_from(value.into_iter().collect())
                    .unwrap_or_else(|_| unreachable!()),
            }),
            _ => Err(value),
        }
    }
}

impl<T: Ord, const N: usize> TryFrom<[T; N]> for FixedSet<T, N> {
    type Error = [T; N];

    fn try_from(mut value: [T; N]) -> Result<Self, Self::Error> {
        value.sort_unstable();
        if value.windows(2).any(|items| items[0] == items[1]) {
            return Err(value);
        }

        Ok(Self { data_: value })
    }
}

impl<T: Ord, const N: usize> IntoIterator for FixedSet<T, N> {
    type Item = T;

    type IntoIter = std::array::IntoIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.data_.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_constructor_sorts_without_allocation() {
        let set = FixedSet::<_, 3>::try_from([3, 1, 2]).unwrap();

        assert_eq!(Into::<[i32; 3]>::into(set), [1, 2, 3]);
    }

    #[test]
    fn array_constructor_rejects_duplicates() {
        assert!(FixedSet::<_, 3>::try_from([3, 1, 3]).is_err());
    }

    #[test]
    fn fixed_set_exposes_slice_and_len_helpers() {
        let set = FixedSet::<_, 3>::try_from([3, 1, 2]).unwrap();

        assert_eq!(set.len(), 3);
        assert!(!set.is_empty());
        assert_eq!(set.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn fixed_set_algebra_returns_dynamic_small_set() {
        let left = FixedSet::<_, 3>::try_from([1, 2, 4]).unwrap();
        let right = FixedSet::<_, 3>::try_from([2, 3, 4]).unwrap();

        assert_eq!(left.union(&right).as_slice(), &[1, 2, 3, 4]);
        assert_eq!(left.intersection(&right).as_slice(), &[2, 4]);
        assert_eq!(left.difference(&right).as_slice(), &[1]);
        assert_eq!(left.symmetric_difference(&right).as_slice(), &[1, 3]);
        assert!(!left.is_disjoint(&right));
        assert!(left.is_disjoint(&FixedSet::<_, 1>::try_from([9]).unwrap()));
    }
}
