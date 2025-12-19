use super::*;

#[derive(Debug)]
pub struct ConsoleControllerStrategy {}

impl Strategy for ConsoleControllerStrategy {
    fn move_request_init(&mut self, perspective: &Perspective) -> MoveRequestInit {
        todo!()
    }

    fn move_request_after_dice_throw(
        &mut self,
        perspective: &Perspective,
    ) -> MoveRequestAfterDiceThrow {
        todo!()
    }

    fn move_request_after_dice_throw_and_dev_card(
        &mut self,
        perspective: &Perspective,
    ) -> MoveRequestAfterDiceThrowAndDevCard {
        todo!()
    }

    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAnswer {
        todo!()
    }

    fn rob(&mut self, perspective: &Perspective) -> RobRequest {
        todo!()
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        todo!()
    }
}
