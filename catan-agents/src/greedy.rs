use std::collections::BTreeSet;

use catan_core::{
    agent::{
        action::{
            ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
        agent::PlayerRuntime,
    },
    gameplay::{
        constants,
        game::{
            event::PlayerNotification,
            index::GameIndex,
            view::{
                ContextFactory, CountingMode, PlayerDecisionContext, SearchFactory,
                VisibilityConfig,
            },
        },
        primitives::{
            Tile,
            build::{Build, Establishment},
            player::PlayerId,
            resource::Resource,
            trade::BankTrade,
        },
    },
    topology::Hex,
};
use itertools::Itertools;

use crate::{lazy, legal};

#[derive(Debug, Default)]
pub struct GreedyAgent {
    id: PlayerId,
    first_initial_resources: Option<BTreeSet<Resource>>,
}

impl GreedyAgent {
    pub fn new(id: PlayerId) -> Self {
        Self {
            id,
            first_initial_resources: None,
        }
    }
}

impl PlayerNotification for GreedyAgent {}

impl PlayerRuntime for GreedyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        let action =
            greedy_init_stage_action(&context, self.id, self.first_initial_resources.as_ref());
        if self.first_initial_resources.is_none() {
            self.first_initial_resources = Some(initial_settlement_resources(
                action.establishment_position,
                context.public.board,
            ));
        }
        action
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        greedy_init_action(context, self.id)
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        greedy_after_dice_action(context, self.id)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        greedy_regular_action(&context, self.id)
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        greedy_move_robbers(context)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        greedy_choose_player_to_rob(context, robber_pos)
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Decline
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        greedy_drop_half(context)
    }
}

pub fn greedy_drop_half(context: PlayerDecisionContext<'_>) -> DropHalfAction {
    lazy::lazy_drop_half(context) // TODO: rank cards
}

pub fn greedy_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
) -> ChoosePlayerToRobAction {
    lazy::lazy_choose_player_to_rob(context, robber_pos) // TODO: try to peek the most wanted card
}

pub fn greedy_move_robbers(context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
    let hex = match context.counting() {
        // blocking max amount of players with the most producing hex
        CountingMode::Human => most_occupied_producing_tile(context),
        CountingMode::Counting => most_occupied_producing_tile(context), // TODO: try to peek the most wanted card
    };

    MoveRobbersAction(hex)
}

pub fn most_occupied_producing_tile(context: PlayerDecisionContext<'_>) -> Hex {
    use catan_core::gameplay::primitives::Tile;

    context
        .public
        .board
        .arrangement
        .hex_iter()
        .filter(|&h| h != context.public.board_state.robber_pos)
        .sorted_by_key(|&hex| {
            std::cmp::Reverse((
                context
                    .public
                    .players_on_hex(hex)
                    .iter()
                    .filter(|&&id| id != context.actor)
                    .count(),
                match context.public.board.arrangement[hex] {
                    Tile::Resource { number, .. } => number.prob_pts(),
                    Tile::River { number } => number.prob_pts() + 3, /* some random *magic* */
                    Tile::Desert => 0,
                },
            ))
        })
        .next()
        .expect("some hex must be occupied by at least one other player")
}

pub fn greedy_after_dice_action(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> PostDiceAction {
    if let Some(usage) = legal::legal_dev_card_usages(&context).into_iter().next() {
        PostDiceAction::UseDevCard(usage)
    } else {
        PostDiceAction::RegularAction(greedy_regular_action(&context, player_id))
    }
}

pub fn greedy_regular_action(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> RegularAction {
    if let Some(build) = best_city_build(context, player_id) {
        return RegularAction::Build(build);
    }
    if let Some(build) = best_settlement_build(context, player_id) {
        return RegularAction::Build(build);
    }
    if let Some(build) = best_road_build(context, player_id) {
        return RegularAction::Build(build);
    }
    if legal::can_buy_dev_card(context) {
        return RegularAction::BuyDevCard;
    }
    if let Some(trade) = best_bank_trade(context, player_id) {
        return RegularAction::TradeWithBank(trade);
    }
    RegularAction::EndMove
}

pub fn greedy_init_action(context: PlayerDecisionContext<'_>, _player_id: PlayerId) -> InitAction {
    if let Some(usage) = legal::legal_dev_card_usages(&context).into_iter().next() {
        InitAction::UseDevCard(usage)
    } else {
        InitAction::RollDice
    }
}

pub fn greedy_init_stage_action(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    already_acquired: Option<&BTreeSet<Resource>>,
) -> InitStageAction {
    let (establishment, road) = context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, player_id)
        .into_iter()
        .max_by_key(|(establishment, _)| {
            initial_settlement_score(context.public.board, *establishment, already_acquired)
        })
        .expect("there must be an initial placement");

    InitStageAction {
        establishment_position: establishment.pos,
        road,
    }
}

fn best_city_build(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<Build> {
    legal::legal_city_spots(context, player_id)
        .into_iter()
        .next()
}

fn best_settlement_build(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> Option<Build> {
    legal::legal_settlement_spots(context, player_id)
        .into_iter()
        .max_by_key(|build| match build {
            Build::Establishment(establishment) => {
                production_score_for_settlement(context.public.board, *establishment)
            }
            Build::Road(_) => 0,
        })
}

fn best_road_build(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<Build> {
    let roads = legal::legal_road_spots(context, player_id);
    let Some(search) = &context.search else {
        return roads.into_iter().next();
    };
    let seed = search.make_owned();

    roads.into_iter().max_by_key(|build| {
        let mut state = seed.state.clone();
        if state.build(player_id, *build).is_err() {
            return 0;
        }
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(
            &state,
            visibility.player_policy(player_id),
            player_id,
        ));
        let context = factory.player_decision_context(player_id, search);
        legal::legal_settlement_spots(&context, player_id).len()
    })
}

fn best_bank_trade(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<BankTrade> {
    let trades = legal::legal_bank_trades(context);
    let Some(search) = &context.search else {
        return trades.into_iter().next();
    };
    let seed = search.make_owned();

    trades.into_iter().max_by_key(|trade| {
        let mut state = seed.state.clone();
        if state.trade_with_bank(player_id, *trade).is_err() {
            return (0, 0);
        }
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(
            &state,
            visibility.player_policy(player_id),
            player_id,
        ));
        let context = factory.player_decision_context(player_id, search);
        next_objective_score(&context, player_id)
    })
}

fn next_objective_score(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> (u8, usize) {
    if !legal::legal_city_spots(context, player_id).is_empty() {
        return (4, legal::legal_city_spots(context, player_id).len());
    }
    if !legal::legal_settlement_spots(context, player_id).is_empty() {
        return (3, legal::legal_settlement_spots(context, player_id).len());
    }
    if !legal::legal_road_spots(context, player_id).is_empty() {
        return (2, legal::legal_road_spots(context, player_id).len());
    }
    if context
        .private
        .resources
        .has_enough(&constants::costs::DEV_CARD)
    {
        return (1, 1);
    }
    (0, 0)
}

fn initial_settlement_score(
    board: &catan_core::gameplay::field::state::BoardLayout,
    establishment: Establishment,
    already_acquired: Option<&BTreeSet<Resource>>,
) -> (usize, u16, usize, u16) {
    let resources = settlement_resource_scores(board, establishment);
    let new_resources = resources
        .iter()
        .filter(|(resource, _)| {
            already_acquired
                .map(|acquired| !acquired.contains(resource))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();

    (
        new_resources.len(),
        new_resources.iter().map(|(_, pts)| *pts).sum::<u16>(),
        resources.len(),
        resources.iter().map(|(_, pts)| *pts).sum::<u16>(),
    )
}

fn production_score_for_settlement(
    board: &catan_core::gameplay::field::state::BoardLayout,
    establishment: Establishment,
) -> u16 {
    settlement_resource_scores(board, establishment)
        .into_iter()
        .map(|(_, pts)| pts)
        .sum()
}

fn initial_settlement_resources(
    pos: catan_core::topology::Intersection,
    board: &catan_core::gameplay::field::state::BoardLayout,
) -> BTreeSet<Resource> {
    settlement_resource_scores(
        board,
        Establishment {
            pos,
            stage: catan_core::gameplay::primitives::build::EstablishmentType::Settlement,
        },
    )
    .into_iter()
    .map(|(resource, _)| resource)
    .collect()
}

fn settlement_resource_scores(
    board: &catan_core::gameplay::field::state::BoardLayout,
    establishment: Establishment,
) -> Vec<(Resource, u16)> {
    establishment
        .pos
        .as_set()
        .into_iter()
        .filter(|hex| hex.norm() <= board.arrangement.radius() as usize)
        .filter_map(|hex| match board.arrangement[hex] {
            Tile::Resource { resource, number } => Some((resource, number.prob_pts() as u16)),
            Tile::River { .. } | Tile::Desert => None,
        })
        .collect()
}
