use crate::{
    bot::{BotPolicy, unsupported_decision_command},
    lazy, legal,
};
use catan_core::{
    gameplay::{
        game::{
            command::{
                ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
                MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
            },
            decision::{DecisionKind, OpenDecision},
            input::PlayerCommand,
            view::PlayerDecisionContext,
        },
        primitives::{
            dev_card::{DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind},
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
        Self::new(PlayerId::new(0))
    }
}

impl RandomAgent {
    pub fn new(id: impl Into<PlayerId>) -> Self {
        let id = id.into();
        Self::with_rng(id, SmallRng::from_rng(&mut rand::rng()))
    }

    pub fn with_seed(id: impl Into<PlayerId>, seed: u64) -> Self {
        let id = id.into();
        Self::with_rng(id, SmallRng::seed_from_u64(seed))
    }
}

impl<R> RandomAgent<R> {
    pub fn with_rng(id: impl Into<PlayerId>, rng: R) -> Self {
        let id = id.into();
        Self { id, rng }
    }
}

impl<R: Rng> RandomAgent<R> {
    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitialPlacementCommand {
        rand_init_stage_action(context, &mut self.rng)
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitCommand {
        rand_init_action(context, &mut self.rng)
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceCommand {
        rand_after_dice_action(context, &mut self.rng)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardCommand {
        PostDevCardCommand::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularCommand {
        rand_regular_action(context, &mut self.rng)
    }

    fn move_robber(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
        rand_move_robber(context, &mut self.rng)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChooseRobbedPlayerCommand {
        rand_choose_player_to_rob(context, robber_pos, &mut self.rng)
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfCommand {
        rand_drop_half(context, &mut self.rng)
    }
}

impl<R: Rng> BotPolicy for RandomAgent<R> {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn command_for(
        &mut self,
        decision: &OpenDecision,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match decision.kind {
            DecisionKind::InitPlacement => Some(PlayerCommand::InitialPlacement(
                self.init_stage_action(context),
            )),
            DecisionKind::InitCommand => {
                Some(PlayerCommand::InitCommand(self.init_action(context)))
            }
            DecisionKind::PostDiceCommand => {
                Some(PlayerCommand::PostDice(self.after_dice_action(context)))
            }
            DecisionKind::PostDevCardCommand => Some(PlayerCommand::PostDevCard(
                self.after_dev_card_action(context),
            )),
            DecisionKind::RegularCommand => {
                Some(PlayerCommand::Regular(self.regular_action(context)))
            }
            DecisionKind::MoveRobber => Some(PlayerCommand::MoveRobber(self.move_robber(context))),
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(
                PlayerCommand::ChooseRobbedPlayer(self.choose_player_to_rob(context, robber_pos)),
            ),
            DecisionKind::DropHalf { .. } => Some(PlayerCommand::DropHalf(self.drop_half(context))),
            DecisionKind::TradeResponse { .. } | DecisionKind::TradeOwnerAction { .. } => {
                unsupported_decision_command(decision.kind)
            }
        }
    }
}

pub fn rand_init_stage_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> InitialPlacementCommand {
    *context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, context.actor)
        .choose(rng)
        .unwrap()
}

pub fn rand_init_action(context: PlayerDecisionContext<'_>, rng: &mut impl Rng) -> InitCommand {
    if rng.random_bool(0.8)
        && let Some(usage) = rand_dev_card_usage(&context, rng)
    {
        return InitCommand::UseDevCard(usage);
    }

    InitCommand::RollDice
}

pub fn rand_after_dice_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> PostDiceCommand {
    if rng.random_bool(0.8)
        && let Some(usage) = rand_dev_card_usage(&context, rng)
    {
        return PostDiceCommand::UseDevCard(usage);
    }

    PostDiceCommand::RegularCommand(rand_regular_action(context, rng))
}

pub fn rand_regular_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> RegularCommand {
    let categories = [
        RandomRegularCommandCategory::EndMove,
        RandomRegularCommandCategory::BuyDevCard,
        RandomRegularCommandCategory::BuildRoad,
        RandomRegularCommandCategory::BuildSettlement,
        RandomRegularCommandCategory::BuildCity,
        RandomRegularCommandCategory::TradeWithBank,
    ];
    let start = rng.random_range(0..categories.len());

    for offset in 0..categories.len() {
        let category = categories[(start + offset) % categories.len()];
        if let Some(action) = rand_regular_action_in_category(&context, category, rng) {
            return action;
        }
    }

    RegularCommand::EndMove
}

pub fn rand_move_robber(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> MoveRobberCommand {
    let n = context.public.board.arrangement.len();
    let tile_index = match rng.random_range(0..n - 1) {
        index if index == context.public.board_state.robber_pos.index().to_spiral() => n - 1,
        index => index,
    };

    MoveRobberCommand(HexIndex::spiral_to_hex(tile_index))
}

pub fn rand_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
    rng: &mut impl Rng,
) -> ChooseRobbedPlayerCommand {
    let id = context
        .public
        .players_on_hex(robber_pos)
        .into_iter()
        .filter(|id| *id != context.actor)
        .choose(rng)
        .expect("controller must forbid this case");

    ChooseRobbedPlayerCommand(id)
}

#[derive(Debug, Clone, Copy)]
enum RandomRegularCommandCategory {
    EndMove,
    BuyDevCard,
    BuildRoad,
    BuildSettlement,
    BuildCity,
    TradeWithBank,
}

fn rand_regular_action_in_category(
    context: &PlayerDecisionContext<'_>,
    category: RandomRegularCommandCategory,
    rng: &mut impl Rng,
) -> Option<RegularCommand> {
    match category {
        RandomRegularCommandCategory::EndMove => Some(RegularCommand::EndMove),
        RandomRegularCommandCategory::BuyDevCard => {
            legal::can_buy_dev_card(context).then_some(RegularCommand::BuyDevCard)
        }
        RandomRegularCommandCategory::BuildRoad => {
            legal::legal_road_spots_iter(context, context.actor)
                .choose(rng)
                .map(RegularCommand::Build)
        }
        RandomRegularCommandCategory::BuildSettlement => {
            legal::legal_settlement_spots_iter(context, context.actor)
                .choose(rng)
                .map(RegularCommand::Build)
        }
        RandomRegularCommandCategory::BuildCity => {
            legal::legal_city_spots_iter(context, context.actor)
                .choose(rng)
                .map(RegularCommand::Build)
        }
        RandomRegularCommandCategory::TradeWithBank => {
            rand_bank_trade(context, rng).map(RegularCommand::TradeWithBank)
        }
    }
}

fn rand_bank_trade(context: &PlayerDecisionContext<'_>, rng: &mut impl Rng) -> Option<BankTrade> {
    let mut selected = None;
    let mut seen = 0usize;

    sample_resource_trades_at_rate(
        context,
        BankTradeKind::BankGeneric,
        Resource::iter(),
        4,
        &mut selected,
        &mut seen,
        rng,
    );

    for port in context.public.ports_acquired_for(context.actor) {
        match port {
            catan_core::gameplay::primitives::PortKind::Special(resource) => {
                sample_resource_trades_at_rate(
                    context,
                    BankTradeKind::PortSpecific,
                    std::iter::once(*resource),
                    2,
                    &mut selected,
                    &mut seen,
                    rng,
                );
            }
            catan_core::gameplay::primitives::PortKind::Universal => {
                sample_resource_trades_at_rate(
                    context,
                    BankTradeKind::PortGeneric,
                    Resource::iter(),
                    3,
                    &mut selected,
                    &mut seen,
                    rng,
                );
            }
        }
    }

    selected
}

fn sample_resource_trades_at_rate(
    context: &PlayerDecisionContext<'_>,
    kind: BankTradeKind,
    give_candidates: impl IntoIterator<Item = Resource>,
    rate: u16,
    selected: &mut Option<BankTrade>,
    seen: &mut usize,
    rng: &mut impl Rng,
) {
    for give in give_candidates
        .into_iter()
        .filter(|give| context.private.resources.has_enough(&(*give, rate).into()))
    {
        for take in Resource::iter().filter(|take| *take != give) {
            *seen += 1;
            if rng.random_range(0..*seen) == 0 {
                *selected = Some(BankTrade { give, take, kind });
            }
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
        .choose(rng)
}

pub fn rand_drop_half(context: PlayerDecisionContext<'_>, rng: &mut impl Rng) -> DropHalfCommand {
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
                    .pop_random(rng)
                    .expect(&format!("must contain {} cards", number_to_drop));
                to_drop[card] += 1;
            }

            DropHalfCommand(to_drop)
        }
        None => {
            log::error!("couldn't find search context for random agent");
            lazy::lazy_drop_half(context)
        }
    }
}
