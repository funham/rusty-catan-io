use serde::{Deserialize, Serialize};

use super::resource::{Resource, ResourceSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BankTradeKind {
    BankGeneric,
    PortGeneric,
    PortSpecific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankTrade {
    pub give: Resource,
    pub take: Resource,
    pub kind: BankTradeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerTrade {
    pub give: ResourceSet,
    pub take: ResourceSet,
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
    pub fn to_bank(&self) -> ResourceSet {
        let res_count = match self.kind {
            BankTradeKind::BankGeneric => 4,
            BankTradeKind::PortGeneric => 3,
            BankTradeKind::PortSpecific => 2,
        };

        (self.give, res_count).into()
    }

    pub fn from_bank(&self) -> ResourceSet {
        (self.take, 1).into()
    }
}
