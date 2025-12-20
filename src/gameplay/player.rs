use std::collections::BTreeSet;

use crate::gameplay::dev_card::{DevCardData, OpponentDevCardData};
use crate::gameplay::primitives::{City, Road, Settlement};
use crate::gameplay::resource::{HasCost, ResourceCollection};
use crate::topology::*;

pub type PlayerId = usize;

#[derive(Debug)]
pub struct PlayerBuildData {
    pub settlements: BTreeSet<Settlement>,
    pub cities: BTreeSet<City>,
    pub roads: graph::RoadGraph, // derives default
}

impl PlayerBuildData {
    pub fn builds_occupancy(&self) -> BTreeSet<Intersection> {
        self.settlements
            .iter()
            .map(|s| s.pos)
            .chain(self.cities.iter().map(|c| c.pos))
            .collect()
    }

    pub fn roads_occupancy(&self) -> BTreeSet<Path> {
        self.roads.roads_occupancy().clone()
    }
}

#[derive(Debug)]
pub struct PlayerData {
    pub resources: ResourceCollection,
    pub dev_cards: DevCardData,
}

pub struct OpponentData {
    pub dev_cards: OpponentDevCardData,
}

impl From<&PlayerData> for OpponentData {
    fn from(player_data: &PlayerData) -> Self {
        Self {
            dev_cards: OpponentDevCardData {
                queued: player_data.dev_cards.queued.total(),
                active: player_data.dev_cards.active.total(),
                played: player_data.dev_cards.played.clone(),
            },
        }
    }
}
