use crate::gameplay::{
    game_state::{Perspective, PlayerTrade, TurnHandlingParams},
    move_request::{RobRequest, TradeAnswer},
    player::Road,
    resource::{Resource, ResourceCollection},
};

pub mod strategy_answers {
    use crate::gameplay::move_request::{
        BankTrade, Buildable, DevCardUsage, PersonalTradeOffer, PublicTradeOffer,
    };

    pub enum MoveRequestInit {
        ThrowDice,
        UseDevCard(DevCardUsage),
    }
    pub enum MoveRequestAfterDevCard {
        ThrowDice,
    }

    pub enum MoveRequestAfterDiceThrow {
        UseDevCard(DevCardUsage),
        OfferPublicTrade(PublicTradeOffer),
        OfferPersonalTrade(PersonalTradeOffer),
        TradeWithBank(BankTrade),
        Build(Buildable),
        EndMove,
    }

    pub enum MoveRequestAfterDiceThrowAndDevCard {
        OfferPublicTrade(PublicTradeOffer),
        OfferPersonalTrade(PersonalTradeOffer),
        TradeWithBank(BankTrade),
        Build(Buildable),
        EndMove,
    }
}

pub trait Strategy: std::fmt::Debug {
    fn move_init(&mut self, perspective: &Perspective) -> strategy_answers::MoveRequestInit;
    fn move_request_after_dev_card(
        &mut self,
        perspective: &Perspective,
    ) -> strategy_answers::MoveRequestAfterDevCard {
        strategy_answers::MoveRequestAfterDevCard::ThrowDice
    }
    fn move_request_after_dice_throw(
        &mut self,
        perspective: &Perspective,
    ) -> strategy_answers::MoveRequestAfterDiceThrow;
    fn move_request_after_dice_throw_and_dev_card(
        &mut self,
        perspective: &Perspective,
    ) -> strategy_answers::MoveRequestAfterDiceThrowAndDevCard;

    fn answer_to_trade(&mut self, trade: &PlayerTrade) -> TradeAnswer;
    fn rob(&mut self, perspective: &Perspective) -> RobRequest;
    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection;
    fn use_roadbuild(&mut self, perspective: &Perspective) -> [Road; 2];
    fn use_year_of_plenty(&mut self, perspective: &Perspective) -> [Resource; 2];
    fn use_monopoly(&mut self, perspective: &Perspective) -> Resource;
}

#[derive(Debug)]
pub struct LazyAssStrategy;

impl Strategy for LazyAssStrategy {
    fn move_init(&mut self, _: &Perspective) -> strategy_answers::MoveRequestInit {
        strategy_answers::MoveRequestInit::ThrowDice
    }

    fn answer_to_trade(&mut self, _: &PlayerTrade) -> TradeAnswer {
        TradeAnswer::Declined
    }

    fn rob(&mut self, perspective: &Perspective) -> RobRequest {
        RobRequest::without_robbing(perspective.field.get_desert_pos())
    }

    fn drop_half(&mut self, perspective: &Perspective) -> ResourceCollection {
        assert!(perspective.player_data.resources.total() > 7);
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
    ) -> strategy_answers::MoveRequestAfterDiceThrow {
        todo!()
    }

    fn move_request_after_dice_throw_and_dev_card(
        &mut self,
        perspective: &Perspective,
    ) -> strategy_answers::MoveRequestAfterDiceThrowAndDevCard {
        todo!()
    }
}

pub struct ConsoleControllerStrategy {}

// impl Strategy for ConsoleControllerStrategy {
// }
