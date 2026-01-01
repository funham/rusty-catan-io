use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedSet<T: Ord, const N: usize> {
    data_: BTreeSet<T>,
}

impl<T: Ord, const N: usize> FixedSet<T, N> {
    pub fn contains(&self, x: &T) -> bool {
        self.data_.contains(x)
    }
}

impl<T: Ord, const N: usize> Into<[T; N]> for FixedSet<T, N> {
    fn into(self) -> [T; N] {
        <[T; N] as TryFrom<Vec<T>>>::try_from(self.data_.into_iter().collect())
            .unwrap_or_else(|_| unreachable!("FixedSet<{}> invariant violated", N))
    }
}

impl<T: Ord, const N: usize> TryFrom<BTreeSet<T>> for FixedSet<T, N> {
    type Error = BTreeSet<T>;

    fn try_from(value: BTreeSet<T>) -> Result<Self, Self::Error> {
        match value.len() {
            x if x == N => Ok(Self { data_: value }),
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

    type IntoIter = std::collections::btree_set::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data_.into_iter()
    }
}
