use std::{
    collections::BTreeMap,
    ops::{Add, AddAssign, Index, IndexMut},
};

use rand::RngExt;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    strum::IntoStaticStr,
)]
#[serde(rename_all = "lowercase")]
pub enum Resource {
    Brick,
    Wood,
    Wheat,
    Sheep,
    Ore,
}

impl Resource {
    pub const LIST: [Resource; 5] = [
        Resource::Brick,
        Resource::Wood,
        Resource::Wheat,
        Resource::Sheep,
        Resource::Ore,
    ];

    pub fn iter() -> impl Iterator<Item = Resource> {
        Self::LIST.iter().cloned()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceMap<T> {
    pub brick: T,
    pub wood: T,
    pub wheat: T,
    pub sheep: T,
    pub ore: T,
}

impl<T> Index<Resource> for ResourceMap<T> {
    type Output = T;

    fn index(&self, resource: Resource) -> &Self::Output {
        match resource {
            Resource::Brick => &self.brick,
            Resource::Wood => &self.wood,
            Resource::Wheat => &self.wheat,
            Resource::Sheep => &self.sheep,
            Resource::Ore => &self.ore,
        }
    }
}

impl<T> IndexMut<Resource> for ResourceMap<T> {
    fn index_mut(&mut self, resource: Resource) -> &mut Self::Output {
        match resource {
            Resource::Brick => &mut self.brick,
            Resource::Wood => &mut self.wood,
            Resource::Wheat => &mut self.wheat,
            Resource::Sheep => &mut self.sheep,
            Resource::Ore => &mut self.ore,
        }
    }
}

impl<T: Default + Copy> TryFrom<&[(Resource, T)]> for ResourceMap<T> {
    type Error = ResourceCollectionError;

    fn try_from(flat_map: &[(Resource, T)]) -> Result<Self, Self::Error> {
        let mut this = Self::default();
        let mut seen = ResourceMap::default();

        for (resource, value) in flat_map {
            if seen[*resource] {
                return Err(ResourceCollectionError::ResourceAppearsTwice);
            }

            seen[*resource] = true;
            this[*resource] = *value;
        }

        Ok(this)
    }
}

pub type ResourceCollection = ResourceMap<u16>;

impl std::fmt::Display for ResourceCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")?;
        write!(f, "Brick: {}, ", self.brick)?;
        write!(f, "Wood: {}, ", self.wood)?;
        write!(f, "Wheat: {}, ", self.wheat)?;
        write!(f, "Sheep: {}, ", self.sheep)?;
        write!(f, "Ore: {}", self.ore)?;
        write!(f, "}}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceCollectionError {
    InsufficientResources {
        available: ResourceCollection,
        required: ResourceCollection,
    },
    ResourceAppearsTwice,
}

impl ResourceCollection {
    pub const ZERO: Self = Self {
        brick: 0,
        wood: 0,
        wheat: 0,
        sheep: 0,
        ore: 0,
    };

    pub fn transfer(
        from: &mut ResourceCollection,
        to: &mut ResourceCollection,
        resources: ResourceCollection,
    ) -> Result<(), ResourceCollectionError> {
        let remainder = from.try_sub(&resources)?;
        *from = remainder;
        *to += &resources;
        Ok(())
    }

    pub fn has_enough(&self, set: &ResourceCollection) -> bool {
        Resource::iter().into_iter().all(|r| self[r] >= set[r])
    }

    pub fn missing(&self, target: &ResourceCollection) -> ResourceCollection {
        ResourceCollection {
            brick: target.brick.saturating_sub(self.brick),
            wood: target.wood.saturating_sub(self.wood),
            wheat: target.wheat.saturating_sub(self.wheat),
            sheep: target.sheep.saturating_sub(self.sheep),
            ore: target.ore.saturating_sub(self.ore),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn total(&self) -> u16 {
        Resource::iter().into_iter().map(|r| self[r] as u16).sum()
    }

    pub fn checked_sub(&self, rhs: &ResourceCollection) -> Option<ResourceCollection> {
        if !self.has_enough(rhs) {
            return None;
        }

        Some(ResourceCollection {
            brick: self.brick - rhs.brick,
            wood: self.wood - rhs.wood,
            wheat: self.wheat - rhs.wheat,
            sheep: self.sheep - rhs.sheep,
            ore: self.ore - rhs.ore,
        })
    }

    pub fn try_sub(
        &self,
        rhs: &ResourceCollection,
    ) -> Result<ResourceCollection, ResourceCollectionError> {
        self.checked_sub(rhs)
            .ok_or(ResourceCollectionError::InsufficientResources {
                available: *self,
                required: *rhs,
            })
    }

    pub fn subtract_in_place(
        &mut self,
        rhs: &ResourceCollection,
    ) -> Result<(), ResourceCollectionError> {
        *self = self.try_sub(rhs)?;
        Ok(())
    }

    // None if empty, weighted random otherwise
    pub fn peek_random(&self) -> Option<Resource> {
        // Calculate total and return None if empty
        if self.is_empty() {
            return None;
        }

        log::debug!("self.total={}", self.total());

        // Generate random number
        let mut rng = rand::rng();
        let rand_val: u16 = rng.random_range(0..self.total());
        let mut cum_total: u16 = 0;

        // Find which resource corresponds to the random value
        for (resource, count) in self.unroll() {
            cum_total += count;

            if rand_val < cum_total {
                return Some(resource);
            }
        }

        unreachable!("peek random: total == 0?")
    }

    pub fn pop_random(&mut self) -> Option<Resource> {
        match self.peek_random() {
            Some(resource) => {
                self.subtract_in_place(&resource.into()).ok()?;
                Some(resource)
            }
            None => None,
        }
    }

    pub fn unroll(&self) -> impl Iterator<Item = (Resource, u16)> {
        Resource::iter().into_iter().map(|r| (r, self[r]))
    }
}

impl Add for ResourceCollection {
    type Output = ResourceCollection;

    fn add(self, rhs: ResourceCollection) -> Self::Output {
        self + &rhs
    }
}

impl Add<&ResourceCollection> for ResourceCollection {
    type Output = ResourceCollection;

    fn add(self, rhs: &ResourceCollection) -> Self::Output {
        ResourceCollection {
            brick: self.brick + rhs.brick,
            wood: self.wood + rhs.wood,
            wheat: self.wheat + rhs.wheat,
            sheep: self.sheep + rhs.sheep,
            ore: self.ore + rhs.ore,
        }
    }
}

impl AddAssign for ResourceCollection {
    fn add_assign(&mut self, rhs: ResourceCollection) {
        *self += &rhs;
    }
}

impl AddAssign<&ResourceCollection> for ResourceCollection {
    fn add_assign(&mut self, rhs: &ResourceCollection) {
        *self = *self + rhs;
    }
}

impl From<Resource> for ResourceCollection {
    fn from(resource: Resource) -> ResourceCollection {
        let mut res = ResourceCollection::default();
        res[resource] = 1;
        res
    }
}

impl From<(Resource, u16)> for ResourceCollection {
    fn from((resource, count): (Resource, u16)) -> ResourceCollection {
        let mut res = ResourceCollection::default();
        res[resource] = count;
        res
    }
}

impl From<BTreeMap<Resource, u16>> for ResourceCollection {
    fn from(value: BTreeMap<Resource, u16>) -> Self {
        let x: Vec<_> = value.into_iter().collect();
        TryFrom::<&[(Resource, u16)]>::try_from(x.as_slice()).unwrap()
    }
}

pub trait HasCost {
    fn cost(&self) -> ResourceCollection;
}
