use crate::gameplay::agent::{agent::Agent, action::*};

use crate::gameplay::field::state::FieldState;
use crate::gameplay::primitives::build::{Road, Settlement};
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::PlayerTrade;
use crate::gameplay::{game::state::Perspective, primitives::resource::ResourceCollection};
use crate::topology::Hex;

#[derive(Debug)]
pub struct ConsoleControllerStrategy {}

impl Agent for ConsoleControllerStrategy {
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAction {
        todo!()
    }

    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> PostDiceThrowAnswer {
        todo!()
    }

    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer {
        todo!()
    }

    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAction {
        todo!()
    }

    fn move_request_rob_hex(&mut self, perspective: &Perspective) -> Hex {
        todo!()
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        todo!()
    }

    fn initialization(&mut self, field: &FieldState, round: u8) -> (Settlement, Road) {
        todo!()
    }

    fn move_request_rob_id(&mut self, perspective: &Perspective) -> PlayerId {
        todo!()
    }
}
