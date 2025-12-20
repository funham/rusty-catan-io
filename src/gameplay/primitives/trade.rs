use super::resource::{Resource, ResourceCollection};
use super::player::PlayerId;

pub struct PublicTradeOffer {
    give: ResourceCollection,
    take: ResourceCollection,
}

pub struct PersonalTradeOffer {
    give: ResourceCollection,
    take: ResourceCollection,
    peer: PlayerId,
}

pub enum BankTradeKind {
    Common,
    PortUniversal,
    PortSpecial,
}

pub struct BankTrade {
    give: Resource,
    take: Resource,
    kind: BankTradeKind,
}

pub struct PlayerTrade {
    pub give: ResourceCollection,
    pub take: ResourceCollection,
}

impl PlayerTrade {
    pub fn reflected(&self) -> Self {
        Self {
            give: self.take,
            take: self.give,
        }
    }
}
