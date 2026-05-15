use std::collections::BTreeSet;

use catan_core::{
    gameplay::{
        constants,
        game::{
            action::{
                ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
                MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction,
            },
            decision::{DecisionKind, OpenDecision},
            input::PlayerCommand,
            view::{CountingMode, PlayerDecisionContext},
        },
        primitives::{
            Tile,
            build::Build,
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::BankTrade,
        },
    },
    topology::{Hex, Intersection},
};

use crate::{
    bot::{BotPolicy, unsupported_decision_command},
    lazy, legal,
};

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

impl GreedyAgent {
    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        let action =
            greedy_init_stage_action(&context, self.id, self.first_initial_resources.as_ref());
        if self.first_initial_resources.is_none() {
            self.first_initial_resources = Some(intersection_resources(
                action.settlement_pos(),
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

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        greedy_drop_half(context)
    }
}

impl BotPolicy for GreedyAgent {
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
            DecisionKind::InitAction => Some(PlayerCommand::InitAction(self.init_action(context))),
            DecisionKind::PostDiceAction => {
                Some(PlayerCommand::PostDice(self.after_dice_action(context)))
            }
            DecisionKind::PostDevCardAction => Some(PlayerCommand::PostDevCard(
                self.after_dev_card_action(context),
            )),
            DecisionKind::RegularAction => {
                Some(PlayerCommand::Regular(self.regular_action(context)))
            }
            DecisionKind::MoveRobber => {
                Some(PlayerCommand::MoveRobbers(self.move_robbers(context)))
            }
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
        .fold(None, |best, hex| {
            let score = (
                context
                    .public
                    .players_on_hex(hex)
                    .filter(|&id| id != context.actor)
                    .count(),
                match context.public.board.arrangement[hex] {
                    Tile::Resource { number, .. } => number.prob_pts(),
                    Tile::River { number } => number.prob_pts() + 3, /* some random *magic* */
                    Tile::Desert => 0,
                },
            );

            match best {
                Some((best_hex, best_score)) if best_score >= score => Some((best_hex, best_score)),
                _ => Some((hex, score)),
            }
        })
        .map(|(hex, _)| hex)
        .expect("some hex must be occupied by at least one other player")
}

pub fn greedy_after_dice_action(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> PostDiceAction {
    if let Some(usage) = legal::first_legal_dev_card_usage(&context) {
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
    if let Some(usage) = legal::first_legal_dev_card_usage(&context) {
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
    context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, player_id)
        .into_iter()
        .max_by_key(|action| {
            initial_settlement_score(
                context.public.board,
                action.settlement_pos(),
                already_acquired,
            )
        })
        .expect("there must be an initial placement")
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
                settlement_production_score(context.public.board, establishment.vtx)
            }
            Build::Road(_) => 0,
        })
}

fn best_road_build(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<Build> {
    let roads = legal::legal_road_spots(context, player_id);
    let Some(resources_after_road) = context
        .private
        .resources
        .checked_sub(&constants::costs::ROAD)
    else {
        return None;
    };

    roads.into_iter().max_by_key(|build| {
        let Build::Road(road) = build else {
            return 0;
        };
        legal::legal_settlement_spots_count_with_extra_road(
            context,
            player_id,
            road.path,
            &resources_after_road,
        )
    })
}

fn best_bank_trade(context: &PlayerDecisionContext<'_>, player_id: PlayerId) -> Option<BankTrade> {
    let trades = legal::legal_bank_trades(context);
    let Some(search) = &context.search else {
        return trades.into_iter().next();
    };
    let state = search.state();

    trades
        .into_iter()
        .max_by_key(|trade| bank_trade_objective_score(context, player_id, *trade, state))
}

fn bank_trade_objective_score(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    trade: BankTrade,
    state: &catan_core::gameplay::game::state::GameState,
) -> (u8, usize) {
    if !state.bank.can_pay(&trade.from_bank()) {
        return (0, 0);
    }

    let Some(resources_after_trade) = resources_after_bank_trade(context.private.resources, trade)
    else {
        return (0, 0);
    };

    next_objective_score_for_resources(context, player_id, &resources_after_trade)
}

fn resources_after_bank_trade(
    resources: &ResourceCollection,
    trade: BankTrade,
) -> Option<ResourceCollection> {
    let mut resources = *resources;
    resources.subtract_in_place(&trade.to_bank()).ok()?;
    resources += &trade.from_bank();
    Some(resources)
}

fn next_objective_score_for_resources(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
    resources: &ResourceCollection,
) -> (u8, usize) {
    let city_count = legal::legal_city_spots_count_with_resources(context, player_id, resources);
    if city_count > 0 {
        return (4, city_count);
    }

    let settlement_count =
        legal::legal_settlement_spots_count_with_resources(context, player_id, resources);
    if settlement_count > 0 {
        return (3, settlement_count);
    }

    let road_count = legal::legal_road_spots_count_with_resources(context, player_id, resources);
    if road_count > 0 {
        return (2, road_count);
    }
    if resources.has_enough(&constants::costs::DEV_CARD) {
        return (1, 1);
    }
    (0, 0)
}

fn initial_settlement_score(
    board: &catan_core::gameplay::field::state::BoardLayout,
    pos: Intersection,
    already_acquired: Option<&BTreeSet<Resource>>,
) -> (usize, u16, usize, u16) {
    let resources = intersection_resource_scores(board, pos);
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

fn settlement_production_score(
    board: &catan_core::gameplay::field::state::BoardLayout,
    pos: Intersection,
) -> u16 {
    intersection_resource_scores(board, pos)
        .into_iter()
        .map(|(_, pts)| pts)
        .sum()
}

fn intersection_resources(
    intersection: catan_core::topology::Intersection,
    board: &catan_core::gameplay::field::state::BoardLayout,
) -> BTreeSet<Resource> {
    intersection_resource_scores(board, intersection)
        .into_iter()
        .map(|(resource, _)| resource)
        .collect()
}

fn intersection_resource_scores(
    board: &catan_core::gameplay::field::state::BoardLayout,
    intersection: Intersection,
) -> Vec<(Resource, u16)> {
    intersection
        .as_arr()
        .into_iter()
        .filter(|hex| hex.norm() <= board.arrangement.radius() as usize)
        .filter_map(|hex| match board.arrangement[hex] {
            Tile::Resource { resource, number } => Some((resource, number.prob_pts() as u16)),
            Tile::River { .. } | Tile::Desert => None,
        })
        .collect()
}
