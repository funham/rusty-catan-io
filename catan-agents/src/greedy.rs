use catan_core::{
    agent::{
        action::{
            ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
        agent::PlayerRuntime,
    },
    gameplay::{
        game::{
            event::PlayerNotification,
            view::{CountingMode, PlayerDecisionContext},
        },
        primitives::player::PlayerId,
    },
    topology::Hex,
};
use itertools::Itertools;

use crate::{lazy, legal};

#[derive(Debug, Default)]
pub struct GreedyAgent {
    id: PlayerId,
}

impl GreedyAgent {
    pub fn new(id: PlayerId) -> Self {
        Self { id }
    }
}

impl PlayerNotification for GreedyAgent {}

impl PlayerRuntime for GreedyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        greedy_init_stage_action(context, self.id)
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
    if let Some(usage) = legal::legal_dev_card_usages(&context, player_id)
        .into_iter()
        .next()
    {
        PostDiceAction::UseDevCard(usage)
    } else {
        PostDiceAction::RegularAction(greedy_regular_action(&context, player_id))
    }
}

pub fn greedy_regular_action(
    context: &PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> RegularAction {
    if let Some(build) = legal::legal_city_spots(context, player_id)
        .into_iter()
        .next()
    {
        return RegularAction::Build(build);
    }
    if let Some(build) = legal::legal_settlement_spots(context, player_id)
        .into_iter()
        .next()
    {
        return RegularAction::Build(build);
    }
    if legal::can_buy_dev_card(context) {
        return RegularAction::BuyDevCard;
    }
    if let Some(build) = legal::legal_road_spots(context, player_id)
        .into_iter()
        .next()
    {
        return RegularAction::Build(build);
    }
    RegularAction::EndMove
}

pub fn greedy_init_action(context: PlayerDecisionContext<'_>, player_id: PlayerId) -> InitAction {
    if let Some(usage) = legal::legal_dev_card_usages(&context, player_id)
        .into_iter()
        .next()
    {
        InitAction::UseDevCard(usage)
    } else {
        InitAction::RollDice
    }
}

pub fn greedy_init_stage_action(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> InitStageAction {
    let (establishment, road) = context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, player_id)
        .into_iter()
        .next()
        .expect("there must be an initial placement");

    InitStageAction {
        establishment_position: establishment.pos,
        road,
    }
}
