use std::{
    collections::BTreeMap,
    ops::{Index, IndexMut},
};

use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resource {
    Brick,
    Wood,
    Wheat,
    Sheep,
    Ore,
}

impl Resource {
    pub fn list() -> [Resource; 5] {
        use Resource::*;
        [Brick, Wood, Wheat, Sheep, Ore]
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceCollection {
    pub brick: u16,
    pub wood: u16,
    pub wheat: u16,
    pub sheep: u16,
    pub ore: u16,
}

#[derive(Debug)]
pub enum ResourceCollectionSubstractionError {
    SubstractionFromSmallerCollection,
}

impl ResourceCollection {
    pub fn transfer<E>(
        from: &mut ResourceCollection,
        to: &mut ResourceCollection,
        resources: ResourceCollection,
        on_error: E,
    ) -> Result<(), E> {
        match *from - &resources {
            Some(remainder) => Ok({
                *from = remainder;
                *to += &resources;
            }),
            None => Err(on_error),
        }
    }

    pub fn new(brick: u16, wood: u16, wheat: u16, sheep: u16, ore: u16) -> Self {
        Self {
            brick,
            wood,
            wheat,
            sheep,
            ore,
        }
    }

    pub fn has_enough(&self, set: &ResourceCollection) -> bool {
        Resource::list().into_iter().all(|r| self[r] >= set[r])
    }

    pub fn empty(&self) -> bool {
        self.total() != 0
    }

    pub fn total(&self) -> u16 {
        Resource::list().into_iter().map(|r| self[r] as u16).sum()
    }

    pub fn substract(
        &self,
        rhs: &ResourceCollection,
    ) -> Result<ResourceCollection, ResourceCollectionSubstractionError> {
        match *self - rhs {
            Some(result) => Ok(result),
            None => Err(ResourceCollectionSubstractionError::SubstractionFromSmallerCollection),
        }
    }

    pub fn substract_inplace(
        &mut self,
        set: &ResourceCollection,
    ) -> Result<(), ResourceCollectionSubstractionError> {
        self.substract_inplace_or_throw(
            set,
            ResourceCollectionSubstractionError::SubstractionFromSmallerCollection,
        )
    }

    pub fn substract_inplace_or_throw<T>(
        &mut self,
        set: &ResourceCollection,
        err: T,
    ) -> Result<(), T> {
        match self.substract(set) {
            Ok(result) => Ok(*self = result),
            Err(_) => Err(err),
        }
    }

    // None if empty, weighted random otherwise
    pub fn peek_random(&self) -> Option<Resource> {
        // Calculate total and return None if empty
        if self.empty() {
            return None;
        }

        // Generate random number
        let mut rng = rand::rng();
        let mut rand_val: u16 = rng.random_range(0..self.total());

        // Find which resource corresponds to the random value
        for (resource, count) in self.unroll() {
            if rand_val < count {
                return Some(resource);
            }
            rand_val -= count;
        }

        unreachable!("peek random: total == 0?")
    }

    pub fn pop_random(&mut self) -> Option<Resource> {
        match self.peek_random() {
            Some(resource) => {
                *self = (*self - &resource.into()).unwrap();
                Some(resource)
            }
            None => None,
        }
    }

    pub fn unroll(&self) -> impl Iterator<Item = (Resource, u16)> {
        Resource::list().into_iter().map(|r| (r, self[r]))
    }
}

impl std::ops::Add<&ResourceCollection> for ResourceCollection {
    type Output = ResourceCollection;

    fn add(self, rhs: &ResourceCollection) -> Self::Output {
        Self::Output::try_from(
            Resource::list()
                .into_iter()
                .map(|r| (r, self[r] + rhs[r]))
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .expect("broken ResourceCollection::Add")
    }
}

impl std::ops::AddAssign<&ResourceCollection> for ResourceCollection {
    fn add_assign(&mut self, rhs: &ResourceCollection) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub<&ResourceCollection> for ResourceCollection {
    type Output = Option<ResourceCollection>;

    fn sub(self, rhs: &ResourceCollection) -> Self::Output {
        match Resource::list()
            .into_iter()
            .map(|r| match self[r].checked_sub(rhs[r]) {
                Some(sub) => Some((r, sub)),
                None => None,
            })
            .collect::<Option<Vec<_>>>()
        {
            Some(v) => {
                Some(ResourceCollection::try_from(&v[..]).expect("broken ResourceCollection::Sub"))
            }
            _ => None,
        }
    }
}

impl Into<ResourceCollection> for Resource {
    fn into(self) -> ResourceCollection {
        let mut res = ResourceCollection::default();
        res[self] = 1;
        res
    }
}

impl Into<ResourceCollection> for (Resource, u16) {
    fn into(self) -> ResourceCollection {
        let mut res: ResourceCollection = self.0.into();
        res[self.0] *= self.1;
        res
    }
}

impl TryFrom<&[(Resource, u16)]> for ResourceCollection {
    type Error = ResourceCollectionCollectError;

    fn try_from(value: &[(Resource, u16)]) -> Result<Self, Self::Error> {
        let mut this = Self::default();
        for (resource, number) in value {
            if this[*resource] != 0 {
                return Err(ResourceCollectionCollectError::ResourceAppearTwice);
            }

            this[*resource] = *number;
        }

        Ok(this)
    }
}

impl From<BTreeMap<Resource, u16>> for ResourceCollection {
    fn from(value: BTreeMap<Resource, u16>) -> Self {
        let x: Vec<_> = value.into_iter().collect();
        TryFrom::<&[(Resource, u16)]>::try_from(x.as_slice()).unwrap()
    }
}

impl Index<Resource> for ResourceCollection {
    type Output = u16;

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

impl IndexMut<Resource> for ResourceCollection {
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
pub trait HasCost {
    fn cost(&self) -> ResourceCollection;
}

#[derive(Debug)]
pub enum ResourceCollectionCollectError {
    ResourceAppearTwice,
}
