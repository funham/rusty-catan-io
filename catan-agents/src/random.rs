use crate::{lazy, legal};
use catan_core::{
    agent::{
        action::{
            ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
        agent::PlayerRuntime,
    },
    gameplay::{
        game::{event::PlayerNotification, view::PlayerDecisionContext},
        primitives::{
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
        },
    },
    topology::{Hex, HexIndex, Path},
};
use rand::{
    Rng, RngExt, SeedableRng,
    rngs::SmallRng,
    seq::{IndexedRandom, IteratorRandom},
};

#[derive(Debug)]
pub struct RandomAgent<R = SmallRng> {
    id: PlayerId,
    rng: R,
}

impl Default for RandomAgent {
    fn default() -> Self {
        Self::new(0)
    }
}

impl RandomAgent {
    pub fn new(id: PlayerId) -> Self {
        Self::with_rng(id, SmallRng::from_rng(&mut rand::rng()))
    }

    pub fn with_seed(id: PlayerId, seed: u64) -> Self {
        Self::with_rng(id, SmallRng::seed_from_u64(seed))
    }
}

impl<R> RandomAgent<R> {
    pub fn with_rng(id: PlayerId, rng: R) -> Self {
        Self { id, rng }
    }
}

impl<R> PlayerNotification for RandomAgent<R> {}

impl<R: Rng> PlayerRuntime for RandomAgent<R> {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        rand_init_stage_action(context, &mut self.rng)
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        rand_init_action(context, &mut self.rng)
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        rand_after_dice_action(context, &mut self.rng)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        rand_regular_action(context, &mut self.rng)
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        rand_move_robbers(context, &mut self.rng)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        rand_choose_player_to_rob(context, robber_pos, &mut self.rng)
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Decline
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        rand_drop_half(context, &mut self.rng)
    }
}

pub fn rand_init_stage_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> InitStageAction {
    let (establishment, road) = context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, context.actor)
        .choose(rng)
        .unwrap()
        .clone();

    InitStageAction {
        establishment_position: establishment.pos,
        road,
    }
}

pub fn rand_init_action(context: PlayerDecisionContext<'_>, rng: &mut impl Rng) -> InitAction {
    if rng.random_bool(0.8)
        && let Some(usage) = rand_dev_card_usage(&context, rng)
    {
        return InitAction::UseDevCard(usage);
    }

    InitAction::RollDice
}

pub fn rand_after_dice_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> PostDiceAction {
    if rng.random_bool(0.8)
        && let Some(usage) = rand_dev_card_usage(&context, rng)
    {
        return PostDiceAction::UseDevCard(usage);
    }

    PostDiceAction::RegularAction(rand_regular_action(context, rng))
}

pub fn rand_regular_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> RegularAction {
    let categories = [
        RandomRegularActionCategory::EndMove,
        RandomRegularActionCategory::BuyDevCard,
        RandomRegularActionCategory::BuildRoad,
        RandomRegularActionCategory::BuildSettlement,
        RandomRegularActionCategory::BuildCity,
        RandomRegularActionCategory::TradeWithBank,
    ];
    let start = rng.random_range(0..categories.len());

    for offset in 0..categories.len() {
        let category = categories[(start + offset) % categories.len()];
        if let Some(action) = rand_regular_action_in_category(&context, category, rng) {
            return action;
        }
    }

    RegularAction::EndMove
}

pub fn rand_move_robbers(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> MoveRobbersAction {
    let n = context.public.board.arrangement.len();
    let tile_index = match rng.random_range(0..n - 1) {
        index if index == context.public.board_state.robber_pos.index().to_spiral() => n - 1,
        index => index,
    };

    MoveRobbersAction(HexIndex::spiral_to_hex(tile_index))
}

pub fn rand_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
    rng: &mut impl Rng,
) -> ChoosePlayerToRobAction {
    let id = context
        .public
        .players_on_hex(robber_pos)
        .into_iter()
        .filter(|id| *id != context.actor)
        .choose(rng)
        .expect("controller must forbid this case");

    ChoosePlayerToRobAction(id)
}

pub fn rand_answer_trade(_context: PlayerDecisionContext<'_>) -> TradeAnswer {
    TradeAnswer::Decline
}

#[derive(Debug, Clone, Copy)]
enum RandomRegularActionCategory {
    EndMove,
    BuyDevCard,
    BuildRoad,
    BuildSettlement,
    BuildCity,
    TradeWithBank,
}

fn rand_regular_action_in_category(
    context: &PlayerDecisionContext<'_>,
    category: RandomRegularActionCategory,
    rng: &mut impl Rng,
) -> Option<RegularAction> {
    match category {
        RandomRegularActionCategory::EndMove => Some(RegularAction::EndMove),
        RandomRegularActionCategory::BuyDevCard => {
            legal::can_buy_dev_card(context).then_some(RegularAction::BuyDevCard)
        }
        RandomRegularActionCategory::BuildRoad => {
            let count = legal::legal_road_spots_count(context, context.actor);
            (count > 0).then(|| {
                legal::legal_road_spots_iter(context, context.actor)
                    .nth(rng.random_range(0..count))
                    .map(RegularAction::Build)
                    .expect("selected road index must be legal")
            })
        }
        RandomRegularActionCategory::BuildSettlement => {
            let count = legal::legal_settlement_spots_count(context, context.actor);
            (count > 0).then(|| {
                legal::legal_settlement_spots_iter(context, context.actor)
                    .nth(rng.random_range(0..count))
                    .map(RegularAction::Build)
                    .expect("selected settlement index must be legal")
            })
        }
        RandomRegularActionCategory::BuildCity => {
            let count = legal::legal_city_spots_count(context, context.actor);
            (count > 0).then(|| {
                legal::legal_city_spots_iter(context, context.actor)
                    .nth(rng.random_range(0..count))
                    .map(RegularAction::Build)
                    .expect("selected city index must be legal")
            })
        }
        RandomRegularActionCategory::TradeWithBank => {
            let count = legal::legal_bank_trade_count(context);
            (count > 0).then(|| {
                legal::legal_bank_trade_at(context, rng.random_range(0..count))
                    .map(RegularAction::TradeWithBank)
                    .expect("selected bank trade index must be legal")
            })
        }
    }
}

fn rand_dev_card_usage(
    context: &PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> Option<DevCardUsage> {
    let active = context.private.dev_cards.active;
    let kinds = [
        UsableDevCard::Knight,
        UsableDevCard::YearOfPlenty,
        UsableDevCard::RoadBuild,
        UsableDevCard::Monopoly,
    ];
    let start = rng.random_range(0..kinds.len());

    for offset in 0..kinds.len() {
        let kind = kinds[(start + offset) % kinds.len()];
        if !active.contains(kind) {
            continue;
        }
        if let Some(usage) = rand_dev_card_usage_of_kind(context, kind, rng) {
            return Some(usage);
        }
    }

    None
}

fn rand_dev_card_usage_of_kind(
    context: &PlayerDecisionContext<'_>,
    kind: UsableDevCard,
    rng: &mut impl Rng,
) -> Option<DevCardUsage> {
    match kind {
        UsableDevCard::Knight => context
            .public
            .board
            .arrangement
            .hex_iter()
            .filter(|rob_hex| *rob_hex != context.public.board_state.robber_pos)
            .flat_map(|rob_hex| {
                let robbed_candidates = legal::legal_rob_targets(context, rob_hex);
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
            .choose(rng),
        UsableDevCard::YearOfPlenty => {
            let bank = &context.search.as_ref()?.state().bank;
            Resource::iter()
                .flat_map(|first| {
                    Resource::iter().filter_map(move |second| {
                        let requested = [first, second].into_iter().fold(
                            ResourceCollection::default(),
                            |mut acc, resource| {
                                acc += &resource.into();
                                acc
                            },
                        );
                        bank.can_pay(&requested)
                            .then_some(DevCardUsage::YearOfPlenty([first, second]))
                    })
                })
                .choose(rng)
        }
        UsableDevCard::RoadBuild => rand_roadbuild_usage(context, rng),
        UsableDevCard::Monopoly => Resource::iter().map(DevCardUsage::Monopoly).choose(rng),
    }
}

fn rand_roadbuild_usage(
    context: &PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> Option<DevCardUsage> {
    let first = rand_road_extension_with_extra_roads(context, [], rng)?;
    let second = rand_road_extension_with_extra_roads(context, [first], rng)?;
    Some(DevCardUsage::RoadBuild([first, second]))
}

fn rand_road_extension_with_extra_roads<const N: usize>(
    context: &PlayerDecisionContext<'_>,
    extra_roads: [Path; N],
    rng: &mut impl Rng,
) -> Option<Path> {
    let count = context
        .public
        .board
        .paths()
        .iter()
        .copied()
        .filter(|path| {
            context
                .public
                .builds
                .can_place_road_with_extra_roads(context.actor, *path, &extra_roads)
                .is_ok()
        })
        .count();

    if count == 0 {
        return None;
    }

    let index = rng.random_range(0..count);
    context
        .public
        .board
        .paths()
        .iter()
        .copied()
        .filter(|path| {
            context
                .public
                .builds
                .can_place_road_with_extra_roads(context.actor, *path, &extra_roads)
                .is_ok()
        })
        .nth(index)
}

pub fn rand_drop_half(context: PlayerDecisionContext<'_>, _rng: &mut impl Rng) -> DropHalfAction {
    let number_to_drop = context.private.resources.total() / 2;

    match context.search {
        Some(search) => {
            let mut to_drop = ResourceCollection::default();
            let search = search.make_owned();
            let mut res = search
                .state
                .players
                .get(search.root_player)
                .resources()
                .clone();

            for _ in 0..number_to_drop {
                let card = res
                    .pop_random()
                    .expect(&format!("must contain {} cards", number_to_drop));
                to_drop[card] += 1;
            }

            DropHalfAction(to_drop)
        }
        None => {
            log::error!("couldn't find search context for random agent");
            lazy::lazy_drop_half(context)
        }
    }
}
