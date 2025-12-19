use std::fmt::Debug;

use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::math::probability::{Probability, Probable};

/// Value that can be produced by rolling two D6's
/// `DiceVal \in [2..12]` (11 possible states)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiceVal(u8);

impl DiceVal {
    pub unsafe fn new_uncheked(val: u8) -> Self {
        Self { 0: val }
    }

    pub fn new(value: u8) -> Option<Self> {
        if (2..=12).contains(&value) {
            Some(Self { 0: value })
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
}

impl Probable for DiceVal {
    fn prob(&self) -> Probability {
        let val = Into::<u8>::into(*self) as i32;
        let ncomb = 6 - i32::abs(val - 7);
        let prob = ncomb as f32 / 36.0;

        prob.try_into().expect("check math")
    }
}

impl TryFrom<u8> for DiceVal {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match Self::new(value) {
            Some(x) => Ok(x),
            None => Err(()),
        }
    }
}

impl Into<u8> for DiceVal {
    fn into(self) -> u8 {
        self.0
    }
}

pub trait DiceRoller: core::fmt::Debug {
    fn roll(&mut self) -> DiceVal;
}

#[derive(Debug)]
pub struct RandomDiceRoller {
    rng: SmallRng,
}

impl RandomDiceRoller {
    pub fn new() -> Self {
        Self {
            rng: SmallRng::from_rng(&mut rand::rng()),
        }
    }
}

impl DiceRoller for RandomDiceRoller {
    fn roll(&mut self) -> DiceVal {
        (self.rng.random_range(1..=6) + self.rng.random_range(1..=6))
            .try_into()
            .unwrap()
    }
}

pub struct ConsoleDiceRoller {
    stream: Box<dyn std::io::BufRead>,
}

impl Debug for ConsoleDiceRoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleDiceRoller").finish()
    }
}

impl DiceRoller for ConsoleDiceRoller {
    fn roll(&mut self) -> DiceVal {
        for _ in 0..10 {
            let mut input_line = String::new();
            self.stream
                .read_line(&mut input_line)
                .expect("IO error: Failed to read line");

            if let Ok(x) = input_line.trim().parse::<u8>() {
                if let Ok(dv) = DiceVal::try_from(x) {
                    return dv;
                } else {
                    println!(
                        "Typed {} is not in (2..=12) range and hence is not a valid dice value",
                        x
                    );
                }
            } else {
                println!("Typed \"{}\" is not a valid unsigned integer", &input_line);
            }
        }

        panic!("Type an actual integer next time, you bitch!")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::probability::{Sequence, Variant};
    use rand::SeedableRng;
    use std::io::{BufReader, Cursor};

    // ===== DiceRoller Tests =====
    #[test]
    fn random_dice_roller_distribution() {
        let mut roller = RandomDiceRoller::new();
        let mut counts = [0u32; 13]; // indices 0-12, we'll use 2-12

        // Roll many times and check distribution
        for _ in 0..10000 {
            let val = roller.roll();
            counts[val.0 as usize] += 1;
        }

        // Check all values in range are generated
        for i in 2..=12 {
            assert!(counts[i] > 0, "Value {} never rolled", i);
        }

        // Check 7 is most common (statistical test)
        assert!(
            counts[7] > counts[2] && counts[7] > counts[12],
            "7 should be more common than extremes"
        );
    }

    #[test]
    fn random_dice_roller_deterministic_with_seed() {
        // Create deterministic RNG
        let seed = [42; 32];
        let rng = SmallRng::from_seed(seed);
        let mut roller = RandomDiceRoller { rng };

        // Get sequence of rolls
        let rolls: Vec<u8> = (0..10).map(|_| roller.roll().into()).collect();

        // Create another roller with same seed
        let rng2 = SmallRng::from_seed(seed);
        let mut roller2 = RandomDiceRoller { rng: rng2 };
        let rolls2: Vec<u8> = (0..10).map(|_| roller2.roll().into()).collect();

        assert_eq!(rolls, rolls2, "Same seed should produce same sequence");
    }

    #[test]
    fn console_dice_roller_valid_input() {
        let input = "7\n"; // Simulate user typing "7"
        let cursor = Cursor::new(input);
        let reader = BufReader::new(cursor);

        let mut roller = ConsoleDiceRoller {
            stream: Box::new(reader),
        };

        let result = roller.roll();
        assert_eq!(result.0, 7);
    }

    #[test]
    fn console_dice_roller_retry_on_invalid() {
        // Test with invalid input followed by valid input
        let input = "invalid\n15\n8\n";
        let cursor = Cursor::new(input);
        let reader = BufReader::new(cursor);

        let mut roller = ConsoleDiceRoller {
            stream: Box::new(reader),
        };

        let result = roller.roll();
        assert_eq!(result.0, 8);
    }

    #[test]
    #[should_panic(expected = "Type an actual integer next time")]
    fn console_dice_roller_panic_after_max_attempts() {
        // 10 lines of invalid input
        let input = "invalid\n".repeat(10);
        let cursor = Cursor::new(input);
        let reader = BufReader::new(cursor);

        let mut roller = ConsoleDiceRoller {
            stream: Box::new(reader),
        };

        roller.roll(); // Should panic
    }

    // ===== Integration Tests =====
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
