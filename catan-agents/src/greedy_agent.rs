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
        let (establishment, road) = context
            .public
            .builds
            .query()
            .possible_initial_placements(context.public.board, self.id)
            .into_iter()
            .next()
            .expect("there must be an initial placement");

        InitStageAction {
            establishment_position: establishment.pos,
            road,
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        if let Some(usage) = legal::legal_dev_card_usages(&context, self.id)
            .into_iter()
            .next()
        {
            InitAction::UseDevCard(usage)
        } else {
            InitAction::RollDice
        }
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        if let Some(usage) = legal::legal_dev_card_usages(&context, self.id)
            .into_iter()
            .next()
        {
            PostDiceAction::UseDevCard(usage)
        } else {
            PostDiceAction::RegularAction(legal::greedy_regular_action(&context, self.id))
        }
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        legal::greedy_regular_action(&context, self.id)
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        for hex in context.public.board.arrangement.hex_iter() {
            if hex != context.public.board_state.robber_pos {
                return MoveRobbersAction(hex);
            }
        }

        unreachable!("there must be a hex without the robber on it")
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
    ) -> ChoosePlayerToRobAction {
        for id in context
            .public
            .players_on_hex(context.public.board_state.robber_pos)
        {
            if id != self.id {
                return ChoosePlayerToRobAction(id);
            }
        }

        ChoosePlayerToRobAction(self.id)
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Declined
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
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
}
