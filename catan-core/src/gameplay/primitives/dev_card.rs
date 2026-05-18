use std::{
    collections::BTreeMap,
    ops::{Add, AddAssign, Index, IndexMut},
};

use super::resource::Resource;
use crate::topology::{Hex, Path};
use num::Integer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UsableDevCard {
    Knight,
    YearOfPlenty,
    RoadBuild,
    Monopoly,
}

impl UsableDevCard {
    pub const ALL: [UsableDevCard; 4] = [
        Self::Knight,
        Self::YearOfPlenty,
        Self::RoadBuild,
        Self::Monopoly,
    ];

    pub fn iter() -> impl Iterator<Item = UsableDevCard> {
        Self::ALL.into_iter()
    }

    pub fn abbrev(&self) -> &'static str {
        match self {
            UsableDevCard::Knight => "KN",
            UsableDevCard::YearOfPlenty => "YP",
            UsableDevCard::RoadBuild => "RB",
            UsableDevCard::Monopoly => "M",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DevCardKind {
    Usable(UsableDevCard),
    VictoryPoint,
}

impl DevCardKind {
    pub const ALL: [DevCardKind; 5] = [
        DevCardKind::VictoryPoint,
        DevCardKind::Usable(UsableDevCard::Knight),
        DevCardKind::Usable(UsableDevCard::Monopoly),
        DevCardKind::Usable(UsableDevCard::YearOfPlenty),
        DevCardKind::Usable(UsableDevCard::RoadBuild),
    ];

    pub fn iter() -> impl Iterator<Item = DevCardKind> {
        Self::ALL.into_iter()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevCardMap<T> {
    pub usable: UsableDevCardMap<T>,
    pub victory_points: T,
}

pub type DevCardSet = DevCardMap<u16>;

impl DevCardSet {
    pub const ZERO: Self = Self {
        usable: UsableDevCardSet::ZERO,
        victory_points: 0,
    };

    pub const fn total(&self) -> u16 {
        self.usable.total() + self.victory_points
    }

    pub fn unroll(&self) -> impl Iterator<Item = (DevCardKind, u16)> {
        DevCardKind::iter().map(|card| (card, self[card]))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsableDevCardMap<T> {
    pub knight: T,
    pub year_of_plenty: T,
    pub road_build: T,
    pub monopoly: T,
}

pub type UsableDevCardSet = UsableDevCardMap<u16>;

impl UsableDevCardSet {
    pub const ZERO: Self = Self {
        knight: 0,
        year_of_plenty: 0,
        road_build: 0,
        monopoly: 0,
    };

    pub const fn total(&self) -> u16 {
        self.knight + self.year_of_plenty + self.road_build + self.monopoly
    }

    pub fn contains(&self, card: UsableDevCard) -> bool {
        self[card] > 0
    }

    pub fn unroll(&self) -> impl Iterator<Item = (UsableDevCard, u16)> {
        UsableDevCard::iter().map(|card| (card, self[card]))
    }
}

impl<T> Index<UsableDevCard> for UsableDevCardMap<T> {
    type Output = T;

    fn index(&self, kind: UsableDevCard) -> &Self::Output {
        match kind {
            UsableDevCard::Knight => &self.knight,
            UsableDevCard::YearOfPlenty => &self.year_of_plenty,
            UsableDevCard::RoadBuild => &self.road_build,
            UsableDevCard::Monopoly => &self.monopoly,
        }
    }
}

impl<T> IndexMut<UsableDevCard> for UsableDevCardMap<T> {
    fn index_mut(&mut self, kind: UsableDevCard) -> &mut Self::Output {
        match kind {
            UsableDevCard::Knight => &mut self.knight,
            UsableDevCard::YearOfPlenty => &mut self.year_of_plenty,
            UsableDevCard::RoadBuild => &mut self.road_build,
            UsableDevCard::Monopoly => &mut self.monopoly,
        }
    }
}

impl<T: Default + Copy> TryFrom<&[(UsableDevCard, T)]> for UsableDevCardMap<T> {
    type Error = DevCardCollectionError;

    fn try_from(flat_map: &[(UsableDevCard, T)]) -> Result<Self, Self::Error> {
        let mut this = Self::default();
        let mut seen = UsableDevCardMap::default();

        for (card, value) in flat_map {
            if seen[*card] {
                return Err(DevCardCollectionError::UsableDevCardAppearsTwice);
            }

            seen[*card] = true;
            this[*card] = *value;
        }

        Ok(this)
    }
}

impl<T> Index<DevCardKind> for DevCardMap<T> {
    type Output = T;

    fn index(&self, kind: DevCardKind) -> &Self::Output {
        match kind {
            DevCardKind::Usable(card) => &self.usable[card],
            DevCardKind::VictoryPoint => &self.victory_points,
        }
    }
}

impl<T> IndexMut<DevCardKind> for DevCardMap<T> {
    fn index_mut(&mut self, kind: DevCardKind) -> &mut Self::Output {
        match kind {
            DevCardKind::Usable(card) => &mut self.usable[card],
            DevCardKind::VictoryPoint => &mut self.victory_points,
        }
    }
}

impl<T: Default + Copy> TryFrom<&[(DevCardKind, T)]> for DevCardMap<T> {
    type Error = DevCardCollectionError;

    fn try_from(flat_map: &[(DevCardKind, T)]) -> Result<Self, Self::Error> {
        let mut this = Self::default();
        let mut seen = DevCardMap::default();

        for (card, value) in flat_map {
            if seen[*card] {
                return Err(DevCardCollectionError::DevCardAppearsTwice);
            }

            seen[*card] = true;
            this[*card] = *value;
        }

        Ok(this)
    }
}

impl Add for DevCardSet {
    type Output = DevCardSet;

    fn add(self, rhs: DevCardSet) -> Self::Output {
        self + &rhs
    }
}

impl Add<&DevCardSet> for DevCardSet {
    type Output = DevCardSet;

    fn add(self, rhs: &DevCardSet) -> Self::Output {
        DevCardSet {
            usable: self.usable + &rhs.usable,
            victory_points: self.victory_points + rhs.victory_points,
        }
    }
}

impl AddAssign for DevCardSet {
    fn add_assign(&mut self, rhs: DevCardSet) {
        *self += &rhs;
    }
}

impl AddAssign<&DevCardSet> for DevCardSet {
    fn add_assign(&mut self, rhs: &DevCardSet) {
        *self = *self + rhs;
    }
}

impl From<DevCardKind> for DevCardSet {
    fn from(card: DevCardKind) -> DevCardSet {
        let mut set = DevCardSet::default();
        set[card] = 1;
        set
    }
}

impl From<(DevCardKind, u16)> for DevCardSet {
    fn from((card, count): (DevCardKind, u16)) -> DevCardSet {
        let mut set = DevCardSet::default();
        set[card] = count;
        set
    }
}

impl From<BTreeMap<DevCardKind, u16>> for DevCardSet {
    fn from(value: BTreeMap<DevCardKind, u16>) -> Self {
        let x: Vec<_> = value.into_iter().collect();
        TryFrom::<&[(DevCardKind, u16)]>::try_from(x.as_slice()).unwrap()
    }
}

impl Add for UsableDevCardSet {
    type Output = UsableDevCardSet;

    fn add(self, rhs: UsableDevCardSet) -> Self::Output {
        self + &rhs
    }
}

impl Add<&UsableDevCardSet> for UsableDevCardSet {
    type Output = UsableDevCardSet;

    fn add(self, rhs: &UsableDevCardSet) -> Self::Output {
        UsableDevCardSet {
            knight: self.knight + rhs.knight,
            year_of_plenty: self.year_of_plenty + rhs.year_of_plenty,
            road_build: self.road_build + rhs.road_build,
            monopoly: self.monopoly + rhs.monopoly,
        }
    }
}

impl AddAssign for UsableDevCardSet {
    fn add_assign(&mut self, rhs: UsableDevCardSet) {
        *self += &rhs;
    }
}

impl AddAssign<&UsableDevCardSet> for UsableDevCardSet {
    fn add_assign(&mut self, rhs: &UsableDevCardSet) {
        *self = *self + rhs;
    }
}

impl From<UsableDevCard> for UsableDevCardSet {
    fn from(card: UsableDevCard) -> UsableDevCardSet {
        let mut set = UsableDevCardSet::default();
        set[card] = 1;
        set
    }
}

impl From<(UsableDevCard, u16)> for UsableDevCardSet {
    fn from((card, count): (UsableDevCard, u16)) -> UsableDevCardSet {
        let mut set = UsableDevCardSet::default();
        set[card] = count;
        set
    }
}

impl From<BTreeMap<UsableDevCard, u16>> for UsableDevCardSet {
    fn from(value: BTreeMap<UsableDevCard, u16>) -> Self {
        let x: Vec<_> = value.into_iter().collect();
        TryFrom::<&[(UsableDevCard, u16)]>::try_from(x.as_slice()).unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevCardCollectionError {
    DevCardAppearsTwice,
    UsableDevCardAppearsTwice,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DevCardData {
    pub queued: UsableDevCardSet, // unavailable in current round
    pub active: UsableDevCardSet, // ready to be played
    pub used: UsableDevCardSet,   // used cards
    pub victory_pts: u16,
}

impl std::fmt::Display for DevCardData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VP: {};", self.victory_pts)?;

        for x in UsableDevCard::ALL {
            write!(
                f,
                " {}: {}|{}|{};",
                x.abbrev(),
                self.used[x],
                self.active[x],
                self.queued[x]
            )?;
        }

        write!(f, " (used|active|queued)")
    }
}

#[derive(Debug)]
pub struct DevCardDataPlayingError;

impl DevCardData {
    pub fn reset_queue(&mut self) {
        for kind in UsableDevCard::iter() {
            self.active[kind] += self.queued[kind];
        }
        self.queued = UsableDevCardSet::default();
    }

    pub fn add(&mut self, card: DevCardKind) {
        match card {
            DevCardKind::Usable(usable_dev_card_kind) => {
                self.queued[usable_dev_card_kind].inc();
            }
            DevCardKind::VictoryPoint => {
                self.victory_pts.inc();
            }
        }
    }

    pub fn move_to_used(&mut self, card: UsableDevCard) -> Result<(), DevCardDataPlayingError> {
        match self.active.contains(card) {
            true => Ok({
                self.active[card].dec();
                self.used[card].inc();
            }),
            false => Err(DevCardDataPlayingError),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevCardUsage {
    Knight {
        rob_hex: Hex,
        robbed_id: Option<crate::gameplay::primitives::player::PlayerId>,
    },
    YearOfPlenty([Resource; 2]),
    RoadBuild([Path; 2]),
    Monopoly(Resource),
}

impl DevCardUsage {
    pub fn card_kind(&self) -> UsableDevCard {
        match self {
            DevCardUsage::Knight { .. } => UsableDevCard::Knight,
            DevCardUsage::YearOfPlenty(_) => UsableDevCard::YearOfPlenty,
            DevCardUsage::RoadBuild(_) => UsableDevCard::RoadBuild,
            DevCardUsage::Monopoly(_) => UsableDevCard::Monopoly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_dev_card_set_supports_named_fields_and_indexing() {
        let mut set = UsableDevCardSet {
            knight: 14,
            year_of_plenty: 2,
            road_build: 2,
            monopoly: 2,
        };

        assert_eq!(set[UsableDevCard::Knight], 14);
        assert_eq!(set[UsableDevCard::YearOfPlenty], 2);
        assert_eq!(set.total(), 20);

        set[UsableDevCard::Monopoly] += 1;

        assert_eq!(set.monopoly, 3);
        assert!(set.contains(UsableDevCard::Monopoly));
    }

    #[test]
    fn dev_card_set_unrolls_usable_and_victory_point_counts() {
        let set = DevCardSet {
            usable: UsableDevCardSet {
                knight: 14,
                year_of_plenty: 2,
                road_build: 2,
                monopoly: 2,
            },
            victory_points: 5,
        };

        let cards: Vec<_> = set.unroll().collect();

        assert_eq!(set.total(), 25);
        assert_eq!(
            cards,
            vec![
                (DevCardKind::VictoryPoint, 5),
                (DevCardKind::Usable(UsableDevCard::Knight), 14),
                (DevCardKind::Usable(UsableDevCard::Monopoly), 2),
                (DevCardKind::Usable(UsableDevCard::YearOfPlenty), 2),
                (DevCardKind::Usable(UsableDevCard::RoadBuild), 2),
            ]
        );
    }
}
