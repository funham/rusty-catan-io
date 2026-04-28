use crate::{
    agent::action::{ChoosePlayerToRobAction, DropHalfAction, InitStageAction, MoveRobbersAction},
    gameplay::{
        game::{
            event::PlayerNotification,
            view::PlayerDecisionContext,
        },
        primitives::player::PlayerId,
    },
};

use super::action::{InitAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer};

pub trait PlayerRuntime: PlayerNotification {
    fn player_id(&self) -> PlayerId;

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction;
    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction;
    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction;
    fn after_dev_card_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDevCardAction;
    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction;
    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction;
    fn choose_player_to_rob(&mut self, context: PlayerDecisionContext<'_>) -> ChoosePlayerToRobAction;
    fn answer_trade(&mut self, context: PlayerDecisionContext<'_>) -> TradeAnswer;
    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction;
}

pub trait Agent: PlayerRuntime {}

impl<T: PlayerRuntime + ?Sized> Agent for T {}
