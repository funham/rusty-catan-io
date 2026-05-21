use catan_core::gameplay::{
    game::{
        trade::{TradeOfferId, TradeScope, TradeSession},
        view::PlayerDecisionContext,
    },
    primitives::{
        player::{PlayerId, player_ids},
        resource::{Resource, ResourceSet},
        trade::{PlayerTrade, PublicTradeOffer},
    },
};

pub fn session<'a>(
    context: &'a PlayerDecisionContext<'_>,
    session_id: catan_core::gameplay::game::trade::TradeSessionId,
) -> Option<&'a TradeSession> {
    context
        .public
        .trade_sessions
        .iter()
        .find(|session| session.id == session_id && session.open)
}

pub fn exact_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Option<ResourceSet> {
    if player_id == context.actor {
        return Some(*context.private.resources);
    }
    context
        .search
        .as_ref()
        .map(|search| *search.state().players.get(player_id).resources())
}

pub fn offer_is_funded(
    context: &PlayerDecisionContext<'_>,
    session: &TradeSession,
    offer_id: TradeOfferId,
    peer_id: PlayerId,
) -> bool {
    let Some(offer) = session.offer(offer_id) else {
        return false;
    };
    let Some(proposer) = exact_resources(context, session.proposer) else {
        return false;
    };
    let Some(peer) = exact_resources(context, peer_id) else {
        return false;
    };
    proposer.has_enough(&offer.trade.give) && peer.has_enough(&offer.trade.take)
}

pub fn first_funded_offer_for_peer(
    context: &PlayerDecisionContext<'_>,
    session: &TradeSession,
    peer_id: PlayerId,
) -> Option<TradeOfferId> {
    session
        .offers
        .iter()
        .filter(|offer| offer.peer.is_none() || offer.peer == Some(peer_id))
        .find(|offer| offer_is_funded(context, session, offer.id, peer_id))
        .map(|offer| offer.id)
}

pub fn committable_offers(
    context: &PlayerDecisionContext<'_>,
    session: &TradeSession,
) -> Vec<(TradeOfferId, PlayerId)> {
    session
        .offers
        .iter()
        .filter_map(move |offer| {
            let peer_id = offer
                .peer
                .or_else(|| session.accepted_peer_for_offer(offer.id))?;
            offer_is_funded(context, session, offer.id, peer_id).then_some((offer.id, peer_id))
        })
        .collect()
}

pub fn resources_after_as_peer(
    resources: &ResourceSet,
    trade: &PlayerTrade,
) -> Option<ResourceSet> {
    let mut after = resources.checked_sub(&trade.take)?;
    after += trade.give;
    Some(after)
}

pub fn resources_after_as_proposer(
    resources: &ResourceSet,
    trade: &PlayerTrade,
) -> Option<ResourceSet> {
    let mut after = resources.checked_sub(&trade.give)?;
    after += trade.take;
    Some(after)
}

pub fn one_card_trade(give: Resource, take: Resource) -> PlayerTrade {
    PlayerTrade {
        give: give.into(),
        take: take.into(),
    }
}

pub fn one_card_public_offer(give: Resource, take: Resource) -> PublicTradeOffer {
    PublicTradeOffer {
        give: give.into(),
        take: take.into(),
    }
}

pub fn one_card_trade_candidates(
    context: &PlayerDecisionContext<'_>,
) -> Vec<(TradeScope, PlayerTrade)> {
    let player_count = context.public.players.len();
    Resource::iter()
        .filter(|give| context.private.resources[*give] > 0)
        .flat_map(move |give| {
            Resource::iter()
                .filter(move |take| *take != give)
                .flat_map(move |take| {
                    player_ids(player_count)
                        .filter(move |peer| *peer != context.actor)
                        .map(move |peer| (TradeScope::Targeted(peer), one_card_trade(give, take)))
                })
        })
        .collect()
}
