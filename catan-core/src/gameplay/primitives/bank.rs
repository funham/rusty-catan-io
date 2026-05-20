use serde::{Deserialize, Serialize};

use crate::{
    constants,
    gameplay::primitives::{
        dev_card::DevCardKind,
        player::PlayerId,
        resource::{Resource, ResourceMap, ResourceSet},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bank {
    pub resources: ResourceSet,
    pub dev_cards: Vec<DevCardKind>,
}

impl Bank {
    pub fn can_pay(&self, resources: &ResourceSet) -> bool {
        self.resources.has_enough(resources)
    }

    pub fn deposit(&mut self, resources: ResourceSet) {
        self.resources += resources;
    }

    pub fn withdraw(&mut self, resources: ResourceSet) -> Result<(), BankResourceExchangeError> {
        self.resources
            .subtract_in_place(&resources)
            .map_err(|_| BankResourceExchangeError::BankIsShort)
    }

    pub fn draw_dev_card(&mut self) -> Option<DevCardKind> {
        self.dev_cards.pop()
    }

    pub fn public_view(&self) -> BankViewOwned {
        let mut resources = ResourceMap {
            brick: DeckFullnessLevel::Empty,
            wood: DeckFullnessLevel::Empty,
            wheat: DeckFullnessLevel::Empty,
            sheep: DeckFullnessLevel::Empty,
            ore: DeckFullnessLevel::Empty,
        };
        for resource in Resource::iter() {
            resources[resource] = DeckFullnessLevel::new_or_panic(self.resources[resource]);
        }

        BankViewOwned {
            resources,
            dev_cards: DeckFullnessLevel::dev_card_deck(self.dev_cards.len() as u16),
        }
    }
}

impl Default for Bank {
    fn default() -> Self {
        let resources = constants::bank::DEFAULT_RESOURCES;

        let dev_cards = constants::bank::DEFAULT_DEV_CARDS
            .unroll()
            .flat_map(|(card, count)| std::iter::repeat_n(card, count as usize))
            .collect();

        Self {
            resources,
            dev_cards,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankViewOwned {
    pub resources: ResourceMap<DeckFullnessLevel>,
    pub dev_cards: DeckFullnessLevel,
}

impl BankViewOwned {
    pub fn fullness(&self, resource: Resource) -> DeckFullnessLevel {
        self.resources[resource]
    }

    pub fn dev_cards_fullness(&self) -> DeckFullnessLevel {
        self.dev_cards
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeckFullnessLevel {
    Empty,
    Low,
    Medium,
    High,
}

impl DeckFullnessLevel {
    pub fn dev_card_deck(count: u16) -> Self {
        match count {
            0 => Self::Empty,
            1..=7 => Self::Low,
            8..=13 => Self::Medium,
            _ => Self::High,
        }
    }

    pub fn new(n: u16) -> Option<Self> {
        [Self::Empty, Self::Low, Self::Medium, Self::High]
            .into_iter()
            .find(|lvl| lvl.range().contains(&n))
    }

    pub fn new_or_panic(n: u16) -> Self {
        Self::new(n).unwrap_or_else(|| panic!("too much cards in a resource deck: {n}"))
    }

    pub fn min(&self) -> u16 {
        match self {
            DeckFullnessLevel::Empty => 0,
            DeckFullnessLevel::Low => 1,
            DeckFullnessLevel::Medium => 8,
            DeckFullnessLevel::High => 14,
        }
    }

    pub fn max(&self) -> u16 {
        match self {
            DeckFullnessLevel::Empty => 0,
            DeckFullnessLevel::Low => 7,
            DeckFullnessLevel::Medium => 13,
            DeckFullnessLevel::High => 19,
        }
    }

    pub fn range(&self) -> std::ops::RangeInclusive<u16> {
        self.min()..=self.max()
    }
}

#[derive(Debug)]
pub enum BankResourceExchangeError {
    BankIsShort,
    AccountIsShort {
        account: PlayerId,
        short: ResourceSet,
    },
}

#[derive(Debug)]
pub enum PlayerResourceExchangeError {
    AccountIsShort { id: PlayerId },
}

#[cfg(test)]
mod tests {
    use super::Bank;
    use crate::gameplay::random::GameRandom;

    #[test]
    fn seeded_dev_card_shuffle_is_reproducible() {
        let mut first = Bank::default();
        let mut second = Bank::default();
        let mut different = Bank::default();
        let mut first_random = GameRandom::seeded(42);
        let mut second_random = GameRandom::seeded(42);
        let mut different_random = GameRandom::seeded(43);

        first_random.shuffle_dev_cards(&mut first.dev_cards);
        second_random.shuffle_dev_cards(&mut second.dev_cards);
        different_random.shuffle_dev_cards(&mut different.dev_cards);

        assert_eq!(first.dev_cards, second.dev_cards);
        assert_ne!(first.dev_cards, different.dev_cards);
    }
}
