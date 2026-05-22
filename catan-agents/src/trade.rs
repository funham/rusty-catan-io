use catan_core::gameplay::{
    game::{
        trade::{TradeOfferId, TradeSession},
        view::PlayerDecisionContext,
    },
    primitives::{
        player::PlayerId,
        resource::{Resource, ResourceSet},
        trade::PlayerTrade,
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
    if offer.proposer == session.proposer {
        proposer.has_enough(&offer.trade.give) && peer.has_enough(&offer.trade.take)
    } else {
        peer_id == offer.proposer
            && peer.has_enough(&offer.trade.give)
            && proposer.has_enough(&offer.trade.take)
    }
}

pub fn first_funded_offer_for_peer(
    context: &PlayerDecisionContext<'_>,
    session: &TradeSession,
    peer_id: PlayerId,
) -> Option<TradeOfferId> {
    session
        .offers
        .iter()
        .filter(|offer| offer.proposer == session.proposer)
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
            let peer_id = if offer.proposer == session.proposer {
                session.accepted_peer_for_offer(offer.id)?
            } else {
                offer.proposer
            };
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

pub fn one_card_trade_candidates(context: &PlayerDecisionContext<'_>) -> Vec<PlayerTrade> {
    Resource::iter()
        .filter(|give| context.private.resources[*give] > 0)
        .flat_map(move |give| {
            Resource::iter()
                .filter(move |take| *take != give)
                .map(move |take| one_card_trade(give, take))
        })
        .collect()
}
