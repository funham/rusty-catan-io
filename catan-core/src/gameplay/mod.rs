pub mod agent;
pub mod field;
pub mod game;
pub mod primitives;
pub mod random;

pub mod constants {
    use crate::gameplay::primitives::resource::ResourceCollection;

    pub mod capacities {
        pub const PLAYER_VIEW_INLINE: usize = 8;
        pub const PLAYER_PORTS_INLINE: usize = 12;
        pub const PLAYER_ESTABLISHMENTS_INLINE: usize = 12;
        pub const PLAYER_ROADS_INLINE: usize = 30;
    }

    pub mod costs {
        use super::*;

        pub const ROAD: ResourceCollection = ResourceCollection {
            brick: 1,
            wood: 1,
            ..ResourceCollection::ZERO
        };
        pub const SETTLEMENT: ResourceCollection = ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 1,
            sheep: 1,
            ..ResourceCollection::ZERO
        };
        pub const CITY: ResourceCollection = ResourceCollection {
            ore: 3,
            wheat: 2,
            ..ResourceCollection::ZERO
        };
        pub const DEV_CARD: ResourceCollection = ResourceCollection {
            wheat: 1,
            sheep: 1,
            ore: 1,
            ..ResourceCollection::ZERO
        };
    }

    pub const LONGEST_ROAD_VP: u16 = 2;
    pub const LARGEST_ARMY_VP: u16 = 2;
}
