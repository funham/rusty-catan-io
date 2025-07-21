use std::{io::stdin, rc::Rc};

use rand::{Rng, SeedableRng, rngs::SmallRng};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Probability(f32);

impl TryFrom<f32> for Probability {
    type Error = ();
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if 0.0 < value || value > 1.0 {
            Err(())
        } else {
            Ok(Self { 0: value })
        }
    }
}

impl Into<f32> for Probability {
    fn into(self) -> f32 {
        self.0
    }
}

impl Probability {
    pub fn zero() -> Self {
        Self::try_from(0.0).unwrap()
    }

    pub fn one() -> Self {
        Self::try_from(1.0).unwrap()
    }

    pub fn half() -> Self {
        Self::try_from(0.5).unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiceVal(pub u8);

impl TryFrom<u8> for DiceVal {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (2..=12).contains(&value) {
            Ok(Self { 0: value })
        } else {
            Err(())
        }
    }
}

impl Into<u8> for DiceVal {
    fn into(self) -> u8 {
        self.0
    }
}

pub trait DiceRoller {
    fn roll(&mut self) -> DiceVal;
    fn pdf(&self, val: DiceVal) -> Probability;
}

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

    fn pdf(&self, val: DiceVal) -> Probability {
        ((6 - (val.0 as i32 - 7)) as f32 / 36.0)
            .try_into()
            .unwrap_or(Probability::zero())
    }
}

pub struct ConsoleDiceRoller {
    stream: Box<dyn std::io::BufRead>,
}

impl DiceRoller for ConsoleDiceRoller {
    fn roll(&mut self) -> DiceVal {
        for _ in (0..10) {
            let mut input_line = String::new();
            self.stream
                .read_line(&mut input_line)
                .expect("Failed to read line");

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

        panic!("Type an actual integer, you bitch!")
    }
    fn pdf(&self, val: DiceVal) -> Probability {
        ((6 - (val.0 as i32 - 7)) as f32 / 36.0)
            .try_into()
            .unwrap_or(Probability::zero())
    }
}

pub struct Dice {
    roller: Box<dyn DiceRoller>,
    state: DiceVal,
}

impl Dice {
    pub fn new(roller: Box<dyn DiceRoller>) -> Self {
        Self {
            roller,
            state: DiceVal::try_from(0).unwrap(),
        }
    }

    pub fn roll(&mut self) -> DiceVal {
        self.state = self.roller.roll();
        self.state
    }

    pub fn state(&self) -> DiceVal {
        self.state
    }

    pub fn pdf(&self, val: DiceVal) -> Probability {
        self.roller.pdf(val)
    }
}

impl Default for Dice {
    fn default() -> Self {
        Dice::new(Box::new(RandomDiceRoller::new()))
    }
}
