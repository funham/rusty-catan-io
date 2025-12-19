use crate::gameplay::{
    game_state::{Perspective, PlayerTrade},
    move_request::{
        MoveRequestAfterDevCard, MoveRequestAfterDiceThrow, MoveRequestAfterDiceThrowAndDevCard,
        MoveRequestInit, RobRequest, TradeAnswer,
    },
    resource::{ResourceCollection},
};

pub mod lazy_ass_strategy;
pub mod cli_strategy;

pub trait Strategy: std::fmt::Debug {
    fn move_request_init(&mut self, perspective: &Perspective) -> MoveRequestInit;
    fn move_request_after_knight(&mut self, perspective: &Perspective) -> MoveRequestAfterDevCard {
        MoveRequestAfterDevCard::ThrowDice
    }
    fn move_request_after_dice_throw(
        &mut self,
        perspective: &Perspective,
    ) -> MoveRequestAfterDiceThrow;
    fn move_request_after_dice_throw_and_dev_card(
        &mut self,
        perspective: &Perspective,
    ) -> MoveRequestAfterDiceThrowAndDevCard;

    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAnswer;
    fn rob(&mut self, perspective: &Perspective) -> RobRequest;
    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection;
}

