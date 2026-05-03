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
        primitives::{player::PlayerId, resource::ResourceCollection},
    },
};

use crate::legal;

#[derive(Debug, Default)]
pub struct LazyAgent {
    id: PlayerId,
}

impl LazyAgent {
    pub fn new(id: PlayerId) -> Self {
        Self { id }
    }
}

impl PlayerNotification for LazyAgent {}

impl PlayerRuntime for LazyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        lazy_init_stage_action(context, self.id)
    }

    fn init_action(&mut self, _context: PlayerDecisionContext<'_>) -> InitAction {
        InitAction::RollDice
    }

    fn after_dice_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDiceAction {
        PostDiceAction::RegularAction(RegularAction::EndMove)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, _context: PlayerDecisionContext<'_>) -> RegularAction {
        RegularAction::EndMove
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        lazy_move_robbers(context)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
    ) -> ChoosePlayerToRobAction {
        lazy_choose_player_to_rob(context, self.id)
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Decline
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        lazy_drop_half(context)
    }
}

pub fn lazy_drop_half(context: PlayerDecisionContext<'_>) -> DropHalfAction {
    let number_to_drop = context.private.resources.total() / 2;
    let mut to_drop = ResourceCollection::default();
    for (resource, number) in context.private.resources.unroll() {
        let remaining = number_to_drop - to_drop.total();

        if remaining == 0 {
            break;
        }

        to_drop[resource] = remaining.min(number);
    }

    DropHalfAction(to_drop)
}

pub fn lazy_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> ChoosePlayerToRobAction {
    let id = legal::legal_rob_targets(&context, player_id)
        .next()
        .expect("GameController must forbid this case");
    ChoosePlayerToRobAction(id)
}

pub fn lazy_move_robbers(context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
    for hex in context.public.board.arrangement.hex_iter() {
        if hex != context.public.board_state.robber_pos {
            return MoveRobbersAction(hex);
        }
    }

    unreachable!("there must be a hex without the robber on it")
}

pub fn lazy_init_stage_action(
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
