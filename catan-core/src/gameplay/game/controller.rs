use super::state::GameState;
use crate::agent::action::{
    self, ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction, MoveRobbersAction,
    PostDevCardAction, PostDiceAction, RegularAction, Request,
};
use crate::gameplay::game::event::{
    self, AuthorizedContext, AuthorizedObserver, GameEvent, PlayerContext, PlayerObserver,
    SpectatorContext, SpectatorObserver,
};
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::primitives::Tile;
use crate::gameplay::primitives::bank::BankResourceExchangeError;
use crate::gameplay::primitives::build::{Build, BuildingError, Establishment, EstablishmentType};
use crate::gameplay::primitives::dev_card::DevCardUsage;
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::BankTrade;
use crate::gameplay::primitives::turn::GameTurn;
use crate::gameplay::{agent::agent::Agent, primitives::resource::HasCost};
use crate::math::dice::DiceRoller;
use crate::{
    gameplay::primitives::resource::ResourceCollection, math::dice::DiceVal, topology::Hex,
};

use std::collections::BTreeSet;

pub enum GameResult {
    Win(PlayerId),
    Interrupted,
}

#[derive(Debug)]
pub enum RobError {
    AutoRob { id: PlayerId },
}

pub struct GameController {
    spectator_observers: Vec<Box<dyn SpectatorObserver>>,
    player_observers: Vec<Box<dyn PlayerObserver>>,
    authorized_observers: Vec<Box<dyn AuthorizedObserver>>,

    game: GameState,
    agents: Vec<Box<dyn Agent>>,
    current_player: PlayerId,
}

impl GameController {
    pub fn new(game: GameState, agents: Vec<Box<dyn Agent>>) -> Self {
        Self {
            spectator_observers: Vec::new(),
            player_observers: Vec::new(),
            authorized_observers: Vec::new(),
            game: game,
            agents,
            current_player: 0,
        }
    }

    // TODO: move into GameInitializer
    pub fn init(
        mut game_init: GameInitializationState,
        agents: &mut Vec<Box<dyn Agent>>,
    ) -> GameState {
        while game_init.turn.get_rounds_played() < 2 {
            let player_id = game_init.turn.get_turn_index();

            loop {
                let action = InitStageAction::request(
                    agents[player_id].as_mut(),
                    &PlayerContext {
                        view: &game_init.clone().finish().view(),
                        player_data: &game_init.clone().finish().private_view(player_id),
                    },
                );

                match game_init.builds.try_init_place(
                    player_id,
                    action.road,
                    Establishment {
                        pos: action.establishment_position,
                        stage: EstablishmentType::Settlement,
                    },
                ) {
                    Err(err) => {
                        match err {
                            BuildingError::InitRoad(_) => {
                                log::error!("invalid initial road placement {:?}", err)
                            }
                            BuildingError::InitSettlement(_) => {
                                log::error!("invalid initial settlement placement {:?}", err)
                            }
                            _ => unreachable!(),
                        };

                        continue;
                    }
                    Ok(()) => {
                        // TODO: notify_observers
                        break;
                    }
                }
            }

            game_init.turn.next();
        }

        GameState {
            turn: GameTurn::new(game_init.field.n_players as u8),
            field: game_init.field,
            bank: game_init.bank,
            players: game_init.players,
            builds: game_init.builds,
        }
    }

    fn curr_agent(&mut self) -> &mut dyn Agent {
        self.agents[self.current_player].as_mut()
    }

    fn notify_observers(&mut self, event: &GameEvent) {
        log::info!("Event: {:?}", event);

        let view = self.game.view();

        for obs in &mut self.spectator_observers {
            obs.on_event(event, &SpectatorContext { view: &view });
        }

        for obs in self.agents.iter_mut() {
            let player = obs.player_id();
            let private = self.game.private_view(player);

            obs.on_event(
                event,
                &PlayerContext {
                    view: &view,
                    player_data: &private,
                },
            );
        }

        for obs in self.player_observers.iter_mut() {
            let player = obs.player_id();
            let private = self.game.private_view(player);

            obs.on_event(
                event,
                &PlayerContext {
                    view: &view,
                    player_data: &private,
                },
            );
        }

        let snapshot = self.game.snapshot();

        for obs in &mut self.authorized_observers {
            obs.on_event(
                event,
                &AuthorizedContext {
                    view: &view,
                    snapshot: &snapshot,
                },
            );
        }
    }

    fn request_dispatch<R: action::Request>(&mut self) -> R {
        let view = self.game.view();
        let private = self.game.private_view(self.current_player);

        R::request(
            self.curr_agent(),
            &PlayerContext {
                view: &view,
                player_data: &private,
            },
        )
    }

    pub fn run(&mut self, dice: &mut dyn DiceRoller) -> GameResult {
        loop {
            if let Some(winner) = self.game.check_win_condition() {
                return GameResult::Win(winner);
            }

            self.current_player = self.game.turn.get_turn_index();

            match self.handle_turn(dice) {
                Ok(_) => self.game.turn.next(),
                Err(_) => break,
            }
        }

        GameResult::Interrupted
    }

    fn handle_turn(&mut self, dice: &mut dyn DiceRoller) -> Result<(), ()> {
        self.game
            .players
            .get_mut(self.current_player)
            .dev_cards_reset_queue();

        self.handle_move_init(dice)
    }

    fn handle_move_init(&mut self, dice: &mut dyn DiceRoller) -> Result<(), ()> {
        let answer = self.request_dispatch::<InitAction>();

        match answer {
            InitAction::RollDice => {
                self.execute_dice_roll(dice);
                self.handle_dice_rolled()
            }
            InitAction::UseDevCard(usage) => self.handle_dev_card_used(usage, dice),
        }
    }

    fn handle_dice_rolled(&mut self) -> Result<(), ()> {
        let answer = self.request_dispatch::<PostDiceAction>();

        match answer {
            PostDiceAction::UseDevCard(usage) => {
                if let Err(e) = self.game.use_dev_card(usage, self.current_player) {
                    log::error!("{:?}", e);
                }
                self.handle_rest()?;
            }

            PostDiceAction::RegularAction(action) => match action {
                RegularAction::Build(build) => {
                    if let Ok(()) = Self::execute_build(&mut self.game, self.current_player, build)
                    {
                        self.notify_observers(&GameEvent::Built(build));
                    }
                }
                RegularAction::TradeWithBank(trade) => {
                    if let Ok(()) =
                        Self::execute_bank_trade(&mut self.game, self.current_player, trade)
                    {
                        self.notify_observers(&GameEvent::Traded);
                    }
                }
                RegularAction::BuyDevCard => {
                    if let Ok(()) = Self::execute_buy_dev_card(&mut self.game, self.current_player)
                    {
                        self.notify_observers(&GameEvent::DevCardBought);
                    }
                }
                RegularAction::OfferPublicTrade(_) => {
                    log::error!("P2P trades not implemented")
                }
                RegularAction::OfferPersonalTrade(_) => {
                    log::error!("P2P trades not implemented")
                }
                RegularAction::EndMove => return Err(()),
            },
        }

        self.handle_dice_rolled()
    }

    fn handle_dev_card_used(
        &mut self,
        usage: DevCardUsage,
        dice: &mut dyn DiceRoller,
    ) -> Result<(), ()> {
        if let Err(e) = self.game.use_dev_card(usage, self.current_player) {
            log::error!("{:?}", e);
        }

        if let Some(winner_id) = self.game.check_win_condition() {
            self.notify_observers(&GameEvent::GameEnded { winner_id });
            return Err(());
        }

        let _ = self.request_dispatch::<PostDevCardAction>();

        self.execute_dice_roll(dice);
        self.handle_rest()
    }

    fn handle_rest(&mut self) -> Result<(), ()> {
        let answer = self.request_dispatch::<RegularAction>();

        match answer {
            RegularAction::Build(build) => {
                if let Ok(()) = Self::execute_build(&mut self.game, self.current_player, build) {
                    self.notify_observers(&GameEvent::Built(build));
                }
            }
            RegularAction::TradeWithBank(trade) => {
                if let Ok(()) = Self::execute_bank_trade(&mut self.game, self.current_player, trade)
                {
                    self.notify_observers(&GameEvent::Traded);
                }
            }
            RegularAction::BuyDevCard => {
                if let Ok(()) = Self::execute_buy_dev_card(&mut self.game, self.current_player) {
                    self.notify_observers(&GameEvent::DevCardBought);
                }
            }
            RegularAction::OfferPublicTrade(_) => log::error!("P2P trades not implemented"),
            RegularAction::OfferPersonalTrade(_) => log::error!("P2P trades not implemented"),
            RegularAction::EndMove => return Err(()),
        }

        if self.game.check_win_condition().is_some() {
            return Err(());
        }

        self.handle_rest()
    }

    fn execute_bank_trade(
        game: &mut GameState,
        player: PlayerId,
        bank_trade: BankTrade,
    ) -> Result<(), BankResourceExchangeError> {
        if let Err(err) =
            game.bank_resource_exchange(player, bank_trade.to_bank(), bank_trade.from_bank())
        {
            log::error!("Invalid bank trade {:?}", err);
            return Err(err);
        }

        Ok(())
    }

    fn execute_build(game: &mut GameState, player: PlayerId, build: Build) -> Result<(), ()> {
        if let Err(err) = game.transfer_to_bank(build.cost(), player) {
            match err {
                BankResourceExchangeError::BankIsShort => unreachable!(),
                BankResourceExchangeError::AccountIsShort { id } => {
                    log::warn!(
                        "Can't build {}: Player#{} has {}",
                        Into::<&str>::into(build),
                        id,
                        game.players.get(id).resources(),
                    );
                    return Err(());
                }
            }
        }

        match game.builds.try_build(player, build.clone()) {
            Ok(()) => Ok(()),
            Err(err) => {
                log::warn!("Couldn't build {}: {:?}", Into::<&str>::into(build), err);
                Err(())
            }
        }
    }

    fn execute_buy_dev_card(game: &mut GameState, player: PlayerId) -> Result<(), ()> {
        const COST: ResourceCollection = ResourceCollection::new(0, 0, 1, 1, 1);

        if game.bank.dev_cards.is_empty() {
            log::warn!("Bank out of dev cards");
            return Err(());
        }

        if let Err(err) = game.transfer_to_bank(COST, player) {
            match err {
                BankResourceExchangeError::BankIsShort => unreachable!(),
                BankResourceExchangeError::AccountIsShort { id } => {
                    log::warn!("Player#{} can't afford dev card", id);
                    return Err(());
                }
            }
        }

        Ok(())
    }

    fn execute_dice_roll(&mut self, dice: &mut dyn DiceRoller) {
        let roll = dice.roll();

        log::info!(
            "Player#[{}] rolled {}",
            self.current_player,
            Into::<u8>::into(roll)
        );

        self.notify_observers(&GameEvent::DiceRolled(roll));

        match roll {
            seven if seven == DiceVal::seven() => {
                Self::execute_seven(&mut self.game, self.current_player)
            }
            other => Self::execute_harvesting(&mut self.game, self.current_player, other),
        }
    }

    fn execute_harvesting(game: &mut GameState, player: PlayerId, num: DiceVal) {
        let hexes = game.field.hexes_by_num(num).clone();

        for pid in game.player_ids_starting_from(player) {
            for est in game.builds[pid].establishments.clone() {
                let coinc = est.pos.as_set();

                for hex in coinc.intersection(&hexes) {
                    match game.field.arrangement[*hex] {
                        Tile::Resource { resource, .. } => {
                            let _ = game.transfer_from_bank(
                                (resource, est.stage.harvest_amount() as u16).into(),
                                pid,
                            );
                        }
                        Tile::Desert => {}
                        Tile::River { .. } => todo!(),
                    }
                }
            }
        }
    }

    fn execute_seven(_game: &mut GameState, _player: PlayerId) {
        log::trace!("seven rolled — robber logic here");
    }
}
