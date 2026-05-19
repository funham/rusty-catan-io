use std::{
    collections::BTreeMap,
    ops::{Add, AddAssign, Index, IndexMut},
};

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
    pub const ALL: [Resource; 5] = [
        Resource::Brick,
        Resource::Wood,
        Resource::Wheat,
        Resource::Sheep,
        Resource::Ore,
    ];

    pub fn iter() -> impl Iterator<Item = Resource> {
        Self::ALL.iter().cloned()
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

pub type ResourceSet = ResourceMap<u16>;

impl std::fmt::Display for ResourceSet {
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
        available: ResourceSet,
        required: ResourceSet,
    },
    ResourceAppearsTwice,
}

impl ResourceSet {
    pub const EMPTY: Self = Self {
        brick: 0,
        wood: 0,
        wheat: 0,
        sheep: 0,
        ore: 0,
    };

    pub fn transfer(
        from: &mut ResourceSet,
        to: &mut ResourceSet,
        resources: ResourceSet,
    ) -> Result<(), ResourceCollectionError> {
        let remainder = from.try_sub(&resources)?;
        *from = remainder;
        *to += &resources;
        Ok(())
    }

    pub fn has_enough(&self, set: &ResourceSet) -> bool {
        Resource::iter().into_iter().all(|r| self[r] >= set[r])
    }

    pub fn missing(&self, target: &ResourceSet) -> ResourceSet {
        ResourceSet {
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

    pub fn checked_sub(&self, rhs: &ResourceSet) -> Option<ResourceSet> {
        if !self.has_enough(rhs) {
            return None;
        }

        Some(ResourceSet {
            brick: self.brick - rhs.brick,
            wood: self.wood - rhs.wood,
            wheat: self.wheat - rhs.wheat,
            sheep: self.sheep - rhs.sheep,
            ore: self.ore - rhs.ore,
        })
    }

    pub fn try_sub(&self, rhs: &ResourceSet) -> Result<ResourceSet, ResourceCollectionError> {
        self.checked_sub(rhs)
            .ok_or(ResourceCollectionError::InsufficientResources {
                available: *self,
                required: *rhs,
            })
    }

    pub fn subtract_in_place(&mut self, rhs: &ResourceSet) -> Result<(), ResourceCollectionError> {
        *self = self.try_sub(rhs)?;
        Ok(())
    }

    pub fn unroll(&self) -> impl Iterator<Item = (Resource, u16)> {
        Resource::iter().into_iter().map(|r| (r, self[r]))
    }
}

impl Add for ResourceSet {
    type Output = ResourceSet;

    fn add(self, rhs: ResourceSet) -> Self::Output {
        self + &rhs
    }
}

impl Add<&ResourceSet> for ResourceSet {
    type Output = ResourceSet;

    fn add(self, rhs: &ResourceSet) -> Self::Output {
        ResourceSet {
            brick: self.brick + rhs.brick,
            wood: self.wood + rhs.wood,
            wheat: self.wheat + rhs.wheat,
            sheep: self.sheep + rhs.sheep,
            ore: self.ore + rhs.ore,
        }
    }
}

impl AddAssign for ResourceSet {
    fn add_assign(&mut self, rhs: ResourceSet) {
        *self += &rhs;
    }
}

impl AddAssign<&ResourceSet> for ResourceSet {
    fn add_assign(&mut self, rhs: &ResourceSet) {
        *self = *self + rhs;
    }
}

impl From<Resource> for ResourceSet {
    fn from(resource: Resource) -> ResourceSet {
        let mut res = ResourceSet::default();
        res[resource] = 1;
        res
    }
}

impl From<(Resource, u16)> for ResourceSet {
    fn from((resource, count): (Resource, u16)) -> ResourceSet {
        let mut res = ResourceSet::default();
        res[resource] = count;
        res
    }
}

impl From<BTreeMap<Resource, u16>> for ResourceSet {
    fn from(value: BTreeMap<Resource, u16>) -> Self {
        let x: Vec<_> = value.into_iter().collect();
        TryFrom::<&[(Resource, u16)]>::try_from(x.as_slice()).unwrap()
    }
}
