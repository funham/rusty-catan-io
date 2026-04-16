// use crate::gameplay::agent::{action::*, agent::Agent};

// use crate::gameplay::field::state::FieldState;
// use crate::gameplay::primitives::build::{Road, Settlement};
// use crate::gameplay::primitives::player::PlayerId;
// use crate::gameplay::primitives::trade::PlayerTrade;
// use crate::gameplay::{game::state::RefPerspective, primitives::resource::ResourceCollection};
// use crate::topology::Hex;

// #[derive(Debug)]
// pub struct ConsoleControllerStrategy {}

// impl Agent for ConsoleControllerStrategy {
//     fn move_request_init(&mut self, perspective: &RefPerspective) -> InitialAction {
//         todo!()
//     }

//     fn move_request_after_dice_throw(&mut self, perspective: &RefPerspective) -> PostDiceThrowAnswer {
//         todo!()
//     }

//     fn move_request_rest(&mut self, perspective: &RefPerspective) -> FinalStateAnswer {
//         todo!()
//     }

//     fn answer_to_trade(&mut self, perspective: &RefPerspective, trade: &PlayerTrade) -> TradeAction {
//         todo!()
//     }

//     fn move_request_rob_hex(&mut self, perspective: &RefPerspective) -> Hex {
//         todo!()
//     }

//     fn drop_half(&mut self, perspective: &RefPerspective) -> ResourceCollection {
//         todo!()
//     }

//     fn move_request_rob_id(&mut self, perspective: &RefPerspective) -> PlayerId {
//         todo!()
//     }
    
//     fn initialization(&mut self, perspective: &RefPerspective) -> (Settlement, Road) {
//         todo!()
//     }
// }
