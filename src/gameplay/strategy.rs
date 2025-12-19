use log::error;

use crate::gameplay::{
    game_state::{Perspective, PlayerTrade},
    move_request::{
        MoveRequestAfterDevCard, MoveRequestAfterDiceThrow, MoveRequestAfterDiceThrowAndDevCard,
        MoveRequestInit, RobRequest, TradeAnswer,
    },
    player::Road,
    resource::{Resource, ResourceCollection},
};

pub trait Strategy: std::fmt::Debug {
    fn move_init(&mut self, perspective: &Perspective) -> MoveRequestInit;
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
    fn use_roadbuild(&mut self, perspective: &Perspective) -> [Road; 2];
    fn use_year_of_plenty(&mut self, perspective: &Perspective) -> [Resource; 2];
    fn use_monopoly(&mut self, perspective: &Perspective) -> Resource;
}

#[derive(Debug)]
pub struct LazyAssStrategy;

impl Strategy for LazyAssStrategy {
    fn move_init(&mut self, _: &Perspective) -> MoveRequestInit {
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
            error!("Should not drop cards");
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

    fn use_roadbuild(&mut self, perspective: &Perspective) -> [Road; 2] {
        todo!()
    }

    fn use_year_of_plenty(&mut self, perspective: &Perspective) -> [Resource; 2] {
        todo!()
    }
    fn use_monopoly(&mut self, _: &Perspective) -> Resource {
        Resource::Ore
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

pub struct ConsoleControllerStrategy {}

// impl Strategy for ConsoleControllerStrategy {
// }
