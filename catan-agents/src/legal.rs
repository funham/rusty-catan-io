use catan_core::{
    agent::action::RegularAction,
    gameplay::{
        game::view::{PlayerDecisionContext, PublicPlayerResources},
        primitives::{
            build::{Build, Establishment, EstablishmentType, Road},
            dev_card::{DevCardUsage, UsableDevCardKind},
            player::PlayerId,
            resource::Resource,
        },
    },
};

pub fn legal_cities(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
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

pub fn legal_settlements(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
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

pub fn legal_roads(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
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

pub fn can_buy_dev_card(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> bool {
    let Some(search) = &context.search else {
        return false;
    };
    let mut state = search.make_owned().state;
    state.buy_dev_card(player_id).is_ok()
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

    if active.contains(UsableDevCardKind::Knight) {
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

    if active.contains(UsableDevCardKind::YearOfPlenty) {
        for first in Resource::list() {
            for second in Resource::list() {
                candidates.push(DevCardUsage::YearOfPlenty([first, second]));
            }
        }
    }

    if active.contains(UsableDevCardKind::Monopoly) {
        candidates.extend(Resource::list().into_iter().map(DevCardUsage::Monopoly));
    }

    if active.contains(UsableDevCardKind::RoadBuild) {
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

pub fn greedy_regular_action(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> RegularAction {
    if let Some(build) = legal_cities(context, player_id).into_iter().next() {
        return RegularAction::Build(build);
    }
    if let Some(build) = legal_settlements(context, player_id).into_iter().next() {
        return RegularAction::Build(build);
    }
    if can_buy_dev_card(context, player_id) {
        return RegularAction::BuyDevCard;
    }
    if let Some(build) = legal_roads(context, player_id).into_iter().next() {
        return RegularAction::Build(build);
    }
    RegularAction::EndMove
}
