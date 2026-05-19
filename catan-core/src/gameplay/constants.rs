use crate::gameplay::primitives::{
    dev_card::{DevCardSet, UsableDevCardSet},
    resource::ResourceSet,
};

pub mod costs {
    use super::*;

    pub const ROAD: ResourceSet = ResourceSet {
        brick: 1,
        wood: 1,
        ..ResourceSet::EMPTY
    };
    pub const SETTLEMENT: ResourceSet = ResourceSet {
        brick: 1,
        wood: 1,
        wheat: 1,
        sheep: 1,
        ..ResourceSet::EMPTY
    };
    pub const CITY: ResourceSet = ResourceSet {
        ore: 3,
        wheat: 2,
        ..ResourceSet::EMPTY
    };
    pub const DEV_CARD: ResourceSet = ResourceSet {
        wheat: 1,
        sheep: 1,
        ore: 1,
        ..ResourceSet::EMPTY
    };
}

pub mod bank {
    use super::*;

    pub const DEFAULT_RESOURCES: ResourceSet = ResourceSet {
        brick: 19,
        wood: 19,
        wheat: 19,
        sheep: 19,
        ore: 19,
    };

    pub const DEFAULT_DEV_CARDS: DevCardSet = DevCardSet {
        usable: UsableDevCardSet {
            knight: 14,
            year_of_plenty: 2,
            road_build: 2,
            monopoly: 2,
        },
        victory_points: 5,
    };
}

pub mod vp {
    pub const LONGEST_ROAD_VP: u16 = 2;
    pub const LARGEST_ARMY_VP: u16 = 2;
    pub const VP_TO_WIN: u16 = 10;
    pub const SETTLEMENT_VP: u16 = 1;
    pub const CITY_VP: u16 = 2;
}

pub mod capacities {
    pub const PLAYER_VIEW_INLINE: usize = 8;
    pub const PLAYER_PORTS_INLINE: usize = 12;
    pub const PLAYER_ESTABLISHMENTS_INLINE: usize = 12;
    pub const PLAYER_ROADS_INLINE: usize = 30;
    pub const EVENT_BATCH_INLINE: usize = 32;
    pub const RESOURCE_DISTRIBUTION_INLINE: usize = 8;
    pub const EVENT_RECIPIENTS_INLINE: usize = 2;
    pub const GAME_END_STATS_INLINE: usize = 8;
}
