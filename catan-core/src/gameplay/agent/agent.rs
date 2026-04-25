use crate::{
    agent::action::{ChoosePlayerToRobAction, DropHalfAction, InitStageAction, MoveRobbersAction},
    gameplay::game::event::{PlayerContext, PlayerObserver},
};

use super::action::{InitAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer};

pub trait Agent: PlayerObserver {
    fn init_stage_action(&mut self, context: &PlayerContext) -> InitStageAction;
    fn init_action(&mut self, context: &PlayerContext) -> InitAction;
    fn after_dice_action(&mut self, context: &PlayerContext) -> PostDiceAction;
    fn after_dev_card_action(&mut self, context: &PlayerContext) -> PostDevCardAction;
    fn regular_action(&mut self, context: &PlayerContext) -> RegularAction;
    fn move_robbers(&mut self, context: &PlayerContext) -> MoveRobbersAction;
    fn choose_player_to_rob(&mut self, context: &PlayerContext) -> ChoosePlayerToRobAction;
    fn answer_trade(&mut self, context: &PlayerContext) -> TradeAnswer;
    fn drop_half(&mut self, context: &PlayerContext) -> DropHalfAction;
}
