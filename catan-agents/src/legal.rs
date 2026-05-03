use std::collections::BTreeSet;

use catan_core::{
    agent::action::RegularAction,
    constants::costs,
    gameplay::{
        game::view::{PlayerDecisionContext, PublicPlayerResources},
        primitives::{
            PortKind,
            build::{Build, Establishment, EstablishmentType, Road},
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::Resource,
            trade::{BankTrade, BankTradeKind},
        },
    },
};

pub fn legal_city_spots(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
    let Some(search) = &context.search else {
        return Vec::new();
    };
    let seed = search.make_owned();

    seed.state.builds[player_id]
        .establishments
        .iter()
        .copied()
        .filter(|est| est.stage == EstablishmentType::Settlement)
        .map(|est| {
            Build::Establishment(Establishment {
                pos: est.pos,
                stage: EstablishmentType::City,
            })
        })
        .filter(|build| {
            let mut state = seed.state.clone();
            state.build(player_id, *build).is_ok()
        })
        .collect()
}

pub fn legal_settlement_spots(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Vec<Build> {
    let Some(search) = &context.search else {
        return Vec::new();
    };
    let seed = search.make_owned();

    context
        .public
        .board
        .arrangement
        .intersections()
        .into_iter()
        .map(|pos| {
            Build::Establishment(Establishment {
                pos,
                stage: EstablishmentType::Settlement,
            })
        })
        .filter(|build| {
            let mut state = seed.state.clone();
            state.build(player_id, *build).is_ok()
        })
        .collect()
}

pub fn legal_road_spots(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
    let Some(search) = &context.search else {
        return Vec::new();
    };
    let seed = search.make_owned();

    context
        .public
        .board
        .arrangement
        .paths()
        .into_iter()
        .map(|pos| Build::Road(Road { pos }))
        .filter(|build| {
            let mut state = seed.state.clone();
            state.build(player_id, *build).is_ok()
        })
        .collect()
}

pub fn can_buy_dev_card(context: &PlayerDecisionContext<'_>) -> bool {
    // let Some(search) = &context.search else {
    //     return false;
    // };
    // let mut state = search.make_owned().state;
    // state.buy_dev_card(player_id).is_ok()

    context.private.resources.has_enough(&costs::DEV_CARD)
}

pub fn can_buy_road(context: &PlayerDecisionContext<'_>) -> bool {
    context.private.resources.has_enough(&costs::ROAD)
}

pub fn can_buy_settlement(context: &PlayerDecisionContext<'_>) -> bool {
    context.private.resources.has_enough(&costs::SETTLEMENT)
}

pub fn can_buy_city(context: &PlayerDecisionContext<'_>) -> bool {
    context.private.resources.has_enough(&costs::CITY)
}

pub fn legal_dev_card_usages(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Vec<DevCardUsage> {
    let Some(search) = &context.search else {
        return Vec::new();
    };

    let state = search.make_owned().state;
    let active = context.private.dev_cards.active;
    let mut candidates = Vec::new();

    if active.contains(UsableDevCard::Knight) {
        for rob_hex in context.public.board.arrangement.hex_iter() {
            if rob_hex == context.public.board_state.robber_pos {
                continue;
            }

            let robbed_candidates = context
                .public
                .players_on_hex(rob_hex)
                .into_iter()
                .filter(|id| *id != player_id)
                .filter(|id| public_resource_total(context, *id) > 0)
                .collect::<Vec<_>>();

            match robbed_candidates.as_slice() {
                [] => candidates.push(DevCardUsage::Knight {
                    rob_hex,
                    robbed_id: None,
                }),
                ids => candidates.extend(ids.iter().map(|robbed_id| DevCardUsage::Knight {
                    rob_hex,
                    robbed_id: Some(*robbed_id),
                })),
            }
        }
    }

    if active.contains(UsableDevCard::YearOfPlenty) {
        for first in Resource::iter() {
            for second in Resource::iter() {
                candidates.push(DevCardUsage::YearOfPlenty([first, second]));
            }
        }
    }

    if active.contains(UsableDevCard::Monopoly) {
        candidates.extend(Resource::iter().into_iter().map(DevCardUsage::Monopoly));
    }

    if active.contains(UsableDevCard::RoadBuild) {
        let roads = context
            .public
            .board
            .arrangement
            .paths()
            .into_iter()
            .collect::<Vec<_>>();
        for first in &roads {
            for second in &roads {
                if first != second {
                    candidates.push(DevCardUsage::RoadBuild([*first, *second]));
                }
            }
        }
    }

    candidates
        .into_iter()
        .filter(|usage| {
            let mut state = state.clone();
            state.use_dev_card(usage.clone(), player_id).is_ok()
        })
        .collect()
}

pub fn legal_rob_targets(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> impl Iterator<Item = PlayerId> {
    context
        .public
        .players_on_hex(context.public.board_state.robber_pos)
        .into_iter()
        .filter(move |id| id != &player_id)
}

fn public_resource_total(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> u16 {
    if player_id == context.actor {
        return context.private.resources.total();
    }

    context
        .public
        .players
        .iter()
        .find(|player| player.player_id == player_id)
        .map(|player| match player.resources {
            PublicPlayerResources::Exact(resources) => resources.total(),
            PublicPlayerResources::Total(total) => total,
        })
        .unwrap_or_default()
}

pub fn legal_trades(context: &PlayerDecisionContext<'_>) -> impl IntoIterator<Item = BankTrade> {
    let mut result = Vec::new();

    /* bank-trade regular */
    {
        let give = Resource::iter()
            .filter(|resource| context.private.resources.has_enough(&(*resource, 4).into()))
            .collect();

        let trades = list_trades(
            Some(TradeFilter {
                give,
                kind: [BankTradeKind::BankGeneric].into(),
                ..Default::default()
            }),
            None,
        )
        .into_iter();

        result.extend(trades);
    }

    /* bank-trade ports */

    for port in &context.public.get_ports_aquired()[context.actor] {
        let trades = match port {
            PortKind::Special(resource) => {
                let give = match context.private.resources.has_enough(&(*resource, 2).into()) {
                    true => [*resource].into(),
                    false => [].into(),
                };

                list_trades(
                    Some(TradeFilter {
                        give,
                        kind: [BankTradeKind::PortSpecific].into(),
                        ..Default::default()
                    }),
                    None,
                )
            }
            PortKind::Universal => {
                let give = Resource::iter()
                    .filter(|resource| context.private.resources.has_enough(&(*resource, 2).into()))
                    .collect();

                list_trades(
                    Some(TradeFilter {
                        give,
                        kind: [BankTradeKind::PortGeneric].into(),
                        ..Default::default()
                    }),
                    None,
                )
            }
        }
        .into_iter();

        result.extend(trades);
    }
    result
}

pub fn legal_regular_action(context: &PlayerDecisionContext<'_>) -> Vec<RegularAction> {
    let mut result = Vec::new();

    /* end move */

    result.push(RegularAction::EndMove);

    /* buy dev card */

    if can_buy_dev_card(context) {
        result.push(RegularAction::BuyDevCard);
    }

    /* builds  */

    if can_buy_road(context) {
        result.extend(
            legal_road_spots(context, context.actor)
                .into_iter()
                .map(|road| RegularAction::Build(road)),
        );
    }

    if can_buy_settlement(context) {
        result.extend(
            legal_settlement_spots(context, context.actor)
                .into_iter()
                .map(|settlement| RegularAction::Build(settlement)),
        );
    }

    if can_buy_city(context) {
        result.extend(
            legal_city_spots(context, context.actor)
                .into_iter()
                .map(|city| RegularAction::Build(city)),
        );
    }

    /* bank trades */

    result.extend(
        legal_trades(context)
            .into_iter()
            .map(|trade| RegularAction::TradeWithBank(trade)),
    );

    // RegularAction::OfferPublicTrade(public_trade_offer) => todo!(),
    // RegularAction::OfferPersonalTrade(personal_trade_offer) => todo!(),

    result
}

#[derive(Debug, Default, Clone)]
pub struct TradeFilter {
    pub give: BTreeSet<Resource>,
    pub take: BTreeSet<Resource>,
    pub kind: BTreeSet<BankTradeKind>,
}

pub fn list_trades(
    white: Option<TradeFilter>,
    black: Option<TradeFilter>,
) -> impl IntoIterator<Item = BankTrade> {
    let mut result = vec![];
    for give in Resource::iter()
        // whitelist
        .filter(|give| !matches!(white.clone(), Some(white) if !white.give.contains(give)))
        // blacklist
        .filter(|give| !matches!(black.clone(), Some(black) if black.give.contains(give)))
    {
        for take in Resource::iter()
            .filter(|take| *take != give)
            // whitelist
            .filter(|take| !matches!(white.clone(), Some(white) if !white.take.contains(take)))
            // blacklist
            .filter(|take| !matches!(black.clone(), Some(black) if black.take.contains(take)))
        {
            use BankTradeKind::*;
            for kind in [BankGeneric, PortGeneric, PortSpecific]
                .into_iter()
                // whitelist
                .filter(|k| !matches!(white.clone(), Some(white) if !white.kind.contains(k)))
                // blacklist
                .filter(|k| !matches!(black.clone(), Some(black) if black.kind.contains(k)))
            {
                result.push(BankTrade { give, take, kind });
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use catan_core::{
        agent::action::RegularAction,
        gameplay::{
            game::{
                index::GameIndex,
                init::GameInitializationState,
                state::GameState,
                view::{ContextFactory, SearchFactory, VisibilityConfig},
            },
            primitives::{
                build::{BoardBuildData, Build, Establishment, EstablishmentType, Road},
                player::PlayerId,
                resource::ResourceCollection,
            },
        },
    };

    use crate::greedy;

    fn initialized_state() -> GameState {
        let mut init = GameInitializationState::default();
        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .next()
            .expect("default board should have initial placements");
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");
        init.builds = find_builds_with_legal_settlement(&init, init.builds.clone(), 4)
            .expect("test should find a road network with a legal settlement");
        init.finish()
    }

    fn find_builds_with_legal_settlement(
        init: &GameInitializationState,
        builds: BoardBuildData,
        depth: u8,
    ) -> Option<BoardBuildData> {
        if has_legal_settlement(init, &builds) {
            return Some(builds);
        }
        if depth == 0 {
            return None;
        }

        for pos in init.board.arrangement.paths() {
            let mut candidate = builds.clone();
            if candidate.try_build(0, Build::Road(Road { pos })).is_err() {
                continue;
            }
            if let Some(found) = find_builds_with_legal_settlement(init, candidate, depth - 1) {
                return Some(found);
            }
        }

        None
    }

    fn has_legal_settlement(init: &GameInitializationState, builds: &BoardBuildData) -> bool {
        init.board
            .arrangement
            .intersections()
            .into_iter()
            .any(|pos| {
                let mut candidate = builds.clone();
                candidate
                    .try_build(
                        0,
                        Build::Establishment(Establishment {
                            pos,
                            stage: EstablishmentType::Settlement,
                        }),
                    )
                    .is_ok()
            })
    }

    fn greedy_action_with_resources(resources: ResourceCollection) -> RegularAction {
        let mut state = initialized_state();
        state
            .transfer_from_bank(resources, 0)
            .expect("bank should fund test player");
        greedy_action(&state, 0)
    }

    fn greedy_action(state: &GameState, player_id: PlayerId) -> RegularAction {
        let index = GameIndex::rebuild(state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(
            state,
            visibility.player_policy(player_id),
            player_id,
        ));
        let context = factory.player_decision_context(player_id, search);
        greedy::greedy_regular_action(&context, player_id)
    }

    #[test]
    fn greedy_builds_city_before_settlement() {
        let action = greedy_action_with_resources(ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 3,
            sheep: 1,
            ore: 3,
        });

        match action {
            RegularAction::Build(Build::Establishment(establishment)) => {
                assert_eq!(establishment.stage, EstablishmentType::City);
            }
            other => panic!("expected city build, got {other:?}"),
        }
    }

    #[test]
    fn greedy_builds_settlement_before_buying_dev_card() {
        let action = greedy_action_with_resources(ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 1,
            sheep: 1,
            ore: 1,
        });

        match action {
            RegularAction::Build(Build::Establishment(establishment)) => {
                assert_eq!(establishment.stage, EstablishmentType::Settlement);
            }
            other => panic!("expected settlement build, got {other:?}"),
        }
    }

    #[test]
    fn greedy_buys_dev_card_when_no_build_is_affordable() {
        let action = greedy_action_with_resources(ResourceCollection {
            brick: 0,
            wood: 0,
            wheat: 1,
            sheep: 1,
            ore: 1,
        });

        assert!(matches!(action, RegularAction::BuyDevCard));
    }

    #[test]
    fn greedy_builds_road_when_no_higher_priority_action_is_affordable() {
        let action = greedy_action_with_resources(ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 0,
            sheep: 0,
            ore: 0,
        });

        assert!(matches!(action, RegularAction::Build(Build::Road(_))));
    }
}
