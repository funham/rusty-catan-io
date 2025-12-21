use std::ops::{Index, IndexMut};

use super::{Robbery, resource::Resource};
use crate::topology::Path;
use num::Integer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UsableDevCardKind {
    Knight,
    YearOfPlenty,
    RoadBuild,
    Monopoly,
}

#[derive(Debug)]
pub enum DevCardKind {
    Usable(UsableDevCardKind),
    VictoryPoint,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct UsableDevCardCollection {
    data: [<UsableDevCardCollection as Index<UsableDevCardKind>>::Output; 4],
}

impl UsableDevCardCollection {
    pub fn total(&self) -> u16 {
        self.data.iter().sum()
    }

    pub fn contains(&self, card: UsableDevCardKind) -> bool {
        self[card] > 0
    }
}

impl Index<UsableDevCardKind> for UsableDevCardCollection {
    type Output = u16;

    fn index(&self, kind: UsableDevCardKind) -> &Self::Output {
        let index = match kind {
            UsableDevCardKind::Knight => 0,
            UsableDevCardKind::YearOfPlenty => 1,
            UsableDevCardKind::RoadBuild => 2,
            UsableDevCardKind::Monopoly => 3,
        };

        &self.data[index]
    }
}
impl IndexMut<UsableDevCardKind> for UsableDevCardCollection {
    fn index_mut(&mut self, kind: UsableDevCardKind) -> &mut Self::Output {
        let index = match kind {
            UsableDevCardKind::Knight => 0,
            UsableDevCardKind::YearOfPlenty => 1,
            UsableDevCardKind::RoadBuild => 2,
            UsableDevCardKind::Monopoly => 3,
        };

        &mut self.data[index]
    }
}

#[derive(Debug, Default, Clone)]
pub struct DevCardData {
    pub queued: UsableDevCardCollection, // unavailable in current round
    pub active: UsableDevCardCollection, // ready to be played
    pub used: UsableDevCardCollection,   // used cards
    pub victory_pts: u16,
}

pub struct DevCardDataPlayingError;

impl DevCardData {
    pub fn reset_queue(&mut self) {
        // self.active.add(self.queued);
        self.queued = UsableDevCardCollection::default();
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

    pub fn move_to_used(&mut self, card: UsableDevCardKind) -> Result<(), DevCardDataPlayingError> {
        match self.active.contains(card) {
            true => Ok({
                self.active[card].dec();
                self.used[card].inc();
            }),
            false => Err(DevCardDataPlayingError),
        }
    }
}

pub struct SecuredDevCardData {
    pub(crate) queued: u16,
    pub(crate) active: u16,
    pub(crate) played: UsableDevCardCollection,
}

impl SecuredDevCardData {
    pub fn max_potential_vp(&self) -> u16 {
        self.queued + self.active
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DevCardUsage {
    Knight(Robbery),
    YearOfPlenty([Resource; 2]),
    RoadBuild([Path; 2]),
    Monopoly(Resource),
}

impl DevCardUsage {
    pub fn card_kind(&self) -> UsableDevCardKind {
        match self {
            DevCardUsage::Knight(_) => UsableDevCardKind::Knight,
            DevCardUsage::YearOfPlenty(_) => UsableDevCardKind::YearOfPlenty,
            DevCardUsage::RoadBuild(_) => UsableDevCardKind::YearOfPlenty,
            DevCardUsage::Monopoly(_) => UsableDevCardKind::Monopoly,
        }
    }
}
