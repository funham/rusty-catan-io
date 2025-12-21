use answer::{
    AfterDiceThrowAnswer, AfterKnightAnswer, FinalStateAnswer, InitialAnswer, RobberyAnswer,
    TradeAnswer,
};

use crate::gameplay::field::state::FieldState;
use crate::gameplay::primitives::build::{Road, Settlement};
use crate::gameplay::primitives::trade::PlayerTrade;
use crate::gameplay::{game::state::Perspective, primitives::resource::ResourceCollection};

pub mod answer;
pub mod cli_strategy;
pub mod lazy_ass_strategy;

pub trait Strategy: std::fmt::Debug {
    /* methods used during your turn */
    #[must_use]
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAnswer;
    #[must_use]
    fn move_request_after_knight(&mut self, _: &Perspective) -> AfterKnightAnswer {
        AfterKnightAnswer::ThrowDice
    }
    #[must_use]
    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> AfterDiceThrowAnswer;
    #[must_use]
    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer;
    #[must_use]
    fn move_request_rob(&mut self, perspective: &Perspective) -> RobberyAnswer;
    #[must_use]
    fn initialization(&mut self, field: &FieldState, round: u8) -> (Settlement, Road);

    /* methods not directly related with turn state */

    #[must_use]
    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAnswer;
    #[must_use]
    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection;
}
