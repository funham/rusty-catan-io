use std::fmt;

use rand::{Rng, RngExt, SeedableRng, rngs::SmallRng, seq::SliceRandom};

use crate::{
    common::SmallSet,
    constants,
    gameplay::primitives::{
        dev_card::DevCardKind,
        resource::{Resource, ResourceSet},
    },
    math::dice::DiceRoll,
};

type EntropySource = Box<dyn Rng>;

pub trait DiceProvider: fmt::Debug {
    fn roll(&mut self, rng: &mut dyn Rng) -> DiceRoll;
}

pub trait ResourcePicker: fmt::Debug {
    fn pick(&mut self, resources: &ResourceSet, rng: &mut dyn Rng) -> Option<Resource>;
}

pub trait DevCardShuffler: fmt::Debug {
    fn shuffle(&mut self, deck: &mut [DevCardKind], rng: &mut dyn Rng);
}

pub struct GameRandom {
    rng: EntropySource,
    dice: Box<dyn DiceProvider>,
    resource_picker: Box<dyn ResourcePicker>,
    dev_cards: Box<dyn DevCardShuffler>,
}

impl GameRandom {
    pub fn thread() -> Self {
        Self::from_rng(rand::rng())
    }

    pub fn from_rng<R>(rng: R) -> Self
    where
        R: Rng + 'static,
    {
        GameRandomBuilder::from_rng(rng).build()
    }

    pub fn seeded(seed: u64) -> Self {
        Self::from_rng(SmallRng::seed_from_u64(seed))
    }

    pub fn roll_dice(&mut self) -> DiceRoll {
        self.dice.roll(self.rng.as_mut())
    }

    pub fn pick_resource(&mut self, resources: &ResourceSet) -> Option<Resource> {
        self.resource_picker.pick(resources, self.rng.as_mut())
    }

    pub fn shuffle_dev_cards(&mut self, deck: &mut [DevCardKind]) {
        self.dev_cards.shuffle(deck, self.rng.as_mut());
    }
}

impl Default for GameRandom {
    fn default() -> Self {
        Self::thread()
    }
}

impl fmt::Debug for GameRandom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GameRandom")
            .field("rng", &"<entropy source>")
            .field("dice", &self.dice)
            .field("resource_picker", &self.resource_picker)
            .field("dev_cards", &self.dev_cards)
            .finish()
    }
}

pub struct GameRandomBuilder {
    rng: EntropySource,
    dice: Box<dyn DiceProvider>,
    resource_picker: Box<dyn ResourcePicker>,
    dev_cards: Box<dyn DevCardShuffler>,
}

impl GameRandomBuilder {
    pub fn new() -> Self {
        Self::from_rng(rand::rng())
    }

    pub fn from_rng<R>(rng: R) -> Self
    where
        R: Rng + 'static,
    {
        Self {
            rng: Box::new(rng),
            dice: Box::new(TwoD6Dice),
            resource_picker: Box::new(WeightedResourcePicker),
            dev_cards: Box::new(DefaultShuffler),
        }
    }

    pub fn dice(mut self, dice: impl DiceProvider + 'static) -> Self {
        self.dice = Box::new(dice);
        self
    }

    pub fn resource_picker(mut self, picker: impl ResourcePicker + 'static) -> Self {
        self.resource_picker = Box::new(picker);
        self
    }

    pub fn dev_cards(mut self, randomizer: impl DevCardShuffler + 'static) -> Self {
        self.dev_cards = Box::new(randomizer);
        self
    }

    pub fn build(self) -> GameRandom {
        GameRandom {
            rng: self.rng,
            dice: self.dice,
            resource_picker: self.resource_picker,
            dev_cards: self.dev_cards,
        }
    }
}

impl Default for GameRandomBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct TwoD6Dice;

impl DiceProvider for TwoD6Dice {
    fn roll(&mut self, rng: &mut dyn Rng) -> DiceRoll {
        (rng.random_range(1..=DiceRoll::D6_SIDES) + rng.random_range(1..=DiceRoll::D6_SIDES))
            .try_into()
            .expect("two d6 rolls should always produce a valid dice roll")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedDice {
    roll: DiceRoll,
}

impl FixedDice {
    pub fn new(roll: DiceRoll) -> Self {
        Self { roll }
    }
}

impl DiceProvider for FixedDice {
    fn roll(&mut self, _rng: &mut dyn Rng) -> DiceRoll {
        self.roll
    }
}

#[derive(Debug, Default)]
pub struct WeightedResourcePicker;

impl ResourcePicker for WeightedResourcePicker {
    fn pick(&mut self, resources: &ResourceSet, rng: &mut dyn Rng) -> Option<Resource> {
        if resources.is_empty() {
            return None;
        }

        resource_at_offset(resources, rng.random_range(0..resources.total()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PreferredResourcePicker {
    preferred: Resource,
}

impl PreferredResourcePicker {
    pub fn new(preferred: Resource) -> Self {
        Self { preferred }
    }
}

impl ResourcePicker for PreferredResourcePicker {
    fn pick(&mut self, resources: &ResourceSet, rng: &mut dyn Rng) -> Option<Resource> {
        if resources[self.preferred] > 0 {
            return Some(self.preferred);
        }

        WeightedResourcePicker.pick(resources, rng)
    }
}

#[derive(Debug, Default)]
pub struct DefaultShuffler;

impl DevCardShuffler for DefaultShuffler {
    fn shuffle(&mut self, deck: &mut [DevCardKind], rng: &mut dyn Rng) {
        deck.shuffle(rng);
    }
}

#[derive(Debug, Default)]
pub struct NoOpShuffler;

impl DevCardShuffler for NoOpShuffler {
    fn shuffle(&mut self, _deck: &mut [DevCardKind], _rng: &mut dyn Rng) {}
}

#[derive(Debug, Default)]
pub struct FixedOrderShuffler {
    desired_order: Vec<DevCardKind>,
}

impl DevCardShuffler for FixedOrderShuffler {
    fn shuffle(&mut self, deck: &mut [DevCardKind], _rng: &mut dyn Rng) {
        /* validation(comment out if not needed) */
        {
            let reference = deck
                .iter()
                .cloned()
                .collect::<SmallSet<_, { constants::bank::DEFAULT_DEV_CARDS.total() as usize }>>();

            let actual = self
                .desired_order
                .iter()
                .cloned()
                .collect::<SmallSet<_, { constants::bank::DEFAULT_DEV_CARDS.total() as usize }>>();

            assert_eq!(
                actual, reference,
                "dev card set must match the canonical one"
            );
        }

        deck.copy_from_slice(&self.desired_order);
    }
}

fn resource_at_offset(resources: &ResourceSet, offset: u16) -> Option<Resource> {
    let mut seen = 0;

    for (resource, count) in resources.unroll() {
        seen += count;
        if offset < seen {
            return Some(resource);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gameplay::primitives::{
            bank::Bank,
            dev_card::{DevCardKind, UsableDevCard},
            resource::{Resource, ResourceSet},
        },
        math::dice::DiceRoll,
    };
    use rand::{SeedableRng, rngs::SmallRng};

    #[test]
    fn seeded_game_random_repeats_all_default_component_outputs() {
        let mut first = GameRandom::seeded(42);
        let mut second = GameRandom::seeded(42);

        let resources = ResourceSet {
            brick: 2,
            wood: 1,
            wheat: 3,
            sheep: 0,
            ore: 1,
        };
        let mut first_bank = Bank::default();
        let mut second_bank = Bank::default();

        assert_eq!(first.roll_dice(), second.roll_dice());
        assert_eq!(
            first.pick_resource(&resources),
            second.pick_resource(&resources)
        );

        first.shuffle_dev_cards(&mut first_bank.dev_cards);
        second.shuffle_dev_cards(&mut second_bank.dev_cards);
        assert_eq!(first_bank.dev_cards, second_bank.dev_cards);
    }

    #[test]
    fn from_rng_uses_one_shared_entropy_stream_for_default_components() {
        let seed = 77;
        let resources = ResourceSet {
            brick: 1,
            wood: 2,
            wheat: 3,
            sheep: 4,
            ore: 5,
        };

        let mut expected_rng = SmallRng::seed_from_u64(seed);
        let mut expected_dice = TwoD6Dice;
        let mut expected_picker = WeightedResourcePicker;
        let expected_roll = expected_dice.roll(&mut expected_rng);
        let expected_resource = expected_picker.pick(&resources, &mut expected_rng);

        let mut random = GameRandom::from_rng(SmallRng::seed_from_u64(seed));

        assert_eq!(random.roll_dice(), expected_roll);
        assert_eq!(random.pick_resource(&resources), expected_resource);
    }

    #[test]
    fn builder_accepts_fixed_dice_provider() {
        let mut random = GameRandomBuilder::from_rng(SmallRng::seed_from_u64(1))
            .dice(FixedDice::new(DiceRoll::six()))
            .build();

        for _ in 0..10 {
            assert_eq!(random.roll_dice(), DiceRoll::six());
        }
    }

    #[test]
    fn builder_accepts_resource_picker_that_prefers_a_specific_resource() {
        let mut random = GameRandomBuilder::from_rng(SmallRng::seed_from_u64(1))
            .resource_picker(PreferredResourcePicker::new(Resource::Ore))
            .build();

        let resources = ResourceSet {
            brick: 4,
            wood: 4,
            wheat: 4,
            sheep: 4,
            ore: 1,
        };

        assert_eq!(random.pick_resource(&resources), Some(Resource::Ore));
    }

    #[test]
    fn builder_accepts_custom_dev_card_randomizer() {
        #[derive(Debug)]
        struct ReverseDeck;

        impl DevCardShuffler for ReverseDeck {
            fn shuffle(&mut self, deck: &mut [DevCardKind], _rng: &mut dyn rand::Rng) {
                deck.reverse();
            }
        }

        let mut random = GameRandomBuilder::new().dev_cards(ReverseDeck).build();
        let mut deck = vec![
            DevCardKind::VictoryPoint,
            DevCardKind::Usable(UsableDevCard::Knight),
            DevCardKind::Usable(UsableDevCard::Monopoly),
        ];

        random.shuffle_dev_cards(&mut deck);

        assert_eq!(
            deck,
            vec![
                DevCardKind::Usable(UsableDevCard::Monopoly),
                DevCardKind::Usable(UsableDevCard::Knight),
                DevCardKind::VictoryPoint,
            ]
        );
    }
}
