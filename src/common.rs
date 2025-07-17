pub use std::collections::BTreeSet;

pub struct Indexed<I, T> {
    id: I,
    val: T,
}