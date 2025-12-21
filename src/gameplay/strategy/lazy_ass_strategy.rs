use super::*;

use crate::gameplay::primitives::Robbery;

#[derive(Debug, Default)]
pub struct LazyAssStrategy;

impl Strategy for LazyAssStrategy {
    fn move_request_init(&mut self, _: &Perspective) -> InitialAnswer {
        InitialAnswer::ThrowDice
    }

    fn answer_to_trade(&mut self, _: &Perspective, _: &PlayerTrade) -> TradeAnswer {
        TradeAnswer::Declined
    }

    fn move_request_rob(&mut self, perspective: &Perspective) -> RobberyAnswer {
        RobberyAnswer {
            robbery: Robbery::just_move(perspective.field.get_desert_pos()),
        }
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        if perspective.player_view.resources.total() <= 7 {
            log::error!("Should not drop cards");
        }

        let number_to_drop = perspective.player_view.resources.total() / 2;
        let mut to_drop = ResourceCollection::default();
        for (resource, number) in perspective.player_view.resources.unroll() {
            let remaining = number_to_drop - to_drop.total();

            if remaining == 0 {
                break;
            }

            to_drop[resource] = remaining.min(number);
        }

        to_drop
    }

    fn move_request_after_dice_throw(&mut self, perspective: &Perspective) -> AfterDiceThrowAnswer {
        AfterDiceThrowAnswer::EndMove
    }

    fn move_request_rest(&mut self, perspective: &Perspective) -> FinalStateAnswer {
        FinalStateAnswer::EndMove
    }

    fn initialization(&mut self, field: &FieldState, round: u8) -> (Settlement, Road) {
        todo!()
    }
}
