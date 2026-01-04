use catan_core::{
    agent::action::{
        FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceThrowAnswer, TradeAction,
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
        strategy::lazy_ass_strategy::LazyAssStrategy,
    },
    topology::Hex,
};

pub trait Agent {
    /* methods used during your turn */
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAction;
    fn move_request_after_dev_card(&mut self, _: &Perspective) -> PostDevCardAction {
        PostDevCardAction::ThrowDice
    }
    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> PostDiceThrowAnswer;
    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer;
    fn move_request_rob_hex(&mut self, perspective: &Perspective) -> Hex;
    fn move_request_rob_id(&mut self, perspective: &Perspective) -> PlayerId;
    fn initialization(&mut self, field: &FieldState, round: u8) -> (Settlement, Road);

    /* methods not directly related with turn state */

    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAction;
    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection;
}

// pub struct AgentFactory;

// impl AgentFactory {
//     pub fn fetch(_name: &str) -> Box<dyn Agent> {
//         log::warn!("todo: implement strategy table");
//         Box::new(LazyAssStrategy)
//     }
// }
