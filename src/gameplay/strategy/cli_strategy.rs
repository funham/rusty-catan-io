use super::*;

#[derive(Debug)]
pub struct ConsoleControllerStrategy {}

impl Strategy for ConsoleControllerStrategy {
    fn move_request_init(&mut self, perspective: &Perspective) -> InitialAnswer {
        todo!()
    }

    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> AfterDiceThrowAnswer {
        todo!()
    }

    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer {
        todo!()
    }

    fn answer_to_trade(&mut self, perspective: &Perspective, trade: &PlayerTrade) -> TradeAnswer {
        todo!()
    }

    fn move_request_rob(&mut self, perspective: &Perspective) -> RobberyAnswer {
        todo!()
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        todo!()
    }
}
