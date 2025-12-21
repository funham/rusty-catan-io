use super::player::PlayerId;
use super::resource::{Resource, ResourceCollection};

pub struct PublicTradeOffer {
    pub give: ResourceCollection,
    pub take: ResourceCollection,
}

pub struct PersonalTradeOffer {
    pub give: ResourceCollection,
    pub take: ResourceCollection,
    pub peer_id: PlayerId,
}

pub enum BankTradeKind {
    Common,
    PortUniversal,
    PortSpecial,
}

pub struct BankTrade {
    pub give: Resource,
    pub take: Resource,
    pub kind: BankTradeKind,
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

impl BankTrade {
    pub fn to_bank(&self) -> ResourceCollection {
        let res_count = match self.kind {
            BankTradeKind::Common => 4,
            BankTradeKind::PortUniversal => 3,
            BankTradeKind::PortSpecial => 2,
        };

        (self.give, res_count).into()
    }

    pub fn from_bank(&self) -> ResourceCollection {
        (self.take, 1).into()
    }
}
