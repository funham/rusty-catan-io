use super::state::GameState;
use crate::gameplay::agent::action;
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::primitives::HexResource;
use crate::gameplay::primitives::build::{BuildingError, Builds, City, Settlement};
use crate::gameplay::primitives::dev_card::DevCardUsage;
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::{BankTrade, PersonalTradeOffer, PublicTradeOffer};
use crate::gameplay::primitives::turn::GameTurn;
use crate::math::dice::DiceRoller;
use crate::topology::HasPos;
use crate::{
    agent::agent::Agent,
    gameplay::primitives::resource::ResourceCollection,
    math::dice::DiceVal,
    topology::{Hex, Intersection},
};
use std::collections::BTreeSet;

pub enum GameResult {
    Win(PlayerId),
    Interrupted,
}

/// convinient struct with neccessary info about player who's turn it currently is
pub struct TurnHandlingParams<'a, 'b> {
    pub(super) player_id: PlayerId,
    pub(super) game: &'a mut GameState,
    pub(super) strategies: &'b mut Vec<Box<dyn Agent>>,
}

#[derive(Debug, Default)]
pub struct GameController {}

impl GameController {
    pub fn init(
        mut game_init: GameInitializationState,
        strategies: &mut Vec<Box<dyn Agent>>,
        dice: Box<dyn DiceRoller>,
    ) -> GameState {
        while game_init.turn.get_rounds_played() < 2 {
            let player_id = game_init.turn.get_turn_index();

            let (settlement, road) = strategies[player_id]
                .initialization(&game_init.field, game_init.turn.get_rounds_played() as u8);

            match game_init.builds.try_init_place(player_id, road, settlement) {
                Err(err) => match err {
                    BuildingError::InitRoad() => {
                        log::error!("invalid initial road placement {:?}", err)
                    }
                    BuildingError::InitSettlement() => {
                        log::error!("invalid initial settlement placement {:?}", err)
                    }
                    _ => unreachable!(),
                },
                _ => (),
            }

            game_init.turn.next();
        }

        GameState {
            turn: GameTurn::new(game_init.field.n_players as u8),
            field: game_init.field,
            dice,
            bank: game_init.bank,
            players: game_init.players,
            builds: game_init.builds,
        }
    }

    // execute game untill it's over
    pub fn run(game: &mut GameState, strategies: &mut Vec<Box<dyn Agent>>) -> GameResult {
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
        params
            .game
            .players
            .get_mut(params.player_id)
            .dev_cards_reset_queue();

        // functional state machine handling
        let _ = GameController::handle_move_init(params);
        // end of the move; cant't send request to a strategy
        // ... move ending routines
        Ok(())
    }

    /* Turn handling methods; Kind of a procedural state machine */

    fn handle_move_init(params: &mut TurnHandlingParams) -> Result<(), ()> {
        match params.strategies[params.player_id]
            .move_request_init(&params.game.get_perspective(params.player_id))
        {
            action::InitialAction::ThrowDice => {
                Self::execute_dice_trow(params);
                Self::handle_dice_thrown(params)
            }
            action::InitialAction::UseDevCard(usage) => Self::handle_dev_card_used(params, usage),
        }
    }

    fn handle_dice_thrown(params: &mut TurnHandlingParams) -> Result<(), ()> {
        match params.strategies[params.player_id]
            .move_request_after_dice_throw(&params.game.get_perspective(params.player_id))
        {
            action::PostDiceThrowAnswer::UseDevCard(dev_card_usage)
                if matches!(dev_card_usage, DevCardUsage::Knight(_)) =>
            {
                let rob_hex = if let DevCardUsage::Knight(rob_hex) = dev_card_usage {
                    rob_hex
                } else {
                    unreachable!()
                };
                let robbed_id = Self::get_robbed_id(params, rob_hex);
                let _ = params
                    .game
                    .use_robbers(rob_hex, params.player_id, robbed_id);
                Self::handle_rest(params)?;
            }
            action::PostDiceThrowAnswer::UseDevCard(dev_card_usage) => {
                if let Err(e) = params.game.use_dev_card(dev_card_usage, params.player_id) {
                    log::error!("{:?}", e);
                }
                Self::handle_rest(params)?;
            }
            action::PostDiceThrowAnswer::OfferPublicTrade(public_trade_offer) => {
                // Self::execute_public_trade_offer(params, public_trade_offer, acceptor);
                log::warn!("Trades are not implemented yet")
            }
            action::PostDiceThrowAnswer::OfferPersonalTrade(personal_trade_offer) => {
                // Self::execute_personal_trade_offer(params, personal_trade_offer);
                log::warn!("Trades are not implemented yet")
            }
            action::PostDiceThrowAnswer::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            action::PostDiceThrowAnswer::Build(buildable) => {
                Self::execute_build(params, buildable);
            }
            action::PostDiceThrowAnswer::EndMove => {
                return Err(());
            }
        }

        Self::handle_dice_thrown(params)
    }

    fn handle_dev_card_used(
        params: &mut TurnHandlingParams,
        usage: DevCardUsage,
    ) -> Result<(), ()> {
        if let Err(e) = params.game.use_dev_card(usage, params.player_id) {
            log::error!("{:?}", e);
        }

        if let Some(_) = params.game.check_win_condition() {
            return Err(());
        }

        let _ = params.strategies[params.player_id]
            .move_request_after_dev_card(&params.game.get_perspective(params.player_id)); // dice throw (must be manual for players)

        Self::execute_dice_trow(params);
        Self::handle_rest(params)
    }

    fn handle_rest(params: &mut TurnHandlingParams) -> Result<(), ()> {
        match params.strategies[params.player_id]
            .move_request_rest(&params.game.get_perspective(params.player_id))
        {
            action::FinalStateAnswer::OfferPublicTrade(public_trade_offer) => {
                // Self::execute_public_trade_offer(params, public_trade_offer);
                log::warn!("Trades are not implemented yet")
            }
            action::FinalStateAnswer::OfferPersonalTrade(personal_trade_offer) => {
                // Self::execute_personal_trade_offer(params, personal_trade_offer);
                log::warn!("Trades are not implemented yet")
            }
            action::FinalStateAnswer::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            action::FinalStateAnswer::Build(buildable) => Self::execute_build(params, buildable),
            action::FinalStateAnswer::EndMove => {
                return Err(());
            }
        }

        if params.game.check_win_condition().is_some() {
            return Err(());
        }

        // !warning! recursion
        Self::handle_rest(params)
    }

    /* Helper methods, to reduce clutter, no calls to `handle*` methods allowed */

    fn execute_public_trade_offer(
        params: &mut TurnHandlingParams,
        trade: PublicTradeOffer,
        acceptor: PlayerId,
    ) {
        if let Err(err) = params
            .game
            .players_resource_exchange((params.player_id, trade.give), (acceptor, trade.take))
        {
            log::error!("Invalid player resource exchange: {:?}", err)
        }
    }

    fn execute_personal_trade_offer(params: &mut TurnHandlingParams, trade: PersonalTradeOffer) {
        if let Err(err) = params
            .game
            .players_resource_exchange((params.player_id, trade.give), (trade.peer_id, trade.take))
        {
            log::error!("Invalid player resource exchange: {:?}", err)
        }
    }

    fn execute_bank_trade(params: &mut TurnHandlingParams, bank_trade: BankTrade) {
        if let Err(err) = params.game.bank_resource_exchange(
            params.player_id,
            bank_trade.to_bank(),
            bank_trade.from_bank(),
        ) {
            log::error!("Invalid bank resource exchange: {:?}", err)
        }
    }

    fn execute_build(params: &mut TurnHandlingParams, buildable: Builds) {
        if let Err(err) = params.game.builds.try_build(params.player_id, buildable) {
            log::error!("Invalid building try: {:?}", err)
        }
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
                match params.game.field.hexes[hex].hex_resource {
                    HexResource::Some(resource) => {
                        if let Err(e) = params.game.transfer_from_bank(
                            (resource, amount_to_harvest).into(),
                            params.player_id,
                        ) {
                            log::error!("{:?}", e);
                        }
                    }
                    HexResource::Desert => (),
                    HexResource::River => todo!(),
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

        let hexes_with_num = params.game.field.hexes_by_num(num).clone();

        for player_id in params.game.player_ids_starting_from(params.player_id) {
            Self::execute_harvesting_for_one_player(
                params,
                &hexes_with_num,
                params.game.builds[player_id].settlements.clone(),
                Settlement::harvesting_rate(),
            );

            Self::execute_harvesting_for_one_player(
                params,
                &hexes_with_num,
                params.game.builds[player_id].cities.clone(),
                City::harvesting_rate(),
            );
        }
    }

    fn execute_seven(params: &mut TurnHandlingParams) {
        for (id, strategy) in params.strategies.iter_mut().enumerate() {
            if params
                .game
                .players
                .get(params.player_id)
                .resources()
                .total()
                <= 7
            {
                continue;
            }

            // in more than 7 cards
            let to_drop = strategy.drop_half(&params.game.get_perspective(params.player_id));

            if to_drop.total()
                != params
                    .game
                    .players
                    .get(params.player_id)
                    .resources()
                    .total()
                    / 2
            {
                log::error!(
                    "wrong number of cards dropped; {} instead of {}",
                    to_drop.total(),
                    params
                        .game
                        .players
                        .get(params.player_id)
                        .resources()
                        .total()
                        / 2
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

        let robbery_hex = params.strategies[params.player_id]
            .move_request_rob_hex(&params.game.get_perspective(params.player_id));

        let robbed_id = Self::get_robbed_id(params, robbery_hex);

        match params
            .game
            .use_robbers(robbery_hex, params.player_id, robbed_id)
        {
            Ok(_) => (),
            Err(e) => log::error!("strategy sent invalid rob request: {:?}", e),
        }
    }

    fn get_robbed_id(params: &mut TurnHandlingParams, rob_hex: Hex) -> Option<PlayerId> {
        match params.game.players_on_hex(rob_hex).as_slice() {
            [] => None,
            [robbed_id] => Some(*robbed_id),
            _ => Some(
                params.strategies[params.player_id]
                    .move_request_rob_id(&params.game.get_perspective(params.player_id)),
            ),
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
