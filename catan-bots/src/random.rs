use crate::{bot::BotPolicy, lazy, legal, trade};
use catan_core::{
    gameplay::{
        game::{
            command::{
                ChooseRobbedPlayerCommand, DiscardHalfCommand, InitCommand,
                InitialPlacementCommand, MoveRobberCommand, PostDevCardCommand, PostDiceCommand,
                RegularCommand,
            },
            decision::DecisionKind,
            input::{DecisionRequest, PlayerCommand, TradeCommand, TradeResponseCommand},
            trade::TradeSessionId,
            view::PlayerDecisionContext,
        },
        primitives::{
            dev_card::{DevCardUsage, UsableDevCard},
            player::{PlayerId, player_ids},
            resource::{Resource, ResourceSet},
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
use smallvec::SmallVec;

#[derive(Debug)]
pub struct RandomAgent<R = SmallRng> {
    id: PlayerId,
    rng: R,
    attempted_trades: SmallVec<[(ResourceSet, ResourceSet); 16]>,
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
        Self {
            id,
            rng,
            attempted_trades: SmallVec::new(),
        }
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
        rand_regular_action_with_trades(context, &mut self.rng, &mut self.attempted_trades)
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

    fn discard_half(&mut self, context: PlayerDecisionContext<'_>) -> DiscardHalfCommand {
        rand_discard_half(context, &mut self.rng)
    }

    fn trade_response(
        &mut self,
        context: PlayerDecisionContext<'_>,
        session_id: TradeSessionId,
    ) -> TradeResponseCommand {
        rand_trade_response(context, session_id, &mut self.rng)
    }

    fn trade_owner_action(
        &mut self,
        context: PlayerDecisionContext<'_>,
        session_id: TradeSessionId,
    ) -> TradeCommand {
        rand_trade_owner_action(context, session_id, &mut self.rng)
    }
}

impl<R: Rng> BotPolicy for RandomAgent<R> {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn command_for(
        &mut self,
        request: &DecisionRequest,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match request.kind() {
            DecisionKind::InitialPlacement => Some(PlayerCommand::InitialPlacement(
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
            DecisionKind::DiscardHalf { .. } => {
                Some(PlayerCommand::DiscardHalf(self.discard_half(context)))
            }
            DecisionKind::TradeResponse { session } => Some(PlayerCommand::Trade(
                TradeCommand::Respond(self.trade_response(context, session)),
            )),
            DecisionKind::TradeOwnerAction { session } => Some(PlayerCommand::Trade(
                self.trade_owner_action(context, session),
            )),
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
    let mut attempted = SmallVec::new();
    rand_regular_action_with_trades(context, rng, &mut attempted)
}

fn rand_regular_action_with_trades(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
    attempted_trades: &mut SmallVec<[(ResourceSet, ResourceSet); 16]>,
) -> RegularCommand {
    let categories = [
        RandomRegularCommandCategory::EndMove,
        RandomRegularCommandCategory::BuyDevCard,
        RandomRegularCommandCategory::BuildRoad,
        RandomRegularCommandCategory::BuildSettlement,
        RandomRegularCommandCategory::BuildCity,
        RandomRegularCommandCategory::PlayerTrade,
        RandomRegularCommandCategory::TradeWithBank,
    ];
    let start = rng.random_range(0..categories.len());

    for offset in 0..categories.len() {
        let category = categories[(start + offset) % categories.len()];
        if let Some(action) =
            rand_regular_action_in_category(&context, category, rng, attempted_trades)
        {
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
    PlayerTrade,
    TradeWithBank,
}

fn rand_regular_action_in_category(
    context: &PlayerDecisionContext<'_>,
    category: RandomRegularCommandCategory,
    rng: &mut impl Rng,
    attempted_trades: &mut SmallVec<[(ResourceSet, ResourceSet); 16]>,
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
        RandomRegularCommandCategory::PlayerTrade => {
            rand_player_trade(context, rng, attempted_trades)
        }
        RandomRegularCommandCategory::TradeWithBank => {
            rand_bank_trade(context, rng).map(RegularCommand::TradeWithBank)
        }
    }
}

fn rand_player_trade(
    context: &PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
    attempted_trades: &mut SmallVec<[(ResourceSet, ResourceSet); 16]>,
) -> Option<RegularCommand> {
    let candidates = trade::one_card_trade_candidates(context)
        .filter(|offer| {
            if attempted_trades.contains(&(offer.give, offer.take)) {
                return false;
            }
            player_ids(context.public.players.len())
                .filter(|peer| *peer != context.actor)
                .any(|peer| {
                    trade::exact_resources(context, peer)
                        .is_some_and(|resources| resources.has_enough(&offer.take))
                })
        })
        .collect::<Vec<_>>();
    let offer = candidates.choose(rng).copied()?;
    attempted_trades.push((offer.give, offer.take));
    Some(RegularCommand::OfferTrade(offer))
}

pub fn rand_trade_response(
    context: PlayerDecisionContext<'_>,
    session_id: TradeSessionId,
    rng: &mut impl Rng,
) -> TradeResponseCommand {
    let Some(session) = trade::session(&context, session_id) else {
        return TradeResponseCommand::Reject;
    };
    let funded = session
        .offers
        .iter()
        .filter(|offer| offer.proposer == session.proposer)
        .filter(|offer| trade::offer_is_funded(&context, session, offer.id, context.actor))
        .map(|offer| offer.id)
        .collect::<Vec<_>>();
    funded
        .choose(rng)
        .copied()
        .filter(|_| rng.random_bool(0.5))
        .map(|offer_id| TradeResponseCommand::Accept { offer_id })
        .unwrap_or(TradeResponseCommand::Reject)
}

pub fn rand_trade_owner_action(
    context: PlayerDecisionContext<'_>,
    session_id: TradeSessionId,
    rng: &mut impl Rng,
) -> TradeCommand {
    let Some(session) = trade::session(&context, session_id) else {
        return TradeCommand::Cancel;
    };
    trade::committable_offers(&context, session)
        .map(|(offer_id, _)| offer_id)
        .choose(rng)
        .filter(|_| rng.random_bool(0.8))
        .map(|offer_id| TradeCommand::Commit { offer_id })
        .unwrap_or(TradeCommand::Cancel)
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
                            ResourceSet::default(),
                            |mut acc, resource| {
                                acc += resource.into();
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

pub fn rand_discard_half(
    context: PlayerDecisionContext<'_>,
    rng: &mut impl Rng,
) -> DiscardHalfCommand {
    let number_to_discard = context.private.resources.total() / 2;

    match context.search {
        Some(search) => {
            let mut to_discard = ResourceSet::default();
            let search = search.make_owned();
            let mut res = *search.state.players.get(search.root_player).resources();

            for _ in 0..number_to_discard {
                let card = pop_random_resource(&mut res, rng)
                    .unwrap_or_else(|| panic!("must contain {number_to_discard} cards"));
                to_discard[card] += 1;
            }

            DiscardHalfCommand(to_discard)
        }
        None => {
            log::error!("couldn't find search context for random agent");
            lazy::lazy_discard_half(context)
        }
    }
}

fn pop_random_resource(resources: &mut ResourceSet, rng: &mut impl Rng) -> Option<Resource> {
    if resources.is_empty() {
        return None;
    }

    let mut offset = rng.random_range(0..resources.total());
    for resource in Resource::iter() {
        let count = resources[resource];
        if offset < count {
            resources.subtract_in_place(&resource.into()).ok()?;
            return Some(resource);
        }
        offset -= count;
    }

    None
}
