use super::*;

#[derive(Debug)]
pub struct LazyAssStrategy;

impl Strategy for LazyAssStrategy {
    fn move_request_init(&mut self, _: &Perspective) -> MoveRequestInit {
        MoveRequestInit::ThrowDice
    }

    fn answer_to_trade(&mut self, _: &Perspective, _: &PlayerTrade) -> TradeAnswer {
        TradeAnswer::Declined
    }

    fn rob(&mut self, perspective: &Perspective) -> RobRequest {
        RobRequest::just_move(perspective.field.get_desert_pos())
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        if perspective.player_data.resources.total() <= 7 {
            log::error!("Should not drop cards");
        }

        let number_to_drop = perspective.player_data.resources.total() / 2;
        let mut to_drop = ResourceCollection::default();
        for (resource, number) in perspective.player_data.resources.unroll() {
            let remaining = number_to_drop - to_drop.total();

            if remaining == 0 {
                break;
            }

            to_drop[resource] = remaining.min(number);
        }

        to_drop
    }

    fn move_request_after_dice_throw(
        &mut self,
        perspective: &Perspective,
    ) -> MoveRequestAfterDiceThrow {
        MoveRequestAfterDiceThrow::EndMove
    }

    fn move_request_after_dice_throw_and_dev_card(
        &mut self,
        perspective: &Perspective,
    ) -> MoveRequestAfterDiceThrowAndDevCard {
        MoveRequestAfterDiceThrowAndDevCard::EndMove
    }
}
