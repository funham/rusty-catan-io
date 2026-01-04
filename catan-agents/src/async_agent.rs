use catan_core::{
    agent::{
        action::{FinalStateAnswer, InitialAction, PostDiceThrowAnswer, TradeAction},
        agent::Agent,
    },
    gameplay::{
        field::state::FieldState,
        game::state::Perspective,
        primitives::{
            build::{Road, Settlement},
            player::PlayerId,
            resource::ResourceCollection,
            trade::PlayerTrade,
        },
        strategy::async_strategy::AsyncStrategy,
    },
    topology::Hex,
};

pub struct AsyncAgent {
    strategy: Box<dyn AsyncStrategy>,
}

impl Agent for AsyncAgent {
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAction {
        todo!()
    }

    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> PostDiceThrowAnswer {
        todo!()
    }

    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer {
        todo!()
    }

    fn move_request_rob_hex(&mut self, perspective: &Perspective) -> Hex {
        todo!()
    }

    fn move_request_rob_id(&mut self, perspective: &Perspective) -> PlayerId {
        todo!()
    }

    fn initialization(&mut self, field: &FieldState, round: u8) -> (Settlement, Road) {
        todo!()
    }

    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAction {
        todo!()
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        todo!()
    }
}
