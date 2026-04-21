use super::state::GameState;
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::primitives::Tile;
use crate::gameplay::primitives::bank::BankResourceExchangeError;
use crate::gameplay::primitives::build::{Build, BuildingError, Establishment};
use crate::gameplay::primitives::dev_card::DevCardUsage;
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::BankTrade;
use crate::gameplay::primitives::turn::GameTurn;
use crate::gameplay::{
    agent::{
        action,
        agent::{Agent, AgentRequest, AgentResponse},
    },
    primitives::resource::HasCost,
};
use crate::math::dice::DiceRoller;
use crate::{
    GameEvent, GameObserver, NoopObserver, gameplay::primitives::resource::ResourceCollection,
    math::dice::DiceVal, topology::Hex,
};
use std::collections::BTreeSet;

pub enum GameResult {
    Win(PlayerId),
    Interrupted,
}

pub struct TurnHandlingParams<'a, 'b, 'c> {
    pub(super) player_id: PlayerId,
    pub(super) game: &'a mut GameState,
    pub(super) agents: &'b mut Vec<Box<dyn Agent>>,
    pub(super) observer: &'c mut dyn GameObserver,
}
#[derive(Debug)]
pub enum RobError {
    AutoRob { id: PlayerId },
    WrongAgentResponseType(AgentResponse),
}

#[derive(Debug, Default)]
pub struct GameController {}

impl GameController {
    pub fn init(
        game_init: GameInitializationState,
        strategies: &mut Vec<Box<dyn Agent>>,
    ) -> GameState {
        let mut observer = NoopObserver;
        Self::init_with_observer(game_init, strategies, &mut observer)
    }

    pub fn init_with_observer(
        mut game_init: GameInitializationState,
        strategies: &mut Vec<Box<dyn Agent>>,
        observer: &mut dyn GameObserver,
    ) -> GameState {
        while game_init.turn.get_rounds_played() < 2 {
            let player_id = game_init.turn.get_turn_index();

            let (settlement, road) = match strategies[player_id].respond(
                AgentRequest::Initialization(game_init.perspective(player_id)),
            ) {
                AgentResponse::Initialization {
                    establishment,
                    road,
                } => (establishment, road),
                other => panic!("expected initialization response, got {other:?}"),
            };

            match game_init.builds.try_init_place(player_id, road, settlement) {
                Err(err) => match err {
                    BuildingError::InitRoad(_) => {
                        log::error!("invalid initial road placement {:?}", err)
                    }
                    BuildingError::InitSettlement(_) => {
                        log::error!("invalid initial settlement placement {:?}", err)
                    }
                    _ => unreachable!(),
                },
                _ => (),
            }

            game_init.turn.next();
        }

        let state = GameState {
            turn: GameTurn::new(game_init.field.n_players as u8),
            field: game_init.field,
            bank: game_init.bank,
            players: game_init.players,
            builds: game_init.builds,
        };

        observer.on_event(&GameEvent::GameStarted {
            snapshot: state.snapshot(),
        });

        state
    }

    pub fn run(
        game: &mut GameState,
        agents: &mut Vec<Box<dyn Agent>>,
        dice: &mut dyn DiceRoller,
    ) -> GameResult {
        let mut observer = NoopObserver;
        Self::run_with_observer(game, agents, dice, &mut observer)
    }

    pub fn run_with_observer(
        game: &mut GameState,
        agents: &mut Vec<Box<dyn Agent>>,
        dice: &mut dyn DiceRoller,
        observer: &mut dyn GameObserver,
    ) -> GameResult {
        let mut params = TurnHandlingParams {
            player_id: 0,
            game,
            agents,
            observer,
        };

        loop {
            if let Some(winner_id) = params.game.check_win_condition() {
                params.observer.on_event(&GameEvent::GameFinished {
                    winner_id,
                    snapshot: params.game.snapshot(),
                });
                return GameResult::Win(winner_id);
            };

            let player_id = params.game.turn.get_turn_index();
            params.player_id = player_id;

            match Self::handle_turn(&mut params, dice) {
                Ok(_) => params.game.turn.next(),
                Err(_) => break,
            }
        }

        GameResult::Interrupted
    }

    fn handle_turn(params: &mut TurnHandlingParams, dice: &mut dyn DiceRoller) -> Result<(), ()> {
        params
            .game
            .players
            .get_mut(params.player_id)
            .dev_cards_reset_queue();

        params.observer.on_event(&GameEvent::TurnStarted {
            snapshot: params.game.snapshot(),
        });

        let _ = GameController::handle_move_init(params, dice);
        Ok(())
    }

    fn handle_move_init(
        params: &mut TurnHandlingParams,
        dice: &mut dyn DiceRoller,
    ) -> Result<(), ()> {
        log::trace!("handle_move_init");

        let request = Self::request(
            params,
            AgentRequest::Init(params.game.perspective(params.player_id)),
        );
        let AgentResponse::Init(answer) = request else {
            panic!("wrong agent response type");
        };

        match answer {
            action::InitialAction::RollDice => {
                Self::execute_dice_roll(params, dice);
                Self::handle_dice_rolled(params)
            }
            action::InitialAction::UseDevCard(usage) => {
                Self::handle_dev_card_used(params, usage, dice)
            }
        }
    }

    fn handle_dice_rolled(params: &mut TurnHandlingParams) -> Result<(), ()> {
        let response = Self::request(
            params,
            AgentRequest::AfterDiceThrow(params.game.perspective(params.player_id)),
        );

        let AgentResponse::AfterDice(answer) = response else {
            panic!("wrong agent response type");
        };

        match answer {
            action::PostDiceAnswer::UseDevCard(dev_card_usage)
                if let DevCardUsage::Knight(rob_hex) = dev_card_usage =>
            {
                let robbed_id = Self::get_robbed_id(params, rob_hex);
                let _ = params
                    .game
                    .use_robbers(rob_hex, params.player_id, robbed_id);
                Self::handle_rest(params)?;
            }
            action::PostDiceAnswer::UseDevCard(dev_card_usage) => {
                if let Err(e) = params.game.use_dev_card(dev_card_usage, params.player_id) {
                    log::error!("{:?}", e);
                }
                Self::handle_rest(params)?;
            }
            action::PostDiceAnswer::BuyDevCard => {
                Self::execute_buy_dev_card(params);
            }
            action::PostDiceAnswer::OfferPublicTrade(_) => {
                log::error!("P2P trades are not implemented yet; ignore")
            }
            action::PostDiceAnswer::OfferPersonalTrade(_) => {
                log::error!("P2P trades are not implemented yet; ignore")
            }
            action::PostDiceAnswer::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            action::PostDiceAnswer::Build(build) => {
                Self::execute_build(params, build);
            }
            action::PostDiceAnswer::EndMove => return Err(()),
        }

        Self::handle_dice_rolled(params)
    }

    fn handle_dev_card_used(
        params: &mut TurnHandlingParams,
        usage: DevCardUsage,
        dice: &mut dyn DiceRoller,
    ) -> Result<(), ()> {
        if let Err(e) = params.game.use_dev_card(usage, params.player_id) {
            log::error!("{:?}", e);
        }

        if params.game.check_win_condition().is_some() {
            return Err(());
        }

        let _ = Self::request(
            params,
            AgentRequest::AfterDevCard(params.game.perspective(params.player_id)),
        );

        Self::execute_dice_roll(params, dice);
        Self::handle_rest(params)
    }

    fn handle_rest(params: &mut TurnHandlingParams) -> Result<(), ()> {
        let response = Self::request(
            params,
            AgentRequest::Rest(params.game.perspective(params.player_id)),
        );

        let AgentResponse::Rest(answer) = response else {
            panic!("wrong agent response type");
        };

        match answer {
            action::FinalStateAnswer::OfferPublicTrade(_) => {
                log::error!("P2P trades are not implemented yet; ignore")
            }
            action::FinalStateAnswer::OfferPersonalTrade(_) => {
                log::error!("P2P trades are not implemented yet; ignore")
            }
            action::FinalStateAnswer::TradeWithBank(bank_trade) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            action::FinalStateAnswer::Build(buildable) => Self::execute_build(params, buildable),
            action::FinalStateAnswer::BuyDevCard => Self::execute_buy_dev_card(params),
            action::FinalStateAnswer::EndMove => {
                return Err(());
            }
        }

        if params.game.check_win_condition().is_some() {
            return Err(());
        }

        Self::handle_rest(params)
    }

    fn request(params: &mut TurnHandlingParams, request: AgentRequest) -> AgentResponse {
        params.agents[params.player_id].respond(request)
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

    fn execute_build(params: &mut TurnHandlingParams, build: Build) {
        if let Err(err) = params.game.transfer_to_bank(build.cost(), params.player_id) {
            match err {
                BankResourceExchangeError::BankIsShort => unreachable!(),
                BankResourceExchangeError::AccountIsShort { id } => {
                    log::warn!(
                        "Can't build {}: Player#{} has {}, but {} costs {}",
                        Into::<&str>::into(build),
                        id,
                        params.game.players.get(id).resources(),
                        Into::<&str>::into(build),
                        build.cost()
                    );

                    return;
                }
            }
        }

        if let Err(err) = params
            .game
            .builds
            .try_build(params.player_id, build.clone())
        {
            log::warn!("Couldn't build {}: {:?}", Into::<&str>::into(build), err);
        } else {
            params.observer.on_event(&GameEvent::BuildPlaced {
                player_id: params.player_id,
                build,
                snapshot: params.game.snapshot(),
            });
        }
    }

    fn execute_buy_dev_card(params: &mut TurnHandlingParams) {
        const DEV_CARD_COST: ResourceCollection = ResourceCollection::new(0, 0, 1, 1, 1);

        if params.game.bank.dev_cards.is_empty() {
            log::warn!("Can't buy a dev card: Bank's out of dev cards");
            return;
        }

        if let Err(err) = params
            .game
            .transfer_to_bank(DEV_CARD_COST, params.player_id)
        {
            match err {
                BankResourceExchangeError::BankIsShort => unreachable!(),
                BankResourceExchangeError::AccountIsShort { id } => {
                    log::warn!(
                        "Can't buy a dev card: Player#{} has {}, but a dev card costs {}",
                        params.player_id,
                        params.game.players.get(params.player_id).resources(),
                        DEV_CARD_COST
                    );

                    return;
                }
            }
        }
    }

    fn execute_harvesting_for_one_player(
        params: &mut TurnHandlingParams,
        player_id: PlayerId,
        bounding_set: &BTreeSet<Hex>,
        establishments: impl IntoIterator<Item = Establishment>,
    ) {
        for est in establishments {
            let coincidential_hexes = est.pos.as_set();
            let hexes_to_harvest = coincidential_hexes.intersection(bounding_set);

            for hex in hexes_to_harvest {
                match params.game.field.arrangement[*hex] {
                    Tile::Resource {
                        resource,
                        number: _,
                    } => {
                        if let Err(e) = params.game.transfer_from_bank(
                            (resource, est.stage.harvest_amount() as u16).into(),
                            player_id,
                        ) {
                            log::error!("{:?}", e);
                        }
                    }
                    Tile::Desert => (),
                    Tile::River { number: _ } => todo!("River support is not implemented yet"),
                }
            }
        }
    }

    fn execute_harvesting(params: &mut TurnHandlingParams, num: DiceVal) {
        if num == DiceVal::seven() {
            log::error!("harvesting shouldn't be called if 7 is rolled");
            return;
        }

        let hexes_with_num = params.game.field.hexes_by_num(num).clone();

        for player_id in params.game.player_ids_starting_from(params.player_id) {
            Self::execute_harvesting_for_one_player(
                params,
                player_id,
                &hexes_with_num,
                params.game.builds[player_id].establishments.clone(),
            );
        }
    }

    fn execute_seven(params: &mut TurnHandlingParams) {
        log::trace!("execute_seven");

        for id in 0..params.agents.len() {
            if params.game.players.get(id).resources().total() <= 7 {
                continue;
            }

            let to_drop = match params.agents[id]
                .respond(AgentRequest::DropHalf(params.game.perspective(id)))
            {
                AgentResponse::DropHalf(to_drop) => to_drop,
                other => panic!("expected drop-half response, got {other:?}"),
            };

            if to_drop.total() != params.game.players.get(id).resources().total() / 2 {
                log::error!(
                    "wrong number of cards dropped; {} instead of {}",
                    to_drop.total(),
                    params.game.players.get(id).resources().total() / 2
                );
                return;
            }

            if let Err(e) =
                params
                    .game
                    .bank_resource_exchange(id, to_drop, ResourceCollection::default())
            {
                log::error!("{:?}", e);
            } else {
                params.observer.on_event(&GameEvent::PlayerDiscarded {
                    player_id: id,
                    discarded: to_drop,
                    snapshot: params.game.snapshot(),
                });
            }
        }

        let robbery_hex = loop {
            // validate response type
            let robbery_hex = loop {
                match Self::request(
                    params,
                    AgentRequest::RobHex(params.game.perspective(params.player_id)),
                ) {
                    AgentResponse::RobHex(robbery_hex) => break robbery_hex,
                    other => log::error!("Expected rob-hex response, got {other:?}"),
                };
            };

            // validate response data
            if robbery_hex == params.game.field.robber_pos {
                log::error!("Robbers are already there. Repeat request");
                continue;
            }

            break robbery_hex;
        };

        let robbed_id = Self::get_robbed_id(params, robbery_hex);

        match params
            .game
            .use_robbers(robbery_hex, params.player_id, robbed_id)
        {
            Ok(_) => params.observer.on_event(&GameEvent::RobberMoved {
                player_id: params.player_id,
                hex: robbery_hex,
                robbed_id,
                snapshot: params.game.snapshot(),
            }),
            Err(e) => log::error!("strategy sent invalid rob request: {:?}", e),
        }
    }

    fn get_robbed_id(params: &mut TurnHandlingParams, rob_hex: Hex) -> Option<PlayerId> {
        loop {
            let robbed_id = Self::get_robbed_id_helper(params, rob_hex);

            match robbed_id {
                Ok(robbed_id) => break robbed_id,
                Err(err) => log::error!("Robbing error, trying again. Error: {:?}", err),
            }
        }
    }

    fn get_robbed_id_helper(
        params: &mut TurnHandlingParams,
        rob_hex: Hex,
    ) -> Result<Option<PlayerId>, RobError> {
        match params.game.players_on_hex(rob_hex).as_slice() {
            [] => Ok(None),
            [robbed_id] if robbed_id == &params.player_id => Ok(None),
            [robbed_id] => Ok(Some(*robbed_id)),
            _ => match Self::request(
                params,
                AgentRequest::RobPlayer(params.game.perspective(params.player_id)),
            ) {
                AgentResponse::RobPlayer(id) if id != params.player_id => Ok(Some(id)),
                AgentResponse::RobPlayer(id) => Err(RobError::AutoRob { id }),
                other => Err(RobError::WrongAgentResponseType(other)),
            },
        }
    }

    fn execute_dice_roll(params: &mut TurnHandlingParams, dice: &mut dyn DiceRoller) {
        let roll = dice.roll();
        params.observer.on_event(&GameEvent::DiceRolled {
            player_id: params.player_id,
            value: roll,
            snapshot: params.game.snapshot(),
        });

        match roll {
            seven if seven == DiceVal::seven() => Self::execute_seven(params),
            other => Self::execute_harvesting(params, other),
        }
    }
}
