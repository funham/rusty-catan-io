pub mod agent;
pub mod field;
pub mod game;
pub mod primitives;

pub mod constants {
    use crate::gameplay::primitives::resource::ResourceCollection;

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
            brick: 1,
            wood: 1,
            wheat: 1,
            sheep: 1,
            ..ResourceCollection::ZERO
        };
        pub const DEV_CARD: ResourceCollection = ResourceCollection {
            wheat: 1,
            sheep: 1,
            ore: 1,
            ..ResourceCollection::ZERO
        };
    }
}
