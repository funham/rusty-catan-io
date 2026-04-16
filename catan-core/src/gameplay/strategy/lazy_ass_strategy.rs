// use crate::gameplay::agent::{action::*, agent::Agent};

// use crate::gameplay::field::state::FieldState;
// use crate::gameplay::primitives::build::{Road, Settlement};
// use crate::gameplay::primitives::player::PlayerId;
// use crate::gameplay::primitives::trade::PlayerTrade;
// use crate::gameplay::{game::state::RefPerspective, primitives::resource::ResourceCollection};
// use crate::topology::Hex;

// #[derive(Debug, Default)]
// pub struct LazyAssStrategy;

// impl Agent for LazyAssStrategy {
//     fn move_request_init(&mut self, _: &RefPerspective) -> InitialAction {
//         InitialAction::ThrowDice
//     }

//     fn answer_to_trade(&mut self, _: &RefPerspective, _: &PlayerTrade) -> TradeAction {
//         TradeAction::Declined
//     }

//     fn drop_half(&mut self, perspective: &RefPerspective) -> ResourceCollection {
//         if perspective.player_view.resources.total() <= 7 {
//             log::error!("Should not drop cards");
//         }

//         let number_to_drop = perspective.player_view.resources.total() / 2;
//         let mut to_drop = ResourceCollection::default();
//         for (resource, number) in perspective.player_view.resources.unroll() {
//             let remaining = number_to_drop - to_drop.total();

//             if remaining == 0 {
//                 break;
//             }

//             to_drop[resource] = remaining.min(number);
//         }

//         to_drop
//     }

//     fn move_request_after_dice_throw(&mut self, perspective: &RefPerspective) -> PostDiceThrowAnswer {
//         PostDiceThrowAnswer::EndMove
//     }

//     fn move_request_rest(&mut self, perspective: &RefPerspective) -> FinalStateAnswer {
//         FinalStateAnswer::EndMove
//     }

//     fn move_request_rob_id(&mut self, perspective: &RefPerspective) -> PlayerId {
//         todo!()
//     }

//     fn move_request_rob_hex(&mut self, perspective: &RefPerspective) -> Hex {
//         todo!()
//     }

//     fn initialization(&mut self, perspective: &RefPerspective) -> (Settlement, Road) {
//         todo!()
//     }
// }
