use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    agent::action::RegularAction,
    gameplay::{
        game::view::{PlayerDecisionContext, PublicPlayerResources},
        primitives::{
            PortKind,
            build::{BoardBuildData, Build, Establishment, EstablishmentType, Road},
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::Resource,
            trade::{BankTrade, BankTradeKind},
        },
    },
    topology::{Hex, Path},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildClass {
    Road,
    Settlement,
    City,
}

pub fn legal_initial_placements(context: &PlayerDecisionContext<'_>) -> Vec<(Establishment, Road)> {
    context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, context.actor)
}

pub fn legal_builds(context: &PlayerDecisionContext<'_>, class: BuildClass) -> Vec<Build> {
    match class {
        BuildClass::Road => legal_road_spots(context, context.actor),
        BuildClass::Settlement => legal_settlement_spots(context, context.actor),
        BuildClass::City => legal_city_spots(context, context.actor),
    }
}

pub fn legal_city_spots(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
    let Some(search) = &context.search else {
        log::debug!("legal city spots require search context");
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
        log::debug!("legal settlement spots require search context");
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
        log::debug!("legal road spots require search context");
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
    context
        .private
        .resources
        .has_enough(&crate::constants::costs::DEV_CARD)
}

pub fn can_buy_road(context: &PlayerDecisionContext<'_>) -> bool {
    context
        .private
        .resources
        .has_enough(&crate::constants::costs::ROAD)
}

pub fn can_buy_settlement(context: &PlayerDecisionContext<'_>) -> bool {
    context
        .private
        .resources
        .has_enough(&crate::constants::costs::SETTLEMENT)
}

pub fn can_buy_city(context: &PlayerDecisionContext<'_>) -> bool {
    context
        .private
        .resources
        .has_enough(&crate::constants::costs::CITY)
}

pub fn legal_dev_card_usages(context: &PlayerDecisionContext<'_>) -> Vec<DevCardUsage> {
    let Some(search) = &context.search else {
        log::debug!("legal development card usages require search context");
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

            let robbed_candidates = legal_rob_targets(context, rob_hex);

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
        candidates.extend(legal_roadbuild_usages(context, &state));
    }

    candidates
        .into_iter()
        .filter(|usage| {
            let mut state = state.clone();
            state.use_dev_card(*usage, context.actor).is_ok()
        })
        .collect()
}

fn legal_roadbuild_usages(
    context: &PlayerDecisionContext<'_>,
    state: &crate::gameplay::game::state::GameState,
) -> Vec<DevCardUsage> {
    let paths = context
        .public
        .board
        .arrangement
        .paths()
        .into_iter()
        .collect::<Vec<_>>();
    let mut usages = Vec::new();

    for first in legal_road_paths_from_builds(&state.builds, context.actor, &paths) {
        let mut builds_after_first = state.builds.clone();
        if builds_after_first
            .try_build(context.actor, Build::Road(Road { pos: first }))
            .is_err()
        {
            continue;
        }

        usages.extend(
            legal_road_paths_from_builds(&builds_after_first, context.actor, &paths)
                .into_iter()
                .map(|second| DevCardUsage::RoadBuild([first, second])),
        );
    }

    usages
}

fn legal_road_paths_from_builds(
    builds: &BoardBuildData,
    player_id: PlayerId,
    paths: &[Path],
) -> Vec<Path> {
    paths
        .iter()
        .copied()
        .filter(|pos| {
            let mut candidate = builds.clone();
            candidate
                .try_build(player_id, Build::Road(Road { pos: *pos }))
                .is_ok()
        })
        .collect()
}

pub fn legal_rob_targets(context: &PlayerDecisionContext<'_>, robber_pos: Hex) -> Vec<PlayerId> {
    context
        .public
        .players_on_hex(robber_pos)
        .into_iter()
        .filter(|id| *id != context.actor)
        .filter(|id| public_resource_total(context, *id) > 0)
        .collect()
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

pub fn legal_bank_trades(context: &PlayerDecisionContext<'_>) -> Vec<BankTrade> {
    let mut result = Vec::new();

    result.extend(resource_trades_at_rate(
        context,
        BankTradeKind::BankGeneric,
        Resource::iter(),
        4,
    ));

    for port in &context.public.get_ports_aquired()[context.actor] {
        let trades = match port {
            PortKind::Special(resource) => resource_trades_at_rate(
                context,
                BankTradeKind::PortSpecific,
                std::iter::once(*resource),
                2,
            ),
            PortKind::Universal => {
                resource_trades_at_rate(context, BankTradeKind::PortGeneric, Resource::iter(), 3)
            }
        };

        result.extend(trades);
    }

    result
}

fn resource_trades_at_rate(
    context: &PlayerDecisionContext<'_>,
    kind: BankTradeKind,
    give_candidates: impl IntoIterator<Item = Resource>,
    rate: u16,
) -> Vec<BankTrade> {
    give_candidates
        .into_iter()
        .filter(|give| context.private.resources.has_enough(&(*give, rate).into()))
        .flat_map(|give| {
            Resource::iter()
                .into_iter()
                .filter(move |take| *take != give)
                .map(move |take| BankTrade { give, take, kind })
        })
        .collect()
}

pub fn legal_trades(context: &PlayerDecisionContext<'_>) -> impl IntoIterator<Item = BankTrade> {
    legal_bank_trades(context)
}

pub fn legal_regular_actions(context: &PlayerDecisionContext<'_>) -> Vec<RegularAction> {
    let mut result = Vec::new();
    result.push(RegularAction::EndMove);

    if can_buy_dev_card(context) {
        result.push(RegularAction::BuyDevCard);
    }

    if can_buy_road(context) {
        result.extend(
            legal_road_spots(context, context.actor)
                .into_iter()
                .map(RegularAction::Build),
        );
    }

    if can_buy_settlement(context) {
        result.extend(
            legal_settlement_spots(context, context.actor)
                .into_iter()
                .map(RegularAction::Build),
        );
    }

    if can_buy_city(context) {
        result.extend(
            legal_city_spots(context, context.actor)
                .into_iter()
                .map(RegularAction::Build),
        );
    }

    result.extend(
        legal_bank_trades(context)
            .into_iter()
            .map(RegularAction::TradeWithBank),
    );

    result
}

pub fn legal_regular_action(context: &PlayerDecisionContext<'_>) -> Vec<RegularAction> {
    legal_regular_actions(context)
}

#[derive(Debug, Default, Clone)]
pub struct TradeFilter {
    pub give: BTreeSet<Resource>,
    pub take: BTreeSet<Resource>,
    pub kind: BTreeSet<BankTradeKind>,
}

pub fn list_trades(white: Option<TradeFilter>, black: Option<TradeFilter>) -> Vec<BankTrade> {
    let mut result = vec![];
    for give in Resource::iter()
        .filter(|give| {
            !matches!(white.clone(), Some(white) if !white.give.is_empty() && !white.give.contains(give))
        })
        .filter(|give| !matches!(black.clone(), Some(black) if black.give.contains(give)))
    {
        for take in Resource::iter()
            .filter(|take| *take != give)
            .filter(|take| {
                !matches!(white.clone(), Some(white) if !white.take.is_empty() && !white.take.contains(take))
            })
            .filter(|take| !matches!(black.clone(), Some(black) if black.take.contains(take)))
        {
            use BankTradeKind::*;
            for kind in [BankGeneric, PortGeneric, PortSpecific]
                .into_iter()
                .filter(|k| {
                    !matches!(white.clone(), Some(white) if !white.kind.is_empty() && !white.kind.contains(k))
                })
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
    use crate::gameplay::{
        game::{
            index::GameIndex,
            init::GameInitializationState,
            state::GameState,
            view::{ContextFactory, SearchFactory, VisibilityConfig},
        },
        primitives::{
            PortKind,
            build::{BoardBuildData, Build, Establishment, EstablishmentType, Road},
            dev_card::{DevCardKind, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::BankTradeKind,
        },
    };

    use super::*;

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

    fn context_action_with_resources(resources: ResourceCollection) -> RegularAction {
        let mut state = initialized_state();
        state
            .transfer_from_bank(resources, 0)
            .expect("bank should fund test player");
        preferred_action(&state, 0)
    }

    fn preferred_action(state: &GameState, player_id: PlayerId) -> RegularAction {
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
        legal_regular_actions(&context)
            .into_iter()
            .find(|action| matches!(action, RegularAction::Build(Build::Establishment(est)) if est.stage == EstablishmentType::City))
            .or_else(|| {
                legal_regular_actions(&context)
                    .into_iter()
                    .find(|action| matches!(action, RegularAction::Build(Build::Establishment(est)) if est.stage == EstablishmentType::Settlement))
            })
            .or_else(|| {
                legal_regular_actions(&context)
                    .into_iter()
                    .find(|action| matches!(action, RegularAction::BuyDevCard))
            })
            .or_else(|| {
                legal_regular_actions(&context)
                    .into_iter()
                    .find(|action| matches!(action, RegularAction::Build(Build::Road(_))))
            })
            .unwrap_or(RegularAction::EndMove)
    }

    fn context_bank_trades(
        state: &GameState,
        player_id: PlayerId,
    ) -> Vec<crate::gameplay::primitives::trade::BankTrade> {
        let index = GameIndex::rebuild(state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state,
            index: &index,
            visibility: &visibility,
        };
        let context = factory.player_decision_context(player_id, None);
        legal_bank_trades(&context)
    }

    #[test]
    fn legal_actions_include_city_when_affordable() {
        let action = context_action_with_resources(ResourceCollection {
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
    fn legal_actions_include_settlement_when_affordable() {
        let action = context_action_with_resources(ResourceCollection {
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
    fn legal_actions_include_dev_card_when_affordable() {
        let action = context_action_with_resources(ResourceCollection {
            brick: 0,
            wood: 0,
            wheat: 1,
            sheep: 1,
            ore: 1,
        });

        assert!(matches!(action, RegularAction::BuyDevCard));
    }

    #[test]
    fn legal_actions_include_road_when_affordable() {
        let action = context_action_with_resources(ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 0,
            sheep: 0,
            ore: 0,
        });

        assert!(matches!(action, RegularAction::Build(Build::Road(_))));
    }

    #[test]
    fn initial_placements_exclude_existing_deadzone() {
        let mut init = GameInitializationState::default();
        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .next()
            .expect("default board should have an initial placement");
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");

        let state = init.finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let context = factory.player_decision_context(0, None);
        let legal = legal_initial_placements(&context)
            .into_iter()
            .map(|(settlement, _)| settlement.pos)
            .collect::<std::collections::BTreeSet<_>>();

        assert!(!legal.contains(&settlement.pos));
        for neighbor in settlement.pos.neighbors() {
            assert!(!legal.contains(&neighbor));
        }
    }

    #[test]
    fn initial_placements_have_adjacent_unoccupied_roads() {
        let state = GameInitializationState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let context = factory.player_decision_context(0, None);
        let placements = legal_initial_placements(&context);

        assert!(!placements.is_empty());
        assert!(placements.iter().all(|(settlement, road)| {
            road.pos
                .intersections_iter()
                .any(|intersection| intersection == settlement.pos)
        }));
    }

    fn state_with_port_and_resources(
        port_kind: PortKind,
        resources: ResourceCollection,
    ) -> GameState {
        let mut init = GameInitializationState::default();
        let (port_pos, _) = init
            .board
            .arrangement
            .ports()
            .iter()
            .find(|(_, kind)| **kind == port_kind)
            .or_else(|| init.board.arrangement.ports().iter().next())
            .expect("default board should have ports");
        let settlement_pos = port_pos.intersections()[0];
        let road = init
            .board
            .arrangement
            .path_set()
            .into_iter()
            .find(|path| {
                path.intersections_iter()
                    .any(|intersection| intersection == settlement_pos)
            })
            .map(|pos| Road { pos })
            .expect("port settlement should have adjacent road");
        init.builds
            .try_init_place(
                0,
                road,
                Establishment {
                    pos: settlement_pos,
                    stage: EstablishmentType::Settlement,
                },
            )
            .expect("port settlement should be valid on empty board");
        let mut state = init.finish();
        state
            .transfer_from_bank(resources, 0)
            .expect("bank should fund test resources");
        state
    }

    #[test]
    fn bank_trades_include_generic_trade_without_ports() {
        let mut state = GameInitializationState::default().finish();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 4,
                    ..ResourceCollection::ZERO
                },
                0,
            )
            .expect("bank should fund test resources");

        let options = context_bank_trades(&state, 0);

        assert!(options.iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::BankGeneric) && trade.give == Resource::Brick
        }));
    }

    #[test]
    fn bank_trades_exclude_unaffordable_generic_trades() {
        let state = GameInitializationState::default().finish();
        let options = context_bank_trades(&state, 0);

        assert!(
            options.is_empty(),
            "player with no resources should have no bank trades, got {options:?}"
        );
    }

    #[test]
    fn bank_trades_include_universal_and_specific_ports() {
        let universal = state_with_port_and_resources(
            PortKind::Universal,
            ResourceCollection {
                brick: 3,
                ..ResourceCollection::ZERO
            },
        );
        assert!(context_bank_trades(&universal, 0).iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::PortGeneric) && trade.give == Resource::Brick
        }));

        let specific = state_with_port_and_resources(
            PortKind::Special(Resource::Brick),
            ResourceCollection {
                brick: 2,
                ..ResourceCollection::ZERO
            },
        );
        assert!(context_bank_trades(&specific, 0).iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::PortSpecific) && trade.give == Resource::Brick
        }));
    }

    #[test]
    fn specific_port_trades_only_use_the_acquired_port_resource() {
        let state = state_with_port_and_resources(
            PortKind::Special(Resource::Brick),
            ResourceCollection {
                brick: 2,
                wood: 2,
                ..ResourceCollection::ZERO
            },
        );

        let options = context_bank_trades(&state, 0);

        assert!(options.iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::PortSpecific) && trade.give == Resource::Brick
        }));
        assert!(!options.iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::PortSpecific) && trade.give == Resource::Wood
        }));
    }

    #[test]
    fn legal_roadbuild_usages_are_accepted_by_game_state() {
        let mut state = initialized_state();
        state
            .players
            .get_mut(0)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::RoadBuild));
        state.players.get_mut(0).dev_cards_reset_queue();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);

        let usages = legal_dev_card_usages(&context)
            .into_iter()
            .filter(|usage| matches!(usage, DevCardUsage::RoadBuild(_)))
            .collect::<Vec<_>>();

        assert!(!usages.is_empty());
        for usage in usages {
            if let DevCardUsage::RoadBuild([first, second]) = usage {
                assert_ne!(first, second);
            }
            let mut candidate = state.clone();
            assert!(
                candidate.use_dev_card(usage, 0).is_ok(),
                "legal roadbuild usage should be accepted: {usage:?}"
            );
        }
    }
}
