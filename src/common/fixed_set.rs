use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedSet<T: Ord, const N: usize> {
    data_: [T; N],
}

impl<T: Ord, const N: usize> FixedSet<T, N> {
    pub fn contains(&self, x: &T) -> bool {
        match self.data_.binary_search(x) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data_.iter()
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

    fn try_from(value: [T; N]) -> Result<Self, Self::Error> {
        match <BTreeSet<T> as TryInto<FixedSet<T, N>>>::try_into(
            value.into_iter().collect::<BTreeSet<T>>(),
        ) {
            Ok(x) => Ok(x),
            Err(e) => Err(
                <[T; N] as TryFrom<Vec<T>>>::try_from(e.into_iter().collect::<Vec<_>>())
                    .unwrap_or_else(|_| unreachable!()),
            ),
        }
    }
}

impl<T: Ord, const N: usize> IntoIterator for FixedSet<T, N> {
    type Item = T;

    type IntoIter = std::array::IntoIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.data_.into_iter()
    }
}
