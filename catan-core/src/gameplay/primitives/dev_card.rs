use std::ops::{Index, IndexMut};

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
    const LIST: [UsableDevCard; 4] = [
        Self::Knight,
        Self::YearOfPlenty,
        Self::RoadBuild,
        Self::Monopoly,
    ];

    pub fn abbrev(&self) -> &'static str {
        match self {
            UsableDevCard::Knight => "KN",
            UsableDevCard::YearOfPlenty => "YOP",
            UsableDevCard::RoadBuild => "RB",
            UsableDevCard::Monopoly => "M",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DevCardKind {
    Usable(UsableDevCard),
    VictoryPoint,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DevCardCollection {
    usable: UsableDevCardCollection,
    victory_points: u8,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UsableDevCardCollection {
    data: [<UsableDevCardCollection as Index<UsableDevCard>>::Output; 4],
}

impl UsableDevCardCollection {
    pub fn total(&self) -> u16 {
        self.data.iter().sum()
    }

    pub fn contains(&self, card: UsableDevCard) -> bool {
        self[card] > 0
    }
}

impl Index<UsableDevCard> for UsableDevCardCollection {
    type Output = u16;

    fn index(&self, kind: UsableDevCard) -> &Self::Output {
        let index = match kind {
            UsableDevCard::Knight => 0,
            UsableDevCard::YearOfPlenty => 1,
            UsableDevCard::RoadBuild => 2,
            UsableDevCard::Monopoly => 3,
        };

        &self.data[index]
    }
}
impl IndexMut<UsableDevCard> for UsableDevCardCollection {
    fn index_mut(&mut self, kind: UsableDevCard) -> &mut Self::Output {
        let index = match kind {
            UsableDevCard::Knight => 0,
            UsableDevCard::YearOfPlenty => 1,
            UsableDevCard::RoadBuild => 2,
            UsableDevCard::Monopoly => 3,
        };

        &mut self.data[index]
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DevCardData {
    pub queued: UsableDevCardCollection, // unavailable in current round
    pub active: UsableDevCardCollection, // ready to be played
    pub used: UsableDevCardCollection,   // used cards
    pub victory_pts: u16,
}

impl std::fmt::Display for DevCardData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VP: {};", self.victory_pts)?;

        for x in UsableDevCard::LIST {
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

pub struct DevCardDataPlayingError;

impl DevCardData {
    pub fn reset_queue(&mut self) {
        for kind in [
            UsableDevCard::Knight,
            UsableDevCard::YearOfPlenty,
            UsableDevCard::RoadBuild,
            UsableDevCard::Monopoly,
        ] {
            self.active[kind] += self.queued[kind];
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
