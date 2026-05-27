use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::gameplay::primitives::{
    player::{PlayerId, player_ids},
    resource::{Resource, ResourceSet},
    trade::PlayerTrade,
};

type TradeOffers = SmallVec<[TradeOffer; 16]>;
type TradeResponses = SmallVec<[Option<TradeResponseState>; 8]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TradeSessionId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TradeOfferId(pub u64);

impl From<usize> for TradeOfferId {
    fn from(value: usize) -> Self {
        Self(value as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeOffer {
    pub id: TradeOfferId,
    pub proposer: PlayerId,
    pub trade: PlayerTrade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeResponseState {
    Waiting,
    Accepted { offer_id: TradeOfferId },
    Rejected,
    Countered { offer_id: TradeOfferId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeSession {
    pub id: TradeSessionId,
    pub proposer: PlayerId,
    pub offers: TradeOffers,
    pub responses: TradeResponses,
    pub version: u64,
    pub open: bool,
}

impl TradeSession {
    pub fn new(
        id: TradeSessionId,
        proposer: PlayerId,
        trade: PlayerTrade,
        player_count: usize,
    ) -> Self {
        let mut responses = (0..player_count).map(|_| None).collect::<TradeResponses>();
        for player_id in player_ids(player_count) {
            if player_id == proposer {
                continue;
            }
            responses[player_id.index()] = Some(TradeResponseState::Waiting);
        }

        Self {
            id,
            proposer,
            offers: [TradeOffer {
                id: TradeOfferId(0),
                proposer,
                trade,
            }]
            .into_iter()
            .collect(),
            responses,
            version: 0,
            open: true,
        }
    }

    pub fn original_offer_id(&self) -> TradeOfferId {
        self.offers[0].id
    }

    pub fn offer(&self, id: TradeOfferId) -> Option<&TradeOffer> {
        self.offers.iter().find(|offer| offer.id == id)
    }

    pub fn add_counter_offer(&mut self, proposer: PlayerId, trade: PlayerTrade) -> TradeOfferId {
        let id = TradeOfferId(self.offers.len() as u64);
        self.offers.push(TradeOffer {
            id,
            proposer,
            trade,
        });
        self.version += 1;
        id
    }

    pub fn set_response(&mut self, player_id: PlayerId, response: TradeResponseState) {
        self.responses[player_id.index()] = Some(response);
        self.version += 1;
    }

    pub fn accepted_peer_for_offer(&self, offer_id: TradeOfferId) -> Option<PlayerId> {
        self.responses
            .iter()
            .enumerate()
            .find_map(|(player_id, response)| match response {
                Some(TradeResponseState::Accepted { offer_id: accepted })
                | Some(TradeResponseState::Countered { offer_id: accepted })
                    if *accepted == offer_id =>
                {
                    Some(PlayerId::try_from(player_id).expect("player count should fit in u8"))
                }
                _ => None,
            })
    }
}

pub fn trade_has_overlapping_resources(trade: &PlayerTrade) -> bool {
    Resource::iter().any(|resource| trade.give[resource] > 0 && trade.take[resource] > 0)
}

pub fn trade_has_empty_side(trade: &PlayerTrade) -> bool {
    trade.give.is_empty() || trade.take.is_empty()
}

pub fn trade_is_structurally_valid(trade: &PlayerTrade) -> bool {
    !trade_has_empty_side(trade) && !trade_has_overlapping_resources(trade)
}

pub fn trade_is_funded(
    proposer_resources: &ResourceSet,
    peer_resources: &ResourceSet,
    trade: &PlayerTrade,
) -> bool {
    proposer_resources.has_enough(&trade.give) && peer_resources.has_enough(&trade.take)
}

#[cfg(test)]
mod tests {
    use crate::gameplay::primitives::{
        resource::{Resource, ResourceSet},
        trade::PlayerTrade,
    };

    use super::trade_is_structurally_valid;

    #[test]
    fn player_trade_structure_requires_non_empty_disjoint_sides() {
        assert!(!trade_is_structurally_valid(&PlayerTrade {
            give: ResourceSet::EMPTY,
            take: ResourceSet::from(Resource::Wood),
        }));
        assert!(!trade_is_structurally_valid(&PlayerTrade {
            give: ResourceSet::from(Resource::Brick),
            take: ResourceSet::EMPTY,
        }));
        assert!(!trade_is_structurally_valid(&PlayerTrade {
            give: ResourceSet::from(Resource::Brick),
            take: ResourceSet::from(Resource::Brick),
        }));
        assert!(trade_is_structurally_valid(&PlayerTrade {
            give: ResourceSet::from(Resource::Brick),
            take: ResourceSet::from(Resource::Wood),
        }));
    }
}
