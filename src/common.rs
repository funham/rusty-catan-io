pub use std::collections::BTreeSet;
use std::mem::swap;

pub struct Indexed<I, T> {
    id: I,
    val: T,
}

pub struct Set2<T: Ord + PartialEq> {
    pub a: T,
    pub b: T,
}

impl<T: Ord + PartialEq> From<(T, T)> for Set2<T> {
    fn from(item: (T, T)) -> Self {
        let (mut a, mut b) = item;

        if a < b {
            swap(&mut a, &mut b);
        }

        Self { a, b }
    }
}

impl<T: Ord + PartialEq> Set2<T> {
    pub fn contains(&self, x: &T) -> bool {
        self.a == *x || self.b == *x
    }
}
