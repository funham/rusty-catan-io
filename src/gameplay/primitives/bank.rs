use crate::gameplay::primitives::{
    dev_card::DevCardKind,
    player::PlayerId,
    resource::{Resource, ResourceCollection},
};

#[derive(Debug)]
pub struct Bank {
    pub resources: ResourceCollection,
    pub dev_cards: Vec<DevCardKind>,
}

impl Bank {
    pub fn view(&self) -> BankView {
        BankView { bank: self }
    }
}

#[derive(Debug)]
pub struct BankView<'a> {
    bank: &'a Bank,
}

impl<'a> BankView<'a> {
    pub fn fullness(&self, resource: Resource) -> DeckFullnessLevel {
        match DeckFullnessLevel::new(self.bank.resources[resource]) {
            Some(lvl) => lvl,
            None => {
                log::error!(
                    "too much cards in the bank: {}, where max is {}",
                    self.bank.resources[resource],
                    DeckFullnessLevel::High.max()
                );
                DeckFullnessLevel::High
            }
        }
    }

    pub fn dev_cards_fullness(&self) -> u16 {
        // Development Cards: The deck contains 25 cards:
        //  - 14 Knight Cards
        //  - 6 Progress Cards (2 of each type: Year of Plenty, Monopoly, Road Building)
        //  - 5 Victory Point Cards
        self.bank.dev_cards.len() as u16
    }
}

#[derive(Debug)]
pub enum DeckFullnessLevel {
    Empty,
    Low,
    Medium,
    High,
}

impl DeckFullnessLevel {
    // none if n > max possible amount of cards of one resource
    pub fn new(n: u16) -> Option<Self> {
        for lvl in [Self::Empty, Self::Low, Self::High, Self::High] {
            if lvl.range().contains(&n) {
                return Some(lvl);
            }
        }

        None
    }

    pub fn new_or_panic(n: u16) -> Self {
        for lvl in [Self::Empty, Self::Low, Self::High, Self::High] {
            if n <= lvl.max() {
                return lvl;
            }
        }

        unreachable!("too much cards")
    }

    /// min possible number of cards that a deck with this level can contain
    pub fn min(&self) -> u16 {
        match self {
            DeckFullnessLevel::Empty => 0,
            DeckFullnessLevel::Low => DeckFullnessLevel::Empty.max() + 1,
            DeckFullnessLevel::Medium => DeckFullnessLevel::Low.max() + 1,
            DeckFullnessLevel::High => DeckFullnessLevel::Medium.max() + 1,
        }
    }

    /// max possible number of cards that a deck with this level can contain
    pub fn max(&self) -> u16 {
        match self {
            DeckFullnessLevel::Empty => 0,
            DeckFullnessLevel::Low => 7,
            DeckFullnessLevel::Medium => 13,
            DeckFullnessLevel::High => 19,
        }
    }

    /// possible range in which number of cards can a deck with this level can contain
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
