use answer::{
    AfterDiceThrowAnswer, AfterKnightAnswer, FinalStateAnswer, InitialAnswer, RobberyAnswer,
    TradeAnswer,
};

use crate::gameplay::primitives::trade::PlayerTrade;
use crate::gameplay::{game::state::Perspective, primitives::resource::ResourceCollection};

pub mod answer;
pub mod cli_strategy;
pub mod lazy_ass_strategy;

pub trait Strategy: std::fmt::Debug {
    /* methods used during your turn */
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAnswer;
    fn move_request_after_knight(&mut self, _: &Perspective) -> AfterKnightAnswer {
        AfterKnightAnswer::ThrowDice
    }
    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> AfterDiceThrowAnswer;
    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer;
    fn move_request_rob(&mut self, perspective: &Perspective) -> RobberyAnswer;

    /* methods not directly related with turn state */
    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAnswer;
    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection;
}
