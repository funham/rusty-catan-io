use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::{
    dev_card::{DevCardKind, UsableDevCard},
    player::PlayerId,
    resource::{Resource, ResourceCollection, ResourceMap},
};

#[derive(Debug, Clone)]
pub struct Bank {
    pub resources: ResourceCollection,
    pub dev_cards: Vec<DevCardKind>,
}

impl Bank {
    pub fn can_pay(&self, resources: &ResourceCollection) -> bool {
        self.resources.has_enough(resources)
    }

    pub fn deposit(&mut self, resources: ResourceCollection) {
        self.resources += &resources;
    }

    pub fn withdraw(
        &mut self,
        resources: ResourceCollection,
    ) -> Result<(), BankResourceExchangeError> {
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
        for resource in Resource::list() {
            resources[resource] = DeckFullnessLevel::new_or_panic(self.resources[resource]);
        }

        BankViewOwned {
            resources,
            dev_card_count: self.dev_cards.len() as u16,
        }
    }
}

impl Default for Bank {
    fn default() -> Self {
        let resources = ResourceCollection {
            brick: 19,
            wood: 19,
            wheat: 19,
            sheep: 19,
            ore: 19,
        };

        let mut dev_cards = Vec::new();

        dev_cards.extend([DevCardKind::VictoryPoint; 5]);
        dev_cards.extend([DevCardKind::Usable(UsableDevCard::Knight); 14]);
        dev_cards.extend([DevCardKind::Usable(UsableDevCard::Monopoly); 2]);
        dev_cards.extend([DevCardKind::Usable(UsableDevCard::YearOfPlenty); 2]);
        dev_cards.extend([DevCardKind::Usable(UsableDevCard::RoadBuild); 2]);

        Self {
            resources,
            dev_cards,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankViewOwned {
    pub resources: ResourceMap<DeckFullnessLevel>,
    pub dev_card_count: u16,
}

impl BankViewOwned {
    pub fn fullness(&self, resource: Resource) -> DeckFullnessLevel {
        self.resources[resource]
    }

    pub fn dev_cards_fullness(&self) -> u16 {
        self.dev_card_count
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
    AccountIsShort { id: PlayerId },
}

#[derive(Debug)]
pub enum PlayerResourceExchangeError {
    AccountIsShort { id: PlayerId },
}
