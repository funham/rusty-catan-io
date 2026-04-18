use super::state::GameState;
use crate::gameplay::agent::{
    action,
    agent::{Agent, AgentRequest, AgentResponse},
};
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::primitives::Tile;
use crate::gameplay::primitives::build::{Build, BuildingError, Establishment};
use crate::gameplay::primitives::dev_card::DevCardUsage;
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::BankTrade;
use crate::gameplay::primitives::turn::GameTurn;
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
        match Self::request(
            params,
            AgentRequest::Init(params.game.perspective(params.player_id)),
        ) {
            AgentResponse::Init(action::InitialAction::ThrowDice) => {
                Self::execute_dice_trow(params, dice);
                Self::handle_dice_thrown(params)
            }
            AgentResponse::Init(action::InitialAction::UseDevCard(usage)) => {
                Self::handle_dev_card_used(params, usage, dice)
            }
            other => panic!("expected init response, got {other:?}"),
        }
    }

    fn handle_dice_thrown(params: &mut TurnHandlingParams) -> Result<(), ()> {
        match Self::request(
            params,
            AgentRequest::AfterDiceThrow(params.game.perspective(params.player_id)),
        ) {
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::UseDevCard(
                dev_card_usage,
            )) if matches!(dev_card_usage, DevCardUsage::Knight(_)) => {
                let DevCardUsage::Knight(rob_hex) = dev_card_usage else {
                    unreachable!()
                };
                let robbed_id = Self::get_robbed_id(params, rob_hex);
                let _ = params
                    .game
                    .use_robbers(rob_hex, params.player_id, robbed_id);
                Self::handle_rest(params)?;
            }
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::UseDevCard(
                dev_card_usage,
            )) => {
                if let Err(e) = params.game.use_dev_card(dev_card_usage, params.player_id) {
                    log::error!("{:?}", e);
                }
                Self::handle_rest(params)?;
            }
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::OfferPublicTrade(_)) => {
                log::warn!("Trades are not implemented yet")
            }
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::OfferPersonalTrade(_)) => {
                log::warn!("Trades are not implemented yet")
            }
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::TradeWithBank(
                bank_trade,
            )) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::Build(buildable)) => {
                Self::execute_build(params, buildable);
            }
            AgentResponse::AfterDiceThrow(action::PostDiceThrowAnswer::EndMove) => {
                return Err(());
            }
            other => panic!("expected after-dice response, got {other:?}"),
        }

        Self::handle_dice_thrown(params)
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

        Self::execute_dice_trow(params, dice);
        Self::handle_rest(params)
    }

    fn handle_rest(params: &mut TurnHandlingParams) -> Result<(), ()> {
        match Self::request(
            params,
            AgentRequest::Rest(params.game.perspective(params.player_id)),
        ) {
            AgentResponse::Rest(action::FinalStateAnswer::OfferPublicTrade(_)) => {
                log::warn!("Trades are not implemented yet")
            }
            AgentResponse::Rest(action::FinalStateAnswer::OfferPersonalTrade(_)) => {
                log::warn!("Trades are not implemented yet")
            }
            AgentResponse::Rest(action::FinalStateAnswer::TradeWithBank(bank_trade)) => {
                Self::execute_bank_trade(params, bank_trade);
            }
            AgentResponse::Rest(action::FinalStateAnswer::Build(buildable)) => {
                Self::execute_build(params, buildable)
            }
            AgentResponse::Rest(action::FinalStateAnswer::EndMove) => {
                return Err(());
            }
            other => panic!("expected rest response, got {other:?}"),
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

    fn execute_build(params: &mut TurnHandlingParams, buildable: Build) {
        if let Err(err) = params
            .game
            .builds
            .try_build(params.player_id, buildable.clone())
        {
            log::error!("Invalid building try: {:?}", err)
        } else {
            params.observer.on_event(&GameEvent::BuildPlaced {
                player_id: params.player_id,
                build: buildable,
                snapshot: params.game.snapshot(),
            });
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

        let robbery_hex = match Self::request(
            params,
            AgentRequest::RobHex(params.game.perspective(params.player_id)),
        ) {
            AgentResponse::RobHex(robbery_hex) => robbery_hex,
            other => panic!("expected rob-hex response, got {other:?}"),
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
        match params.game.players_on_hex(rob_hex).as_slice() {
            [] => None,
            [robbed_id] => Some(*robbed_id),
            _ => match Self::request(
                params,
                AgentRequest::RobPlayer(params.game.perspective(params.player_id)),
            ) {
                AgentResponse::RobPlayer(id) => Some(id),
                other => panic!("expected rob-player response, got {other:?}"),
            },
        }
    }

    fn execute_dice_trow(params: &mut TurnHandlingParams, dice: &mut dyn DiceRoller) {
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
