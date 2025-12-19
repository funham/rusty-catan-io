use std::collections::BTreeSet;

use crate::{
    gameplay::{
        hex::HexType,
        move_request::{
            BankTrade, Buildable, DevCardUsage, MoveRequestAfterDiceThrow,
            MoveRequestAfterDiceThrowAndDevCard, MoveRequestInit, PersonalTradeOffer,
            PublicTradeOffer, RobRequest,
        },
        player::{City, HasPos, PlayerId, Settlement},
        resource::ResourceCollection,
    },
    math::dice::DiceVal,
    strategy::Strategy,
    topology::{Hex, Intersection},
};

use crate::gameplay::game_state::{GameState, TurnHandlingParams};

pub enum GameResult {
    Win(PlayerId),
    Interrupted,
}

#[derive(Debug, Default)]
pub struct GameController {}

impl GameController {
    // execute game untill it's over
    pub fn run<'a>(
        game: &mut GameState,
        strategies: &'a mut Vec<&'a mut dyn Strategy>,
    ) -> GameResult {
        let mut params = TurnHandlingParams {
            player_id: 0,
            game,
            strategies: strategies,
        };

        loop {
            if let Some(winner_id) = params.game.check_win_condition() {
                return GameResult::Win(winner_id);
            };

            let player_id = params.game.turn.get_turn_index();
            params.player_id = player_id;

            match Self::handle_turn(&mut params) {
                Ok(_) => params.game.turn.next(),
                Err(_) => break,
            }
        }

        GameResult::Interrupted
    }

    /// Requests current player's strategy, handles it's answers
    /// Leads to recursion in `handle_rest()`
    fn handle_turn(params: &mut TurnHandlingParams) -> Result<(), ()> {
        params.game.players[params.player_id]
            .dev_cards
            .reset_queue();

        // functional state machine handling
        GameController::handle_move_init(params);
        // end of the move; cant't send request to a strategy
        // ... move ending routines
        Ok(())
    }

    /* Turn handling methods; Kind of a procedural state machine */

    fn handle_move_init(params: &mut TurnHandlingParams) {
        match params.strategies[params.player_id]
            .move_request_init(&params.game.get_perspective(params.player_id))
        {
            MoveRequestInit::ThrowDice => {
                Self::execute_dice_trow(params);
                Self::handle_dice_thrown(params);
            }
            MoveRequestInit::UseKnight(rob_request) => {
                Self::handle_dev_card_used(params, DevCardUsage::Knight(rob_request));
            }
        }
    }

    fn handle_dice_thrown(params: &mut TurnHandlingParams) {
        match params.strategies[params.player_id]
            .move_request_after_dice_throw(&params.game.get_perspective(params.player_id))
        {
            MoveRequestAfterDiceThrow::UseDevCard(dev_card_usage) => {
                if let Err(e) = params.game.use_dev_card(dev_card_usage, params.player_id) {
                    log::error!("{:?}", e);
                }
                Self::handle_rest(params);
                return;
            }
            MoveRequestAfterDiceThrow::OfferPublicTrade(public_trade_offer) => {
                Self::execute_public_trade_offer(params, public_trade_offer)
            }
            MoveRequestAfterDiceThrow::OfferPersonalTrade(personal_trade_offer) => {
                Self::execute_personal_trade_offer(params, personal_trade_offer)
            }
            MoveRequestAfterDiceThrow::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            MoveRequestAfterDiceThrow::Build(buildable) => {
                Self::execute_build(params, buildable);
            }
            MoveRequestAfterDiceThrow::EndMove => {
                return;
            }
        }

        Self::handle_dice_thrown(params);
    }

    fn handle_dev_card_used(params: &mut TurnHandlingParams, usage: DevCardUsage) {
        if let Err(e) = params.game.use_dev_card(usage, params.player_id) {
            log::error!("{:?}", e);
        }
        let _ = params.strategies[params.player_id]
            .move_request_after_knight(&params.game.get_perspective(params.player_id)); // dice throw (must be manual for players)
        Self::handle_rest(params);
    }

    fn handle_rest(params: &mut TurnHandlingParams) {
        match params.strategies[params.player_id].move_request_after_dice_throw_and_dev_card(
            &params.game.get_perspective(params.player_id),
        ) {
            MoveRequestAfterDiceThrowAndDevCard::OfferPublicTrade(public_trade_offer) => {
                Self::execute_public_trade_offer(params, public_trade_offer)
            }
            MoveRequestAfterDiceThrowAndDevCard::OfferPersonalTrade(personal_trade_offer) => {
                Self::execute_personal_trade_offer(params, personal_trade_offer);
            }
            MoveRequestAfterDiceThrowAndDevCard::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(params, bank_trade)
            }
            MoveRequestAfterDiceThrowAndDevCard::Build(buildable) => {
                Self::execute_build(params, buildable)
            }
            MoveRequestAfterDiceThrowAndDevCard::EndMove => {
                return;
            }
        }

        // !warning! recursion
        Self::handle_rest(params);
    }

    /* Helper methods, to reduce clutter, no calls to `handle*` methods allowed */

    fn execute_public_trade_offer(
        params: &mut TurnHandlingParams,
        public_trade_offer: PublicTradeOffer,
    ) {
        todo!()
    }

    fn execute_personal_trade_offer(
        params: &mut TurnHandlingParams,
        personal_trade_offer: PersonalTradeOffer,
    ) {
        todo!()
    }

    fn execute_bank_trade(params: &mut TurnHandlingParams, bank_trade: BankTrade) {
        todo!()
    }

    fn execute_build(params: &mut TurnHandlingParams, buildable: Buildable) {
        todo!()
    }

    fn execute_harvesting_for_one_player(
        params: &mut TurnHandlingParams,
        bounding_set: &BTreeSet<Hex>,
        buildings: impl IntoIterator<Item = impl HasPos<Pos = Intersection>>,
        amount_to_harvest: u16,
    ) {
        for build_pos in buildings
            .into_iter()
            .map(|b| b.get_pos())
            // well that's annoying
            .collect::<Vec<_>>()
        {
            let coincidential_hexes = build_pos.as_set();
            let hexes_to_harvest = coincidential_hexes.intersection(&bounding_set);

            for hex in hexes_to_harvest {
                match params.game.field.hexes[hex].hex_type {
                    HexType::Some(resource) => {
                        if let Err(e) = params.game.transfer_from_bank(
                            (resource, amount_to_harvest).into(),
                            params.player_id,
                        ) {
                            log::error!("{:?}", e);
                        }
                    }
                    HexType::Desert => (),
                }
            }
        }
    }

    // TODO: add support for golded river
    // (harvest normally for normal hexes + count wildcards + ask strategy for choosing n random cards)
    fn execute_harvesting(params: &mut TurnHandlingParams, num: DiceVal) {
        if num == DiceVal::seven() {
            log::error!("harvesting shouldn't be called if 7 is rolled");
            return;
        }

        let hexes_with_num = params.game.field.hexes_by_num(num);

        for player_id in params.game.player_ids_starting_from(params.player_id) {
            Self::execute_harvesting_for_one_player(
                params,
                &hexes_with_num,
                params.game.field.builds[player_id].settlements.clone(),
                Settlement::harvesting_rate(),
            );

            Self::execute_harvesting_for_one_player(
                params,
                &hexes_with_num,
                params.game.field.builds[player_id].cities.clone(),
                City::harvesting_rate(),
            );
        }
    }

    fn execute_seven(params: &mut TurnHandlingParams) {
        for (id, strategy) in params.strategies.iter_mut().enumerate() {
            if params.game.players[params.player_id].resources.total() <= 7 {
                continue;
            }

            // in more than 7 cards
            let to_drop = strategy.drop_half(&params.game.get_perspective(params.player_id));

            if to_drop.total() != params.game.players[params.player_id].resources.total() / 2 {
                log::error!(
                    "wrong number of cards dropped; {} instead of {}",
                    to_drop.total(),
                    params.game.players[params.player_id].resources.total() / 2
                );

                return;
            }

            if let Err(e) =
                params
                    .game
                    .bank_resource_exchange(id, to_drop, ResourceCollection::default())
            {
                log::error!("{:?}", e);
            }
        }

        let rob_request: RobRequest =
            params.strategies[params.player_id].rob(&params.game.get_perspective(params.player_id));

        match params.game.execute_robbers(rob_request, params.player_id) {
            Ok(_) => (),
            Err(e) => log::error!("strategy sent invalid rob request: {:?}", e),
        }
    }

    /// Roll dice, asks strategy if 7, harvest resources for all players otherwise
    fn execute_dice_trow(params: &mut TurnHandlingParams) {
        match params.game.dice.roll() {
            seven if seven == DiceVal::seven() => Self::execute_seven(params),
            other => Self::execute_harvesting(params, other),
        }
    }
}
