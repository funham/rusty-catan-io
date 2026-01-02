use action::{
    FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceThrowAnswer, TradeAction,
};

use crate::gameplay::field::state::FieldState;
use crate::gameplay::primitives::build::{Road, Settlement};
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::PlayerTrade;
use crate::gameplay::{game::state::Perspective, primitives::resource::ResourceCollection};
use crate::topology::Hex;

pub mod action;
pub mod cli_strategy;
pub mod lazy_ass_strategy;

pub trait Strategy: std::fmt::Debug {
    /* methods used during your turn */
    #[must_use]
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAction;
    #[must_use]
    fn move_request_after_knight(&mut self, _: &Perspective) -> PostDevCardAction {
        PostDevCardAction::ThrowDice
    }
    #[must_use]
    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> PostDiceThrowAnswer;
    #[must_use]
    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer;
    #[must_use]
    fn move_request_rob_hex(&mut self, perspective: &Perspective) -> Hex;
    #[must_use]
    fn move_request_rob_id(&mut self, perspective: &Perspective) -> PlayerId;
    #[must_use]
    fn initialization(&mut self, field: &FieldState, round: u8) -> (Settlement, Road);

    /* methods not directly related with turn state */

    #[must_use]
    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAction;
    #[must_use]
    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection;
}
