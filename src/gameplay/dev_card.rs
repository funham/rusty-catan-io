use std::ops::{Index, IndexMut};

use num::Integer;

pub enum DevCardStatus {
    NotReady,
    Ready,
    Played,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UsableDevCardKind {
    Knight,
    YearOfPlenty,
    RoadBuild,
    Monopoly,
}
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
    pub played: UsableDevCardCollection, // played cards
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

    pub fn move_to_played(
        &mut self,
        card: UsableDevCardKind,
    ) -> Result<(), DevCardDataPlayingError> {
        match self.active.contains(card) {
            true => Ok({
                self.active[card].dec();
                self.played[card].inc();
            }),
            false => Err(DevCardDataPlayingError),
        }
    }
}

pub struct OpponentDevCardData {
    pub(crate) queued: u16,
    pub(crate) active: u16,
    pub(crate) played: UsableDevCardCollection,
}

impl OpponentDevCardData {
    pub fn max_potential_vp(&self) -> u16 {
        self.queued + self.active
    }
}
