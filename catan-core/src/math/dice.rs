use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

use crate::math::probability::{Probability, Probable};

/// Value that can be produced by rolling two D6's
/// `DiceRoll \in [2..12]` (11 possible states)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiceRoll(u8);

pub type DiceVal = DiceRoll;

/// Number that can be assigned to a harvestable tile.
/// `TileNum \in [2..12] \ {7}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileNum(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiceOutcome {
    Harvest(TileNum),
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
    pub const SEVEN_VALUE: u8 = 7;
    pub const D6_SIDES: u8 = 6;
    pub const PROBABILITY_DENOMINATOR: f32 = 36.0;
    pub const ALL: [DiceRoll; 11] = [
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
        Self(8),
        Self(9),
        Self(10),
        Self(11),
        Self(12),
    ];

    /// # Safety
    /// val should be in [2;12]
    pub const unsafe fn new_unchecked(val: u8) -> Self {
        Self(val)
    }

    pub fn new(value: u8) -> Option<Self> {
        if (Self::MIN_VALUE..=Self::MAX_VALUE).contains(&value) {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn two() -> Self {
        Self::new(2).expect("2 is a valid dice value")
    }

    pub fn three() -> Self {
        Self::new(3).expect("3 is a valid dice value")
    }

    pub fn four() -> Self {
        Self::new(4).expect("4 is a valid dice value")
    }

    pub fn five() -> Self {
        Self::new(5).expect("5 is a valid dice value")
    }

    pub fn six() -> Self {
        Self::new(6).expect("6 is a valid dice value")
    }

    pub fn seven() -> Self {
        Self::new(7).expect("7 is a valid dice value")
    }

    pub fn eight() -> Self {
        Self::new(8).expect("8 is a valid dice value")
    }

    pub fn nine() -> Self {
        Self::new(9).expect("9 is a valid dice value")
    }

    pub fn ten() -> Self {
        Self::new(10).expect("10 is a valid dice value")
    }

    pub fn eleven() -> Self {
        Self::new(11).expect("11 is a valid dice value")
    }

    pub fn twelve() -> Self {
        Self::new(12).expect("12 is a valid dice value")
    }

    pub fn max() -> Self {
        Self::twelve()
    }

    pub fn min() -> Self {
        Self::two()
    }

    pub fn list() -> impl Iterator<Item = DiceVal> {
        Self::ALL.into_iter()
    }

    pub fn prob_pts(&self) -> u8 {
        Self::D6_SIDES - i32::abs(self.0 as i32 - Self::SEVEN_VALUE as i32) as u8
    }

    pub fn resolve(self) -> DiceOutcome {
        match self.as_u8() {
            Self::SEVEN_VALUE => DiceOutcome::Seven,
            other => DiceOutcome::Harvest(
                TileNum::try_from(other).expect("non-seven dice roll should be a tile number"),
            ),
        }
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl Default for DiceRoll {
    fn default() -> Self {
        Self::seven()
    }
}

impl Serialize for DiceRoll {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
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
        let prob = self.prob_pts() as f32 / Self::PROBABILITY_DENOMINATOR;
        prob.try_into().expect("check math")
    }
}

impl TryFrom<u8> for DiceRoll {
    type Error = DiceRollError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match Self::new(value) {
            Some(x) => Ok(x),
            None => Err(DiceRollError::OutOfRange),
        }
    }
}

impl From<DiceRoll> for u8 {
    fn from(value: DiceRoll) -> u8 {
        value.0
    }
}

impl std::fmt::Display for DiceRollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl TileNum {
    pub const ALL: [TileNum; 10] = [
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(8),
        Self(9),
        Self(10),
        Self(11),
        Self(12),
    ];

    pub fn new(value: u8) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn iter() -> impl Iterator<Item = TileNum> {
        Self::ALL.into_iter()
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }

    pub fn prob_pts(&self) -> u8 {
        DiceRoll::from(*self).prob_pts()
    }
}

impl TryFrom<u8> for TileNum {
    type Error = TileNumError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            DiceRoll::SEVEN_VALUE => Err(TileNumError::Seven),
            DiceRoll::MIN_VALUE..=DiceRoll::MAX_VALUE => Ok(Self(value)),
            _ => Err(TileNumError::OutOfRange),
        }
    }
}

impl From<TileNum> for u8 {
    fn from(value: TileNum) -> Self {
        value.0
    }
}

impl From<TileNum> for DiceRoll {
    fn from(value: TileNum) -> Self {
        Self(value.0)
    }
}

impl Serialize for TileNum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::probability::{Sequence, Variant};

    #[test]
    fn dice_roll_resolves_seven_separately_from_harvest_numbers() {
        assert_eq!(DiceRoll::seven().resolve(), DiceOutcome::Seven);

        for value in [2, 3, 4, 5, 6, 8, 9, 10, 11, 12] {
            let roll = DiceRoll::try_from(value).unwrap();
            assert_eq!(
                roll.resolve(),
                DiceOutcome::Harvest(TileNum::try_from(value).unwrap())
            );
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
    fn complete_workflow() {
        // Create some dice values
        let low_values = vec![DiceVal::try_from(2).unwrap(), DiceVal::try_from(3).unwrap()];

        let high_values = vec![
            DiceVal::try_from(11).unwrap(),
            DiceVal::try_from(12).unwrap(),
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
