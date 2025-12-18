use std::collections::BTreeSet;

use crate::{
    gameplay::{
        hex::HexType,
        move_request::{
            BankTrade, Buildable, DevCardUsage, PersonalTradeOffer, PublicTradeOffer, RobRequest,
        },
        player::{HasPos, PlayerId},
        strategy::strategy_answers::{
            self, MoveRequestAfterDiceThrow, MoveRequestAfterDiceThrowAndDevCard,
        },
    },
    math::dice::DiceVal,
    topology::{Hex, Vertex},
};

use crate::gameplay::game_state::{GameState, TurnHandlingParams};

pub enum GameResult {
    Win(PlayerId),
    Interrupted,
}

#[derive(Debug, Default)]
pub struct GameController {}

// TODO: add GUI calls (View as in MVC pattern)
impl GameController {
    // execute game untill it's over
    pub fn run(&self, game: &mut GameState) -> GameResult {
        loop {
            if let Some(winner_id) = game.check_win_condition() {
                return GameResult::Win(winner_id);
            };

            match Self::handle_turn(game) {
                Ok(_) => game.turn.next(),
                Err(_) => break,
            }
        }

        GameResult::Interrupted
    }

    /// Requests current player's strategy, handles it's answers
    /// Leads to recursion in `handle_rest()`
    fn handle_turn(game: &mut GameState) -> Result<(), bool> {
        let mut params = game.get_params();

        game.players[params.player_id].data.dev_cards.reset_queue();

        // functional state machine handling
        GameController::handle_move_init(game, &mut params);
        // end of the move; cant't send request to a strategy
        // ... move ending routines
        todo!()
    }

    /* Turn handling methods; Kind of a procedural state machine */

    fn handle_move_init(game: &mut GameState, params: &mut TurnHandlingParams) {
        match params.strategy.borrow_mut().move_init(todo!()) {
            strategy_answers::MoveRequestInit::ThrowDice => {
                Self::execute_dice_trow(game, &mut params);
                Self::handle_dice_thrown(game, &mut params);
            }
            strategy_answers::MoveRequestInit::UseDevCard(dev_card_usage) => {
                Self::handle_dev_card_used(game, &mut params, dev_card_usage);
            }
        }
    }

    fn handle_dice_thrown(game: &mut GameState, params: &mut TurnHandlingParams) {
        match params
            .strategy
            .borrow_mut()
            .move_request_after_dice_throw(todo!())
        {
            MoveRequestAfterDiceThrow::UseDevCard(dev_card_usage) => {
                game.use_dev_card(dev_card_usage, params.player_id);
                Self::handle_rest(game, params);
                return;
            }
            MoveRequestAfterDiceThrow::OfferPublicTrade(public_trade_offer) => {
                Self::execute_public_trade_offer(game, params, public_trade_offer)
            }
            MoveRequestAfterDiceThrow::OfferPersonalTrade(personal_trade_offer) => {
                Self::execute_personal_trade_offer(game, params, personal_trade_offer)
            }
            MoveRequestAfterDiceThrow::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(game, params, bank_trade);
            }
            MoveRequestAfterDiceThrow::Build(buildable) => {
                Self::execute_build(game, params, buildable);
            }
            MoveRequestAfterDiceThrow::EndMove => {
                return;
            }
        }

        Self::handle_dice_thrown(game, params);
    }

    fn handle_dev_card_used(
        game: &mut GameState,
        params: &mut TurnHandlingParams,
        usage: DevCardUsage,
    ) -> strategy_answers::MoveRequestAfterDevCard {
        game.use_dev_card(usage, params.player_id); // todo: handle errors
        let _ = params
            .strategy
            .borrow_mut()
            .move_request_after_dev_card(todo!()); // dice throw (must be manual for players)
        Self::handle_rest(game, params);
    }

    fn handle_rest(game: &mut GameState, params: &mut TurnHandlingParams) {
        match params
            .strategy
            .borrow_mut()
            .move_request_after_dice_throw_and_dev_card(todo!())
        {
            MoveRequestAfterDiceThrowAndDevCard::OfferPublicTrade(public_trade_offer) => {
                Self::execute_public_trade_offer(game, params, public_trade_offer)
            }
            MoveRequestAfterDiceThrowAndDevCard::OfferPersonalTrade(personal_trade_offer) => {
                Self::execute_personal_trade_offer(game, params, personal_trade_offer);
            }
            MoveRequestAfterDiceThrowAndDevCard::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(game, params, bank_trade)
            }
            MoveRequestAfterDiceThrowAndDevCard::Build(buildable) => {
                Self::execute_build(game, params, buildable)
            }
            MoveRequestAfterDiceThrowAndDevCard::EndMove => {
                return;
            }
        }

        // !warning! recursion
        Self::handle_rest(game, params);
    }

    /* Helper methods, to reduce clutter, no calls to `handle*` methods allowed */

    fn execute_public_trade_offer(
        game: &mut GameState,
        params: &mut TurnHandlingParams,
        public_trade_offer: PublicTradeOffer,
    ) {
        todo!()
    }

    fn execute_personal_trade_offer(
        game: &mut GameState,
        params: &mut TurnHandlingParams,
        personal_trade_offer: PersonalTradeOffer,
    ) {
        todo!()
    }

    fn execute_bank_trade(
        game: &mut GameState,
        params: &mut TurnHandlingParams,
        bank_trade: BankTrade,
    ) {
        todo!()
    }

    fn execute_build(game: &mut GameState, params: &mut TurnHandlingParams, buildable: Buildable) {
        todo!()
    }

    fn execute_harvesting_for_one_player(
        game: &mut GameState,
        player_id: PlayerId,
        bounding_set: &BTreeSet<Hex>,
        buildings: impl IntoIterator<Item = impl HasPos<Pos = Vertex>>,
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
                match game.field.hexes[hex].hex_type {
                    HexType::Some(resource) => {
                        let _ = game.pay_to_player((resource, amount_to_harvest).into(), player_id);
                    }
                    HexType::Desert => todo!(),
                }
            }
        }
    }

    // TODO: add support for golded river
    // (harvest normally for normal hexes + count wildcards + ask strategy for choosing n random cards)
    fn execute_harvesting(game: &mut GameState, params: &mut TurnHandlingParams, num: DiceVal) {
        assert_ne!(num, DiceVal::seven());
        let hexes_with_num = game.field.hexes_by_num(num);

        for player_id in game.player_ids_starting_from(params.player_id) {
            Self::execute_harvesting_for_one_player(
                game,
                player_id,
                &hexes_with_num,
                game.field.builds[player_id].settlements.clone(),
                1,
            );

            Self::execute_harvesting_for_one_player(
                game,
                player_id,
                &hexes_with_num,
                game.field.builds[player_id].cities.clone(),
                2,
            );
        }
    }

    fn execute_seven(game: &mut GameState, params: &mut TurnHandlingParams) {
        for player in &game.players {
            player
                .strategy
                .borrow_mut()
                .drop_half(&game.get_perspective(params.player_id));
        }

        let rob_request: RobRequest = params
            .strategy
            .borrow_mut()
            .rob(&game.get_perspective(params.player_id));

        game.execute_robbers(rob_request, params.player_id)
            .expect("bruuh");
    }

    /// Roll dice, asks strategy if 7, harvest resources for all players otherwise
    fn execute_dice_trow(game: &mut GameState, params: &mut TurnHandlingParams) {
        match game.dice.roll() {
            seven if seven == DiceVal::seven() => Self::execute_seven(game, params),
            other => Self::execute_harvesting(game, params, other),
        }
    }
}
