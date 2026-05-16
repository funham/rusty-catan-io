use serde::{Deserialize, Serialize};

use crate::{
    common::SmallSet,
    constants::costs,
    gameplay::game::action::{InitStageAction, RegularAction},
    gameplay::{
        game::view::{PlayerDecisionContext, PublicPlayerResources},
        primitives::{
            PortKind,
            build::{
                BoardBuildData, Build, Establishment, EstablishmentType, PlayerBuildData, Road,
            },
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind},
        },
    },
    topology::{Hex, Path},
};

#[cfg(feature = "bench-counters")]
pub mod counters {
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct LegalCounters {
        pub city_candidates: u64,
        pub settlement_candidates: u64,
        pub road_candidates: u64,
        pub dev_card_candidates: u64,
        pub roadbuild_candidates: u64,
    }

    static CITY_CANDIDATES: AtomicU64 = AtomicU64::new(0);
    static SETTLEMENT_CANDIDATES: AtomicU64 = AtomicU64::new(0);
    static ROAD_CANDIDATES: AtomicU64 = AtomicU64::new(0);
    static DEV_CARD_CANDIDATES: AtomicU64 = AtomicU64::new(0);
    static ROADBUILD_CANDIDATES: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        CITY_CANDIDATES.store(0, Ordering::Relaxed);
        SETTLEMENT_CANDIDATES.store(0, Ordering::Relaxed);
        ROAD_CANDIDATES.store(0, Ordering::Relaxed);
        DEV_CARD_CANDIDATES.store(0, Ordering::Relaxed);
        ROADBUILD_CANDIDATES.store(0, Ordering::Relaxed);
    }

    pub fn snapshot() -> LegalCounters {
        LegalCounters {
            city_candidates: CITY_CANDIDATES.load(Ordering::Relaxed),
            settlement_candidates: SETTLEMENT_CANDIDATES.load(Ordering::Relaxed),
            road_candidates: ROAD_CANDIDATES.load(Ordering::Relaxed),
            dev_card_candidates: DEV_CARD_CANDIDATES.load(Ordering::Relaxed),
            roadbuild_candidates: ROADBUILD_CANDIDATES.load(Ordering::Relaxed),
        }
    }

    pub(super) fn city_candidate() {
        CITY_CANDIDATES.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn settlement_candidate() {
        SETTLEMENT_CANDIDATES.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn road_candidate() {
        ROAD_CANDIDATES.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn dev_card_candidate() {
        DEV_CARD_CANDIDATES.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn roadbuild_candidate() {
        ROADBUILD_CANDIDATES.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildClass {
    Road,
    Settlement,
    City,
}

pub fn legal_initial_placements(context: &PlayerDecisionContext<'_>) -> Vec<InitStageAction> {
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
    legal_city_spots_iter(context, player_id).collect()
}

pub fn legal_city_spots_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Box<dyn Iterator<Item = Build> + 'a> {
    if context.search.is_none() {
        log::debug!("legal city spots require search context");
        return Box::new(std::iter::empty());
    }
    if !context.private.resources.has_enough(&costs::CITY)
        || context.public.builds.by_player(player_id).cities_count() >= PlayerBuildData::CITY_LIMIT
    {
        return Box::new(std::iter::empty());
    }

    Box::new(
        context
            .public
            .builds
            .by_player(player_id)
            .establishments
            .iter()
            .copied()
            .filter(|est| est.stage == EstablishmentType::Settlement)
            .map(|est| {
                #[cfg(feature = "bench-counters")]
                counters::city_candidate();
                Build::Establishment(Establishment {
                    vtx: est.vtx,
                    stage: EstablishmentType::City,
                })
            }),
    )
}

pub fn legal_city_spots_count(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> usize {
    legal_city_spots_count_with_resources(context, player_id, context.private.resources)
}

pub fn legal_city_spots_count_with_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &crate::gameplay::primitives::resource::ResourceCollection,
) -> usize {
    if context.search.is_none() {
        log::debug!("legal city spots require search context");
        return 0;
    }
    if !resources.has_enough(&costs::CITY)
        || context.public.builds.by_player(player_id).cities_count() >= PlayerBuildData::CITY_LIMIT
    {
        return 0;
    }

    context
        .public
        .builds
        .by_player(player_id)
        .establishments
        .iter()
        .copied()
        .filter(|est| est.stage == EstablishmentType::Settlement)
        .inspect(|_| {
            #[cfg(feature = "bench-counters")]
            counters::city_candidate();
        })
        .count()
}

pub fn legal_settlement_spots(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Vec<Build> {
    legal_settlement_spots_iter(context, player_id).collect()
}

pub fn legal_settlement_spots_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Box<dyn Iterator<Item = Build> + 'a> {
    if !can_search_settlement_with_resources(context, player_id, context.private.resources) {
        return Box::new(std::iter::empty());
    }

    Box::new(
        context
            .public
            .board
            .intersections()
            .iter()
            .copied()
            .filter(move |&pos| {
                #[cfg(feature = "bench-counters")]
                counters::settlement_candidate();
                context
                    .public
                    .builds
                    .can_place_settlement(player_id, pos)
                    .is_ok()
            })
            .map(|pos| {
                Build::Establishment(Establishment {
                    vtx: pos,
                    stage: EstablishmentType::Settlement,
                })
            }),
    )
}

pub fn legal_settlement_spots_count(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> usize {
    legal_settlement_spots_count_with_resources(context, player_id, context.private.resources)
}

pub fn legal_settlement_spots_count_with_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &crate::gameplay::primitives::resource::ResourceCollection,
) -> usize {
    if !can_search_settlement_with_resources(context, player_id, resources) {
        return 0;
    }

    context
        .public
        .board
        .intersections()
        .iter()
        .copied()
        .filter(move |&pos| {
            #[cfg(feature = "bench-counters")]
            counters::settlement_candidate();
            context
                .public
                .builds
                .can_place_settlement(player_id, pos)
                .is_ok()
        })
        .count()
}

pub fn legal_settlement_spots_count_with_extra_road(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    extra_road: Path,
    resources: &crate::gameplay::primitives::resource::ResourceCollection,
) -> usize {
    if !can_search_settlement_with_resources(context, player_id, resources) {
        return 0;
    }

    context
        .public
        .board
        .intersections()
        .iter()
        .copied()
        .filter(move |&pos| {
            #[cfg(feature = "bench-counters")]
            counters::settlement_candidate();
            context
                .public
                .builds
                .can_place_settlement_with_extra_roads_iter(player_id, pos, [extra_road])
                .is_ok()
        })
        .count()
}

pub fn legal_road_spots(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Vec<Build> {
    legal_road_spots_iter(context, player_id).collect()
}

pub fn legal_road_spots_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Box<dyn Iterator<Item = Build> + 'a> {
    if !can_search_road_with_resources(context, player_id, context.private.resources) {
        return Box::new(std::iter::empty());
    }

    Box::new(
        context
            .public
            .builds
            .road_extension_candidates(player_id, context.public.board.paths())
            .into_iter()
            .inspect(move |_| {
                #[cfg(feature = "bench-counters")]
                counters::road_candidate();
            })
            .map(|pos| Build::Road(Road { path: pos })),
    )
}

pub fn legal_road_spots_count(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> usize {
    legal_road_spots_count_with_resources(context, player_id, context.private.resources)
}

pub fn legal_road_spots_count_with_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &crate::gameplay::primitives::resource::ResourceCollection,
) -> usize {
    if !can_search_road_with_resources(context, player_id, resources) {
        return 0;
    }

    context
        .public
        .builds
        .road_extension_candidates(player_id, context.public.board.paths())
        .into_iter()
        .inspect(|_| {
            #[cfg(feature = "bench-counters")]
            counters::road_candidate();
        })
        .count()
}

fn can_search_settlement_with_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &crate::gameplay::primitives::resource::ResourceCollection,
) -> bool {
    if context.search.is_none() {
        log::debug!("legal settlement spots require search context");
        return false;
    }
    if player_id >= context.public.builds.players().len() {
        return false;
    }
    if !resources.has_enough(&crate::constants::costs::SETTLEMENT)
        || context
            .public
            .builds
            .by_player(player_id)
            .settlements_count()
            >= PlayerBuildData::SETTLEMENT_LIMIT
    {
        return false;
    }
    true
}

fn can_search_road_with_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &crate::gameplay::primitives::resource::ResourceCollection,
) -> bool {
    if context.search.is_none() {
        log::debug!("legal road spots require search context");
        return false;
    }
    if player_id >= context.public.builds.players().len() {
        return false;
    }
    if !resources.has_enough(&crate::constants::costs::ROAD)
        || context.public.builds.by_player(player_id).roads_count() >= PlayerBuildData::ROAD_LIMIT
    {
        return false;
    }
    true
}

pub fn can_buy_dev_card(context: &PlayerDecisionContext<'_>) -> bool {
    context.public.bank.dev_card_count > 0
        && context
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
    legal_dev_card_usages_iter(context).collect()
}

pub fn legal_dev_card_usages_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
) -> Box<dyn Iterator<Item = DevCardUsage> + 'a> {
    let Some(search) = &context.search else {
        log::debug!("legal development card usages require search context");
        return Box::new(std::iter::empty());
    };

    let state = search.state();
    let active = context.private.dev_cards.active;

    let knight = active
        .contains(UsableDevCard::Knight)
        .then(move || {
            context
                .public
                .board
                .arrangement
                .hex_iter()
                .filter(|rob_hex| *rob_hex != context.public.board_state.robber_pos)
                .flat_map(|rob_hex| {
                    let robbed_candidates = legal_rob_targets(context, rob_hex);
                    if robbed_candidates.is_empty() {
                        vec![DevCardUsage::Knight {
                            rob_hex,
                            robbed_id: None,
                        }]
                    } else {
                        robbed_candidates
                            .into_iter()
                            .map(|robbed_id| DevCardUsage::Knight {
                                rob_hex,
                                robbed_id: Some(robbed_id),
                            })
                            .collect()
                    }
                })
        })
        .into_iter()
        .flatten();

    let year_of_plenty = active
        .contains(UsableDevCard::YearOfPlenty)
        .then(move || {
            Resource::iter().flat_map(move |first| {
                Resource::iter().filter_map(move |second| {
                    let requested = [first, second].into_iter().fold(
                        ResourceCollection::default(),
                        |mut acc, resource| {
                            acc += &resource.into();
                            acc
                        },
                    );
                    state
                        .bank
                        .can_pay(&requested)
                        .then_some(DevCardUsage::YearOfPlenty([first, second]))
                })
            })
        })
        .into_iter()
        .flatten();

    let monopoly = active
        .contains(UsableDevCard::Monopoly)
        .then(|| Resource::iter().map(DevCardUsage::Monopoly))
        .into_iter()
        .flatten();

    let roadbuild = active
        .contains(UsableDevCard::RoadBuild)
        .then(|| legal_roadbuild_usages_iter(context))
        .into_iter()
        .flatten();

    Box::new(
        knight
            .chain(year_of_plenty)
            .chain(monopoly)
            .chain(roadbuild)
            .inspect(|_| {
                #[cfg(feature = "bench-counters")]
                counters::dev_card_candidate();
            }),
    )
}

pub fn first_legal_dev_card_usage(context: &PlayerDecisionContext<'_>) -> Option<DevCardUsage> {
    let search = context.search.as_ref()?;
    let state = search.state();
    let active = context.private.dev_cards.active;

    if active.contains(UsableDevCard::Knight) {
        for rob_hex in context.public.board.arrangement.hex_iter() {
            if rob_hex == context.public.board_state.robber_pos {
                continue;
            }

            let robbed_candidates = legal_rob_targets(context, rob_hex);
            return Some(match robbed_candidates.first() {
                Some(robbed_id) => DevCardUsage::Knight {
                    rob_hex,
                    robbed_id: Some(*robbed_id),
                },
                None => DevCardUsage::Knight {
                    rob_hex,
                    robbed_id: None,
                },
            });
        }
    }

    if active.contains(UsableDevCard::YearOfPlenty) {
        for first in Resource::iter() {
            for second in Resource::iter() {
                let requested = [first, second].into_iter().fold(
                    ResourceCollection::default(),
                    |mut acc, resource| {
                        acc += &resource.into();
                        acc
                    },
                );
                if state.bank.can_pay(&requested) {
                    return Some(DevCardUsage::YearOfPlenty([first, second]));
                }
            }
        }
    }

    if active.contains(UsableDevCard::Monopoly)
        && let Some(resource) = Resource::iter().into_iter().next()
    {
        return Some(DevCardUsage::Monopoly(resource));
    }

    if active.contains(UsableDevCard::RoadBuild) {
        return legal_roadbuild_usages_iter(context).next();
    }

    None
}

pub fn legal_roadbuild_usages_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
) -> impl Iterator<Item = DevCardUsage> + 'a {
    legal_k_road_extensions::<2>(
        context.public.builds,
        context.actor,
        context.public.board.paths(),
    )
    .map(DevCardUsage::RoadBuild)
}

pub fn legal_k_road_extensions<'a, const K: usize>(
    builds: &'a BoardBuildData,
    player_id: PlayerId,
    board_paths: &'a [Path],
) -> RoadExtensionIter<'a, K> {
    RoadExtensionIter::new(builds, player_id, board_paths)
}

pub struct RoadExtensionIter<'a, const K: usize> {
    builds: &'a BoardBuildData,
    player_id: PlayerId,
    board_paths: &'a [Path],
    chosen: [Option<Path>; K],
    next_indices: [usize; K],
    depth: usize,
    done: bool,
}

impl<'a, const K: usize> RoadExtensionIter<'a, K> {
    fn new(builds: &'a BoardBuildData, player_id: PlayerId, board_paths: &'a [Path]) -> Self {
        Self {
            builds,
            player_id,
            board_paths,
            chosen: [None; K],
            next_indices: [0; K],
            depth: 0,
            done: false,
        }
    }
}

impl<const K: usize> Iterator for RoadExtensionIter<'_, K> {
    type Item = [Path; K];

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            if self.depth == K {
                let result = std::array::from_fn(|idx| {
                    self.chosen[idx].expect("complete road extension should have every path")
                });
                if self.depth == 0 {
                    self.done = true;
                } else {
                    self.depth -= 1;
                    self.chosen[self.depth] = None;
                }
                return Some(result);
            }

            let prefix = self.chosen[..self.depth]
                .iter()
                .copied()
                .flatten()
                .collect::<Vec<_>>();
            let candidates = self
                .builds
                .road_extension_candidates_with_extra_roads(
                    self.player_id,
                    prefix.clone(),
                    self.board_paths,
                )
                .into_iter()
                .collect::<Vec<_>>();

            if self.next_indices[self.depth] >= candidates.len() {
                self.next_indices[self.depth] = 0;
                if self.depth == 0 {
                    self.done = true;
                    return None;
                }
                self.depth -= 1;
                self.chosen[self.depth] = None;
                continue;
            }

            let candidate = candidates[self.next_indices[self.depth]];
            self.next_indices[self.depth] += 1;

            #[cfg(feature = "bench-counters")]
            counters::roadbuild_candidate();

            self.chosen[self.depth] = Some(candidate);
            self.depth += 1;
            if self.depth < K {
                self.next_indices[self.depth] = 0;
            }
        }
    }
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
    legal_bank_trades_iter(context).collect()
}

pub fn legal_bank_trades_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
) -> Box<dyn Iterator<Item = BankTrade> + 'a> {
    let generic =
        resource_trades_at_rate_iter(context, BankTradeKind::BankGeneric, Resource::iter(), 4);
    let port_trades = context
        .public
        .ports_acquired_for(context.actor)
        .iter()
        .map(move |port| -> Box<dyn Iterator<Item = BankTrade> + 'a> {
            match port {
                PortKind::Special(resource) => resource_trades_at_rate_iter(
                    context,
                    BankTradeKind::PortSpecific,
                    std::iter::once(*resource),
                    2,
                ),
                PortKind::Universal => resource_trades_at_rate_iter(
                    context,
                    BankTradeKind::PortGeneric,
                    Resource::iter(),
                    3,
                ),
            }
        })
        .flatten();

    Box::new(generic.chain(port_trades))
}

pub fn legal_bank_trade_count(context: &PlayerDecisionContext<'_>) -> usize {
    let mut count = resource_trades_count_at_rate(context, Resource::iter(), 4);

    for port in context.public.ports_acquired_for(context.actor) {
        count += match port {
            PortKind::Special(resource) => {
                resource_trades_count_at_rate(context, std::iter::once(*resource), 2)
            }
            PortKind::Universal => resource_trades_count_at_rate(context, Resource::iter(), 3),
        };
    }

    count
}

pub fn legal_bank_trade_at(
    context: &PlayerDecisionContext<'_>,
    mut index: usize,
) -> Option<BankTrade> {
    if let Some(trade) = resource_trade_at_rate(
        context,
        BankTradeKind::BankGeneric,
        Resource::iter(),
        4,
        &mut index,
    ) {
        return Some(trade);
    }

    for port in context.public.ports_acquired_for(context.actor) {
        let trade = match port {
            PortKind::Special(resource) => resource_trade_at_rate(
                context,
                BankTradeKind::PortSpecific,
                std::iter::once(*resource),
                2,
                &mut index,
            ),
            PortKind::Universal => resource_trade_at_rate(
                context,
                BankTradeKind::PortGeneric,
                Resource::iter(),
                3,
                &mut index,
            ),
        };
        if trade.is_some() {
            return trade;
        }
    }

    None
}

fn resource_trades_at_rate_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
    kind: BankTradeKind,
    give_candidates: impl IntoIterator<Item = Resource> + 'a,
    rate: u16,
) -> Box<dyn Iterator<Item = BankTrade> + 'a> {
    Box::new(
        give_candidates
            .into_iter()
            .filter(move |give| context.private.resources.has_enough(&(*give, rate).into()))
            .flat_map(move |give| {
                Resource::iter()
                    .filter(move |take| *take != give)
                    .map(move |take| BankTrade { give, take, kind })
            }),
    )
}

fn resource_trades_count_at_rate(
    context: &PlayerDecisionContext<'_>,
    give_candidates: impl IntoIterator<Item = Resource>,
    rate: u16,
) -> usize {
    give_candidates
        .into_iter()
        .filter(|give| context.private.resources.has_enough(&(*give, rate).into()))
        .map(|_| Resource::ALL.len() - 1)
        .sum()
}

fn resource_trade_at_rate(
    context: &PlayerDecisionContext<'_>,
    kind: BankTradeKind,
    give_candidates: impl IntoIterator<Item = Resource>,
    rate: u16,
    index: &mut usize,
) -> Option<BankTrade> {
    for give in give_candidates
        .into_iter()
        .filter(|give| context.private.resources.has_enough(&(*give, rate).into()))
    {
        for take in Resource::iter().filter(|take| *take != give) {
            if *index == 0 {
                return Some(BankTrade { give, take, kind });
            }
            *index -= 1;
        }
    }

    None
}

pub fn legal_trades(context: &PlayerDecisionContext<'_>) -> impl IntoIterator<Item = BankTrade> {
    legal_bank_trades(context)
}

pub fn legal_regular_actions(context: &PlayerDecisionContext<'_>) -> Vec<RegularAction> {
    legal_regular_actions_iter(context).collect()
}

pub fn legal_regular_action_count(context: &PlayerDecisionContext<'_>) -> usize {
    let mut count = 1;

    if can_buy_dev_card(context) {
        count += 1;
    }
    if can_buy_road(context) {
        count += legal_road_spots_count(context, context.actor);
    }
    if can_buy_settlement(context) {
        count += legal_settlement_spots_count(context, context.actor);
    }
    if can_buy_city(context) {
        count += legal_city_spots_count(context, context.actor);
    }
    count += legal_bank_trade_count(context);

    count
}

pub fn legal_regular_action_at(
    context: &PlayerDecisionContext<'_>,
    mut index: usize,
) -> Option<RegularAction> {
    if index == 0 {
        return Some(RegularAction::EndMove);
    }
    index -= 1;

    if can_buy_dev_card(context) {
        if index == 0 {
            return Some(RegularAction::BuyDevCard);
        }
        index -= 1;
    }

    if can_buy_road(context) {
        let count = legal_road_spots_count(context, context.actor);
        if index < count {
            return legal_road_spots_iter(context, context.actor)
                .nth(index)
                .map(RegularAction::Build);
        }
        index -= count;
    }

    if can_buy_settlement(context) {
        let count = legal_settlement_spots_count(context, context.actor);
        if index < count {
            return legal_settlement_spots_iter(context, context.actor)
                .nth(index)
                .map(RegularAction::Build);
        }
        index -= count;
    }

    if can_buy_city(context) {
        let count = legal_city_spots_count(context, context.actor);
        if index < count {
            return legal_city_spots_iter(context, context.actor)
                .nth(index)
                .map(RegularAction::Build);
        }
        index -= count;
    }

    legal_bank_trade_at(context, index).map(RegularAction::TradeWithBank)
}

pub fn legal_regular_actions_iter<'a>(
    context: &'a PlayerDecisionContext<'_>,
) -> Box<dyn Iterator<Item = RegularAction> + 'a> {
    let buy_dev = can_buy_dev_card(context)
        .then_some(RegularAction::BuyDevCard)
        .into_iter();
    let roads = can_buy_road(context)
        .then(|| legal_road_spots_iter(context, context.actor))
        .into_iter()
        .flatten()
        .map(RegularAction::Build);
    let settlements = can_buy_settlement(context)
        .then(|| legal_settlement_spots_iter(context, context.actor))
        .into_iter()
        .flatten()
        .map(RegularAction::Build);
    let cities = can_buy_city(context)
        .then(|| legal_city_spots_iter(context, context.actor))
        .into_iter()
        .flatten()
        .map(RegularAction::Build);
    let bank_trades = legal_bank_trades(context)
        .into_iter()
        .map(RegularAction::TradeWithBank);

    Box::new(
        std::iter::once(RegularAction::EndMove)
            .chain(buy_dev)
            .chain(roads)
            .chain(settlements)
            .chain(cities)
            .chain(bank_trades),
    )
}

#[derive(Debug, Default, Clone)]
pub struct TradeFilter {
    pub give: SmallSet<Resource, 5>,
    pub take: SmallSet<Resource, 5>,
    pub kind: SmallSet<BankTradeKind, 3>,
}

pub fn list_trades(
    show: Option<TradeFilter>,
    hide: Option<TradeFilter>,
) -> impl Iterator<Item = BankTrade> {
    use BankTradeKind::*;

    fn allowed_give(
        give: Resource,
        show: Option<&TradeFilter>,
        hide: Option<&TradeFilter>,
    ) -> bool {
        !matches!(
            show,
            Some(white) if !white.give.is_empty() && !white.give.contains(&give)
        ) && !matches!(
            hide,
            Some(black) if black.give.contains(&give)
        )
    }

    fn allowed_take(
        take: Resource,
        give: Resource,
        show: Option<&TradeFilter>,
        hide: Option<&TradeFilter>,
    ) -> bool {
        take != give
            && !matches!(
                show,
                Some(white) if !white.take.is_empty() && !white.take.contains(&take)
            )
            && !matches!(
                hide,
                Some(black) if black.take.contains(&take)
            )
    }

    fn allowed_kind(
        kind: BankTradeKind,
        white: Option<&TradeFilter>,
        black: Option<&TradeFilter>,
    ) -> bool {
        !matches!(
            white,
            Some(white) if !white.kind.is_empty() && !white.kind.contains(&kind)
        ) && !matches!(
            black,
            Some(black) if black.kind.contains(&kind)
        )
    }

    let mut give_iter = Resource::iter();
    let mut take_iter = Resource::iter();
    let mut kind_iter = [BankGeneric, PortGeneric, PortSpecific].into_iter();

    let mut give = None;
    let mut take = None;

    std::iter::from_fn(move || {
        loop {
            if let (Some(g), Some(t)) = (give, take) {
                for kind in kind_iter.by_ref() {
                    if allowed_kind(kind, show.as_ref(), hide.as_ref()) {
                        return Some(BankTrade {
                            give: g,
                            take: t,
                            kind,
                        });
                    }
                }
            }

            loop {
                if let Some(g) = give {
                    for t in take_iter.by_ref() {
                        if allowed_take(t, g, show.as_ref(), hide.as_ref()) {
                            take = Some(t);
                            kind_iter = [BankGeneric, PortGeneric, PortSpecific].into_iter();
                            break;
                        }
                    }

                    if take.is_some() {
                        break;
                    }
                }

                give = give_iter
                    .by_ref()
                    .find(|&g| allowed_give(g, show.as_ref(), hide.as_ref()));

                give?;

                take_iter = Resource::iter();
                take = None;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::gameplay::{
        game::{
            index::GameIndex,
            init::GameInitializationState,
            state::GameState,
            view::{ContextFactory, PlayerDecisionContext, SearchFactory, VisibilityConfig},
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
            .expect("default board should have initial placements")
            .as_builds();
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
            if candidate
                .try_build(0, Build::Road(Road { path: pos }))
                .is_err()
            {
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
                            vtx: pos,
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

    fn with_decision_context<T>(
        state: &GameState,
        player_id: PlayerId,
        f: impl FnOnce(PlayerDecisionContext<'_>) -> T,
    ) -> T {
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
        f(factory.player_decision_context(player_id, search))
    }

    fn clone_apply_city_spots(
        context: &PlayerDecisionContext<'_>,
        player_id: PlayerId,
    ) -> Vec<Build> {
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
                    vtx: est.vtx,
                    stage: EstablishmentType::City,
                })
            })
            .filter(|build| {
                let mut state = seed.state.clone();
                state.build(player_id, *build).is_ok()
            })
            .collect()
    }

    fn clone_apply_settlement_spots(
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
                    vtx: pos,
                    stage: EstablishmentType::Settlement,
                })
            })
            .filter(|build| {
                let mut state = seed.state.clone();
                state.build(player_id, *build).is_ok()
            })
            .collect()
    }

    fn clone_apply_road_spots(
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
            .paths()
            .into_iter()
            .map(|pos| Build::Road(Road { path: pos }))
            .filter(|build| {
                let mut state = seed.state.clone();
                state.build(player_id, *build).is_ok()
            })
            .collect()
    }

    fn sorted_debug(builds: Vec<Build>) -> Vec<String> {
        let mut values = builds
            .into_iter()
            .map(|build| format!("{build:?}"))
            .collect::<Vec<_>>();
        values.sort();
        values
    }

    fn sorted_regular_debug(actions: Vec<RegularAction>) -> Vec<String> {
        let mut values = actions
            .into_iter()
            .map(|action| format!("{action:?}"))
            .collect::<Vec<_>>();
        values.sort();
        values
    }

    fn sorted_dev_usage_debug(usages: Vec<DevCardUsage>) -> Vec<String> {
        let mut values = usages
            .into_iter()
            .map(|usage| format!("{usage:?}"))
            .collect::<Vec<_>>();
        values.sort();
        values
    }

    #[test]
    fn direct_legal_build_spots_match_clone_apply_generation() {
        let mut state = initialized_state();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 5,
                    wood: 5,
                    wheat: 5,
                    sheep: 5,
                    ore: 5,
                },
                0,
            )
            .expect("bank should fund test player");

        with_decision_context(&state, 0, |context| {
            assert_eq!(
                sorted_debug(legal_city_spots(&context, 0)),
                sorted_debug(clone_apply_city_spots(&context, 0))
            );
            assert_eq!(
                sorted_debug(legal_settlement_spots(&context, 0)),
                sorted_debug(clone_apply_settlement_spots(&context, 0))
            );
            assert_eq!(
                sorted_debug(legal_road_spots(&context, 0)),
                sorted_debug(clone_apply_road_spots(&context, 0))
            );
            assert_eq!(
                legal_city_spots_count(&context, 0),
                clone_apply_city_spots(&context, 0).len()
            );
            assert_eq!(
                legal_settlement_spots_count(&context, 0),
                clone_apply_settlement_spots(&context, 0).len()
            );
            assert_eq!(
                legal_road_spots_count(&context, 0),
                clone_apply_road_spots(&context, 0).len()
            );
        });
    }

    #[test]
    fn lazy_regular_action_iterator_matches_eager_actions() {
        let mut state = initialized_state();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 5,
                    wood: 5,
                    wheat: 5,
                    sheep: 5,
                    ore: 5,
                },
                0,
            )
            .expect("bank should fund test player");

        with_decision_context(&state, 0, |context| {
            assert_eq!(
                sorted_regular_debug(legal_regular_actions(&context)),
                sorted_regular_debug(legal_regular_actions_iter(&context).collect())
            );
        });
    }

    #[test]
    fn indexed_regular_action_lookup_matches_eager_order() {
        let mut state = initialized_state();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 5,
                    wood: 5,
                    wheat: 5,
                    sheep: 5,
                    ore: 5,
                },
                0,
            )
            .expect("bank should fund test player");

        with_decision_context(&state, 0, |context| {
            let expected = legal_regular_actions(&context);

            assert_eq!(legal_regular_action_count(&context), expected.len());
            for (index, expected_action) in expected.into_iter().enumerate() {
                assert_eq!(
                    legal_regular_action_at(&context, index).map(|action| format!("{action:?}")),
                    Some(format!("{expected_action:?}")),
                    "index {index}"
                );
            }
            assert!(legal_regular_action_at(&context, usize::MAX).is_none());
        });
    }

    #[test]
    fn lazy_dev_card_usage_iterator_matches_eager_usages() {
        let mut state = initialized_state();
        for kind in [
            UsableDevCard::Knight,
            UsableDevCard::YearOfPlenty,
            UsableDevCard::RoadBuild,
            UsableDevCard::Monopoly,
        ] {
            state
                .players
                .get_mut(0)
                .dev_cards_add(DevCardKind::Usable(kind));
        }
        state.players.get_mut(0).dev_cards_reset_queue();

        with_decision_context(&state, 0, |context| {
            assert_eq!(
                sorted_dev_usage_debug(legal_dev_card_usages(&context)),
                sorted_dev_usage_debug(legal_dev_card_usages_iter(&context).collect())
            );
        });
    }

    #[test]
    fn lazy_bank_trade_iterator_matches_eager_trades() {
        let state = state_with_port_and_resources(
            PortKind::Universal,
            ResourceCollection {
                brick: 4,
                wood: 3,
                wheat: 2,
                ..ResourceCollection::ZERO
            },
        );

        with_decision_context(&state, 0, |context| {
            let eager = legal_bank_trades(&context)
                .into_iter()
                .map(|trade| format!("{trade:?}"))
                .collect::<Vec<_>>();
            let lazy = legal_bank_trades_iter(&context)
                .map(|trade| format!("{trade:?}"))
                .collect::<Vec<_>>();

            assert_eq!(lazy, eager);
        });
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
    fn can_buy_city_requires_city_resources() {
        let mut state = initialized_state();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 1,
                    wood: 1,
                    wheat: 1,
                    sheep: 1,
                    ore: 0,
                },
                0,
            )
            .expect("bank should fund settlement-cost resources");

        with_decision_context(&state, 0, |context| {
            assert!(!can_buy_city(&context));
            assert!(legal_city_spots(&context, 0).is_empty());
        });
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
    fn legal_actions_exclude_dev_card_when_bank_deck_is_empty() {
        let mut state = initialized_state();
        state.bank.dev_cards.clear();
        state
            .transfer_from_bank(
                ResourceCollection {
                    wheat: 1,
                    sheep: 1,
                    ore: 1,
                    ..ResourceCollection::ZERO
                },
                0,
            )
            .expect("bank should fund test player");

        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);
        let actions = legal_regular_actions(&context);

        assert!(!can_buy_dev_card(&context));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, RegularAction::BuyDevCard)),
            "empty dev-card deck should not produce BuyDevCard, got {actions:?}"
        );
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
            .expect("default board should have an initial placement")
            .as_builds();
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
            .map(|action| action.settlement_pos())
            .collect::<std::collections::BTreeSet<_>>();

        assert!(!legal.contains(&settlement.vtx));
        for neighbor in settlement.vtx.neighbors() {
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
        assert!(
            placements
                .iter()
                .map(InitStageAction::as_builds)
                .all(|(settlement, road)| {
                    road.path
                        .intersections_iter()
                        .any(|intersection| intersection == settlement.vtx)
                })
        );
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
            .map(|pos| Road { path: pos })
            .expect("port settlement should have adjacent road");
        init.builds
            .try_init_place(
                0,
                road,
                Establishment {
                    vtx: settlement_pos,
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
            let mut rng = crate::gameplay::random::GameRandom::seeded(42);
            assert!(
                rng.with_rng(|rng| candidate.use_dev_card_with_rng(usage, 0, rng))
                    .is_ok(),
                "legal roadbuild usage should be accepted: {usage:?}"
            );
        }
    }

    #[test]
    fn lazy_roadbuild_iterator_matches_clone_apply_order() {
        let state = initialized_state();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);
        let paths = context.public.board.paths();

        let mut expected = Vec::new();
        for first in paths.iter().copied() {
            let mut builds_after_first = state.builds.clone();
            if builds_after_first
                .try_build(0, Build::Road(Road { path: first }))
                .is_err()
            {
                continue;
            }

            for second in paths.iter().copied() {
                let mut builds_after_second = builds_after_first.clone();
                if builds_after_second
                    .try_build(0, Build::Road(Road { path: second }))
                    .is_ok()
                {
                    expected.push([first, second]);
                }
            }
        }

        let actual =
            legal_k_road_extensions::<2>(context.public.builds, 0, paths).collect::<Vec<_>>();

        assert_eq!(actual, expected);
    }
}
