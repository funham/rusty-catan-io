use std::num::NonZeroU8;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

use crate::math::probability::{Probability, Probable};

/// Const-instantiates a [`crate::math::dice::DiceRoll`] from a const `u8` expression.
///
/// Invalid values fail at compile time.
///
/// ```ignore
/// let roll = dice_roll!(7);
/// let roll = dice_roll!(7u8);
/// // let bad = dice_roll!(13); // compile-time error
/// ```
#[macro_export]
macro_rules! dice_roll {
    ($value:expr $(,)?) => {
        const {
            match $crate::math::dice::DiceRoll::new($value) {
                Some(roll) => roll,
                None => panic!("invalid DiceRoll value: expected value in 2..=12"),
            }
        }
    };
}

/// Const-instantiates a [`crate::math::dice::DiceOutcome`] from a const `u8` expression.
///
/// Values in `2..=12` are valid. `7` becomes [`crate::math::dice::DiceOutcome::Seven`],
/// and every other valid value becomes [`crate::math::dice::DiceOutcome::Harvest`].
/// Invalid values fail at compile time.
///
/// ```ignore
/// let outcome = dice_outcome!(8);
/// let seven = dice_outcome!(7u8);
/// // let bad = dice_outcome!(1); // compile-time error
/// ```
#[macro_export]
macro_rules! dice_outcome {
    ($value:expr $(,)?) => {
        const {
            match $crate::math::dice::DiceOutcome::new($value) {
                Some(outcome) => outcome,
                None => panic!("invalid DiceOutcome value: expected value in 2..=12, excluding 7"),
            }
        }
    };
}
/// Value that can be produced by rolling two D6's.
///
/// Invariant: `DiceRoll \in [2..=12]`.
///
/// Internally this is backed by [`NonZeroU8`], so `Option<DiceRoll>` has the
/// same size as `DiceRoll`/`u8`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiceRoll(NonZeroU8);

/// Number that can be assigned to a harvestable tile.
///
/// Invariant: `TileNum \in [2..=12] \ {7}`.
///
/// This is also backed by [`NonZeroU8`] for consistency with [`DiceRoll`].
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileNum(NonZeroU8);

/// Resolved outcome of a dice roll.
///
/// This is a real enum so callers can match directly on
/// `DiceOutcome::Harvest(num)` and `DiceOutcome::Seven`.
///
/// Because `TileNum` is backed by [`NonZeroU8`], the compiler can use `0` as
/// the internal niche for the `Seven` variant, so `DiceOutcome` itself remains
/// compact. The tradeoff is that `Option<DiceOutcome>` generally needs an extra
/// discriminant, unlike the previous transparent-wrapper representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DiceOutcome {
    Harvest(TileNum),
    #[default]
    Seven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiceRollError {
    OutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileNumError {
    OutOfRange,
    Seven,
}

impl DiceRoll {
    pub const MIN_VALUE: u8 = 2;
    pub const MAX_VALUE: u8 = 12;
    pub const COUNT: usize = (Self::MAX_VALUE - Self::MIN_VALUE + 1) as usize;
    pub const ALL: [DiceRoll; 11] = Self::all();

    const fn all() -> [DiceRoll; Self::COUNT] {
        let mut result = [dice_roll!(2); Self::COUNT];

        let mut i = 0;
        while i < Self::COUNT {
            result[i] = Self::new(Self::MIN_VALUE + i as u8)
                .expect("invalid DiceRoll value while constructing DiceRoll::ALL");

            i += 1;
        }

        result
    }

    const fn is_valid_dice_roll(value: u8) -> bool {
        value >= Self::MIN_VALUE && value <= Self::MAX_VALUE
    }

    /// Creates a [`DiceRoll`] without checking the range invariant.
    ///
    /// # Safety
    ///
    /// `val` must be in `2..=12`.
    pub const unsafe fn new_unchecked(value: u8) -> Self {
        debug_assert!(Self::is_valid_dice_roll(value));

        // SAFETY: upheld by the caller. Every valid dice roll is non-zero.
        Self(unsafe { NonZeroU8::new_unchecked(value) })
    }

    pub const fn new(value: u8) -> Option<Self> {
        if Self::is_valid_dice_roll(value) {
            // SAFETY: checked above.
            Some(unsafe { Self::new_unchecked(value) })
        } else {
            None
        }
    }

    pub const fn from_tile(tile: TileNum) -> Self {
        // # Safety: TileNum is a subset of DiceRoll
        unsafe { Self::new_unchecked(tile.get()) }
    }

    pub fn iter() -> impl Iterator<Item = DiceRoll> {
        Self::ALL.into_iter()
    }

    pub const fn get(self) -> u8 {
        self.0.get()
    }

    pub const fn prob_pts(&self) -> u8 {
        let delta = self.get() as i32 - 7;
        6 - delta.unsigned_abs() as u8
    }

    pub const fn resolve(self) -> DiceOutcome {
        DiceOutcome::from_dice_roll(self)
    }
}

impl Serialize for DiceRoll {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.get())
    }
}

impl<'de> Deserialize<'de> for DiceRoll {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}

impl Probable for DiceRoll {
    fn prob(&self) -> Probability {
        const D6_SIDES: u32 = 6;
        let prob = self.prob_pts() as f32 / (D6_SIDES * D6_SIDES) as f32;
        prob.try_into().expect("check math")
    }
}

impl TryFrom<u8> for DiceRoll {
    type Error = DiceRollError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(DiceRollError::OutOfRange)
    }
}

impl From<DiceRoll> for u8 {
    fn from(value: DiceRoll) -> u8 {
        value.get()
    }
}

impl std::fmt::Display for DiceRollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl TileNum {
    /// Creates a [`TileNum`] without checking the range invariant.
    ///
    /// # Safety
    ///
    /// `val` must be in `2..=12` and must not be `7`.
    pub const unsafe fn new_unchecked(val: u8) -> Self {
        // SAFETY: upheld by the caller. Every valid tile number is non-zero.
        Self(unsafe { NonZeroU8::new_unchecked(val) })
    }

    pub const fn new(value: u8) -> Option<Self> {
        match DiceRoll::new(value) {
            Some(roll) => Self::from_dice_roll(roll),
            _ => None,
        }
    }

    pub const fn from_dice_roll(roll: DiceRoll) -> Option<Self> {
        match roll.get() {
            7 => None,
            // SAFETY: upheld by the caller. Every valid tile number is non-zero.
            value => unsafe { Some(Self::new_unchecked(value)) },
        }
    }

    pub fn iter() -> impl Iterator<Item = TileNum> {
        DiceRoll::iter().flat_map(Self::from_dice_roll)
    }

    pub const fn get(self) -> u8 {
        self.0.get()
    }

    pub const fn as_roll(self) -> DiceRoll {
        DiceRoll::from_tile(self)
    }
}

impl TryFrom<u8> for TileNum {
    type Error = TileNumError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match DiceRoll::try_from(value) {
            Ok(roll) => Self::try_from(roll),
            Err(_) => Err(TileNumError::OutOfRange),
        }
    }
}

impl TryFrom<DiceRoll> for TileNum {
    type Error = TileNumError;

    fn try_from(value: DiceRoll) -> Result<Self, Self::Error> {
        match value.get() {
            7 => Err(TileNumError::Seven),
            value => {
                // SAFETY: values of diceroll are enforced, and
                // the previous arm excluded 7.
                Ok(unsafe { Self::new_unchecked(value) })
            }
        }
    }
}

impl From<TileNum> for u8 {
    fn from(value: TileNum) -> Self {
        value.get()
    }
}

impl From<TileNum> for DiceRoll {
    fn from(value: TileNum) -> Self {
        Self::from_tile(value)
    }
}

impl Serialize for TileNum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.get())
    }
}

impl<'de> Deserialize<'de> for TileNum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}

impl std::fmt::Display for TileNumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl DiceOutcome {
    pub const fn new(value: u8) -> Option<Self> {
        match DiceRoll::new(value) {
            Some(roll) => Some(Self::from_dice_roll(roll)),
            None => None,
        }
    }

    pub const fn from_dice_roll(roll: DiceRoll) -> Self {
        match roll.get() {
            7 => Self::Seven,
            value => {
                // SAFETY: `roll` is valid and this branch excludes 7, so this
                // value is a valid TileNum.
                Self::Harvest(unsafe { TileNum::new_unchecked(value) })
            }
        }
    }

    pub const fn harvest(tile_num: TileNum) -> Self {
        Self::Harvest(tile_num)
    }

    pub const fn seven() -> Self {
        Self::Seven
    }

    pub const fn as_roll(self) -> DiceRoll {
        match self {
            Self::Harvest(tile_num) => {
                // SAFETY: every valid TileNum is also a valid DiceRoll.
                unsafe { DiceRoll::new_unchecked(tile_num.get()) }
            }
            Self::Seven => dice_roll!(7),
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.as_roll().get()
    }

    pub const fn is_seven(self) -> bool {
        matches!(self, Self::Seven)
    }

    pub const fn tile_num(self) -> Option<TileNum> {
        match self {
            Self::Harvest(tile_num) => Some(tile_num),
            Self::Seven => None,
        }
    }
}

impl From<DiceRoll> for DiceOutcome {
    fn from(value: DiceRoll) -> Self {
        Self::from_dice_roll(value)
    }
}

impl From<TileNum> for DiceOutcome {
    fn from(value: TileNum) -> Self {
        Self::harvest(value)
    }
}

impl From<DiceOutcome> for DiceRoll {
    fn from(value: DiceOutcome) -> Self {
        value.as_roll()
    }
}

impl From<DiceOutcome> for u8 {
    fn from(value: DiceOutcome) -> Self {
        value.as_u8()
    }
}

impl Serialize for DiceOutcome {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for DiceOutcome {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        DiceRoll::try_from(value)
            .map(Self::from_dice_roll)
            .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::probability::{Sequence, Variant};
    use std::mem::size_of;

    #[test]
    fn dice_roll_resolves_seven_separately_from_harvest_numbers() {
        assert_eq!(dice_roll!(7).resolve(), DiceOutcome::Seven);

        for value in [2, 3, 4, 5, 6, 8, 9, 10, 11, 12] {
            let roll = DiceRoll::try_from(value).unwrap();
            let tile = TileNum::try_from(value).unwrap();

            assert_eq!(roll.resolve(), DiceOutcome::Harvest(tile));
        }
    }

    #[test]
    fn tile_num_rejects_seven_but_accepts_harvest_numbers() {
        for value in [2, 3, 4, 5, 6, 8, 9, 10, 11, 12] {
            assert!(TileNum::try_from(value).is_ok());
        }

        assert!(TileNum::try_from(7).is_err());
        assert!(TileNum::try_from(1).is_err());
        assert!(TileNum::try_from(13).is_err());
    }

    #[test]
    fn nonzero_layout_advantage_is_preserved() {
        assert_eq!(size_of::<DiceRoll>(), 1);
        assert_eq!(size_of::<Option<DiceRoll>>(), 1);

        assert_eq!(size_of::<TileNum>(), 1);
        assert_eq!(size_of::<Option<TileNum>>(), 1);

        assert_eq!(size_of::<DiceOutcome>(), 1);
        // Direct enum matching costs us the transparent-wrapper niche for
        // Option<DiceOutcome>. DiceOutcome itself is still compact.
    }

    #[test]
    fn complete_workflow() {
        // Create some dice values
        let low_values = vec![
            DiceRoll::try_from(2).unwrap(),
            DiceRoll::try_from(3).unwrap(),
        ];

        let high_values = vec![
            DiceRoll::try_from(11).unwrap(),
            DiceRoll::try_from(12).unwrap(),
        ];

        // Create variants
        let low_variant = Variant::new(low_values).unwrap();
        let high_variant = Variant::new(high_values).unwrap();

        // Create sequence
        let sequence = Sequence::new(vec![low_variant, high_variant]);

        // Calculate probability
        let prob = sequence.prob();
        assert!(prob.to_float() > 0.0 && prob.to_float() < 1.0);
    }
}
