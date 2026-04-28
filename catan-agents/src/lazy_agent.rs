use catan_core::{
    agent::{
        Agent,
        action::{
            self, ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
    },
    gameplay::{
        game::event::{PlayerContext, PlayerObserver},
        primitives::{player::PlayerId, resource::ResourceCollection},
    },
};

#[derive(Debug, Default)]
pub struct LazyAgent {
    id: PlayerId,
}

impl LazyAgent {
    pub fn new(id: PlayerId) -> Self {
        Self { id }
    }

    fn choose_initial(&self, context: &PlayerContext) -> InitStageAction {
        let (establishment, road) = context
            .view
            .builds
            .query()
            .possible_initial_placements(&context.view.field, self.id)
            .first()
            .expect("there must be a place")
            .clone();

        InitStageAction {
            establishment_position: establishment.pos,
            road,
        }
    }
}

impl PlayerObserver for LazyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }
}

impl Agent for LazyAgent {
    fn init_stage_action(&mut self, context: &PlayerContext) -> InitStageAction {
        self.choose_initial(context)
    }

    fn init_action(&mut self, _context: &PlayerContext) -> InitAction {
        InitAction::RollDice
    }

    fn after_dice_action(&mut self, _context: &PlayerContext) -> PostDiceAction {
        PostDiceAction::RegularAction(RegularAction::EndMove)
    }

    fn after_dev_card_action(&mut self, _context: &PlayerContext) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, _context: &PlayerContext) -> RegularAction {
        RegularAction::EndMove
    }

    fn move_robbers(&mut self, context: &PlayerContext) -> action::MoveRobbersAction {
        for hex in context.view.field.arrangement.hex_iter() {
            if hex != context.view.field.robber_pos {
                return MoveRobbersAction(hex);
            }
        }

        unreachable!("there must be a hex without robbers on it")
    }

    fn choose_player_to_rob(&mut self, context: &PlayerContext) -> action::ChoosePlayerToRobAction {
        for id in context.view.players_on_hex(context.view.field.robber_pos) {
            if id != self.id {
                return ChoosePlayerToRobAction(id);
            }
        }

        unreachable!(
            "there must be a player that's on that hex that isn't a player (validation on the controller's site)"
        )
    }

    fn answer_trade(&mut self, _context: &PlayerContext) -> TradeAnswer {
        TradeAnswer::Declined
    }

    fn drop_half(&mut self, context: &PlayerContext) -> action::DropHalfAction {
        let number_to_drop = context.player_data.resources.total() / 2;
        let mut to_drop = ResourceCollection::default();
        for (resource, number) in context.player_data.resources.unroll() {
            let remaining = number_to_drop - to_drop.total();

            if remaining == 0 {
                break;
            }

            to_drop[resource] = remaining.min(number);
        }

        DropHalfAction(to_drop)
    }
}
