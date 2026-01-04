use std::{
    cmp::max_by,
    ops::{BitAnd, BitOr},
};

use tinyvec::ArrayVec;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Probability(f32);

impl TryFrom<f32> for Probability {
    type Error = ();
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if 0.0 <= value && value <= 1.0 {
            Ok(Self { 0: value })
        } else {
            Err(())
        }
    }
}

impl Into<f32> for Probability {
    fn into(self) -> f32 {
        self.0
    }
}

impl std::ops::Mul for Probability {
    type Output = Probability;

    fn mul(self, rhs: Self) -> Self::Output {
        Self { 0: self.0 * rhs.0 }
    }
}

impl std::ops::Add for Probability {
    type Output = Option<Probability>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::add(&self, &rhs)
    }
}

impl Probability {
    pub unsafe fn new_unchecked(val: f32) -> Self {
        Self { 0: val }
    }

    pub fn new(val: f32) -> Option<Self> {
        TryFrom::<f32>::try_from(val).ok()
    }

    pub unsafe fn add_unchecked(&self, rhs: &Probability) -> Probability {
        Self { 0: self.0 + rhs.0 }
    }

    pub fn add(&self, rhs: &Probability) -> Option<Probability> {
        Self::new(self.0 + rhs.0)
    }

    pub fn to_float(&self) -> f32 {
        self.0
    }

    // do I need it..?
    pub fn zero() -> &'static Self {
        &&Self { 0: 0.0 }
    }

    pub fn one() -> &'static Self {
        &&Self { 0: 1.0 }
    }

    pub fn half() -> &'static Self {
        &&Self { 0: 0.5 }
    }
}

impl Probable for Probability {
    fn prob(&self) -> Probability {
        *self
    }
}

pub trait Probable {
    fn prob(&self) -> Probability;
}

/// Disjunction of possible Die rolls
/// (all unique) (basically Set<DiceVal>)
#[derive(Clone, Copy)]
pub struct Variant<T: Default + Probable + PartialEq + Clone> {
    values: ArrayVec<[T; 11]>,
}

impl<T: Default + Probable + PartialEq + Clone> Variant<T> {
    pub fn new(values: impl IntoIterator<Item = T>) -> Option<Self> {
        let values_vec: Vec<T> = values.into_iter().collect();

        // check
        if values_vec.is_empty() || values_vec.len() > values_vec.capacity() {
            return None;
        }

        // check uniquiness
        if values_vec
            .iter()
            .enumerate()
            .any(|(i, x)| values_vec.as_slice()[i + 1..].contains(x))
        {
            return None;
        }

        Some(Self {
            values: values_vec.into_iter().collect(),
        })
    }
}

impl<T: Default + Probable + PartialEq + Clone> Probable for Variant<T> {
    fn prob(&self) -> Probability {
        self.values.iter().map(|d| d.prob()).fold(
            *Probability::zero(),
            |p, x| p.add(&x).expect("Incorrect probality addition"), // correct while invariant that values[i] != values[j] stands
        )
    }
}

#[derive(Clone)]
pub struct Sequence<T: Default + Probable + PartialEq + Clone> {
    values: Vec<Variant<T>>,
}

impl<T: Default + Probable + PartialEq + Clone> Sequence<T> {
    pub fn new(values: impl IntoIterator<Item = Variant<T>>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }
}

impl<T: Default + Probable + PartialEq + Clone> BitAnd for &Variant<T> {
    type Output = Sequence<T>;

    fn bitand(self, rhs: Self) -> Self::Output {
        Sequence::new([self.clone(), rhs.clone()])
    }
}

impl<T: Default + Probable + PartialEq + Clone> BitAnd for &Sequence<T> {
    type Output = Sequence<T>;

    fn bitand(self, rhs: Self) -> Self::Output {
        Sequence::new([self.values.as_slice(), rhs.values.as_slice()].concat())
    }
}

impl<T: Default + Probable + PartialEq + Clone> BitOr for &Variant<T> {
    type Output = Option<Variant<T>>;

    fn bitor(self, rhs: Self) -> Self::Output {
        Variant::new([self.values.as_slice(), rhs.values.as_slice()].concat())
    }
}

impl<T: Default + Probable + PartialEq + Clone> BitOr for &Sequence<T> {
    type Output = Option<Sequence<T>>;

    fn bitor(self, rhs: Self) -> Self::Output {
        let mut result = Vec::with_capacity(self.values.len().min(rhs.values.len()));

        for (v1, v2) in self.values.iter().zip(rhs.values.iter()) {
            match v1 | v2 {
                Some(v) => result.push(v),
                None => return None,
            }
        }

        let smallest_len = std::cmp::min(&self.values.len(), &rhs.values.len()).clone();
        let largest = max_by(&self.values, &rhs.values, |v1, v2| v1.len().cmp(&v2.len()));

        if smallest_len < largest.len() {
            result.extend_from_slice(&largest[smallest_len..]);
        }

        Some(Sequence::new(result))
    }
}

impl<T: Default + Probable + PartialEq + Clone> Probable for Sequence<T> {
    fn prob(&self) -> Probability {
        self.values
            .iter()
            .map(|d| d.prob())
            .fold(*Probability::one(), |p, x| p * x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Probability Tests =====
    #[test]
    fn probability_creation_valid() {
        assert!(Probability::new(0.0).is_some());
        assert!(Probability::new(0.5).is_some());
        assert!(Probability::new(1.0).is_some());
        assert!(Probability::new(0.333).is_some());
    }

    #[test]
    fn probability_creation_invalid() {
        assert!(Probability::new(-0.1).is_none());
        assert!(Probability::new(1.1).is_none());
        assert!(Probability::new(f32::NAN).is_none());
        assert!(Probability::new(f32::INFINITY).is_none());
    }

    #[test]
    fn probability_try_from() {
        assert!(Probability::try_from(0.5).is_ok());
        assert!(Probability::try_from(2.0).is_err());
    }

    #[test]
    fn probability_multiplication() {
        let p1 = Probability::new(0.5).unwrap();
        let p2 = Probability::new(0.4).unwrap();
        let result = p1 * p2;
        assert_eq!(result.0, 0.2);

        // Multiplication should stay in [0, 1]
        let p3 = Probability::new(0.1).unwrap();
        let p4 = Probability::new(0.2).unwrap();
        let result = p3 * p4;
        assert!(result.0 >= 0.0 && result.0 <= 1.0);
    }

    #[test]
    fn probability_addition() {
        let eps = 0.001;

        let p1 = Probability::new(0.3).unwrap();
        let p2 = Probability::new(0.4).unwrap();

        // Valid addition
        assert!(p1.add(&p2).is_some());
        assert!((p1.add(&p2).unwrap().0 - 0.7).abs() < eps);

        // Invalid addition (exceeds 1.0)
        let p3 = Probability::new(0.6).unwrap();
        let p4 = Probability::new(0.5).unwrap();
        assert!(p3.add(&p4).is_none());
    }

    #[test]
    fn probability_static_methods() {
        assert_eq!(Probability::zero().0, 0.0);
        assert_eq!(Probability::one().0, 1.0);
        assert_eq!(Probability::half().0, 0.5);
    }

    #[test]
    fn sequence_empty() {
        // Empty sequence should have probability 1.0
        let sequence = Sequence::<Probability>::new(vec![]);
        assert_eq!(sequence.prob().0, 1.0);
    }
}
