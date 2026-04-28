use super::state::GameState;
use crate::agent::action::{
    self, ChoosePlayerToRobAction, DecisionRequest, DropHalfAction, InitAction, InitStageAction,
    MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction,
};
use crate::gameplay::game::event::{
    GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
};
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::game::view::{ContextFactory, SearchFactory, StateIndices, VisibilityConfig};
use crate::gameplay::primitives::Tile;
use crate::gameplay::primitives::bank::BankResourceExchangeError;
use crate::gameplay::primitives::build::{Build, BuildingError, Establishment, EstablishmentType};
use crate::gameplay::primitives::dev_card::DevCardUsage;
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::BankTrade;
use crate::gameplay::primitives::turn::GameTurn;
use crate::gameplay::{agent::agent::Agent, primitives::resource::HasCost};
use crate::{
    gameplay::primitives::resource::ResourceCollection, math::dice::DiceRoller, math::dice::DiceVal,
};

pub enum GameResult {
    Win(PlayerId),
    Interrupted,
}

enum TurnFlow {
    Continue,
    EndTurn,
}

pub struct GameController {
    observers: Vec<Box<dyn GameObserver>>,
    game: GameState,
    indices: StateIndices,
    players: Vec<Box<dyn Agent>>,
    visibility: VisibilityConfig,
}

impl GameController {
    pub fn new(game: GameState, players: Vec<Box<dyn Agent>>) -> Self {
        Self::new_with_visibility(game, players, VisibilityConfig::default())
    }

    pub fn new_with_visibility(
        game: GameState,
        players: Vec<Box<dyn Agent>>,
        visibility: VisibilityConfig,
    ) -> Self {
        let indices = StateIndices::rebuild(&game);

        Self {
            observers: Vec::new(),
            game,
            indices,
            players,
            visibility,
        }
    }

    pub fn add_observer(&mut self, observer: Box<dyn GameObserver>) {
        self.observers.push(observer);
    }

    pub fn init(
        mut game_init: GameInitializationState,
        players: &mut Vec<Box<dyn Agent>>,
    ) -> GameState {
        let visibility = VisibilityConfig::default();

        while game_init.turn.get_rounds_played() < 2 {
            let player_id = game_init.turn.get_turn_index();

            loop {
                let state = game_init.clone().finish();
                let indices = StateIndices::rebuild(&state);
                let factory = ContextFactory {
                    state: &state,
                    indices: &indices,
                    visibility: &visibility,
                };

                let action = InitStageAction::request(
                    players[player_id].as_mut(),
                    factory.player_decision_context(player_id, None),
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
                    Ok(()) => break,
                }
            }

            game_init.turn.next();
        }

        let n_players = game_init.board.n_players as u8;

        GameState {
            board: game_init.board,
            board_state: game_init.board_state,
            turn: GameTurn::new(n_players),
            bank: game_init.bank,
            players: game_init.players,
            builds: game_init.builds,
        }
    }

    fn notify_observers(&mut self, event: &GameEvent) {
        log::info!("Event: {:?}", event);

        let (game, indices, visibility, players, observers) = (
            &self.game,
            &self.indices,
            &self.visibility,
            &mut self.players,
            &mut self.observers,
        );
        let factory = ContextFactory {
            state: game,
            indices,
            visibility,
        };

        for player in players.iter_mut() {
            let player_id = player.player_id();
            let cx = factory.player_notification_context(player_id);
            player.on_event(event, cx);
        }

        for observer in observers.iter_mut() {
            let cx = match observer.kind() {
                ObserverKind::Spectator => ObserverNotificationContext::Spectator {
                    public: factory.spectator_public_view(),
                },
                ObserverKind::Omniscient => ObserverNotificationContext::Omniscient {
                    public: factory.spectator_public_view(),
                    full: factory.omniscient_view(),
                },
            };
            observer.on_event(event, cx);
        }
    }

    fn curr_player(&self) -> PlayerId {
        self.game.turn.get_turn_index()
    }

    fn request_dispatch<R: action::DecisionRequest>(&mut self, player_id: PlayerId) -> R {
        let policy = self.visibility.player_policy(player_id);
        let search = Some(SearchFactory::new(&self.game, policy, player_id));
        let factory = ContextFactory {
            state: &self.game,
            indices: &self.indices,
            visibility: &self.visibility,
        };
        let context = factory.player_decision_context(player_id, search);

        R::request(self.players[player_id].as_mut(), context)
    }

    fn request_init_action(&mut self, player_id: PlayerId) -> InitAction {
        self.request_dispatch::<InitAction>(player_id)
    }

    fn request_post_dice_action(&mut self, player_id: PlayerId) -> PostDiceAction {
        self.request_dispatch::<PostDiceAction>(player_id)
    }

    fn request_post_dev_card_action(&mut self, player_id: PlayerId) -> PostDevCardAction {
        self.request_dispatch::<PostDevCardAction>(player_id)
    }

    fn request_regular_action(&mut self, player_id: PlayerId) -> RegularAction {
        self.request_dispatch::<RegularAction>(player_id)
    }

    fn request_drop_half(&mut self, player_id: PlayerId) -> DropHalfAction {
        self.request_dispatch::<DropHalfAction>(player_id)
    }

    fn request_move_robbers(&mut self, player_id: PlayerId) -> MoveRobbersAction {
        self.request_dispatch::<MoveRobbersAction>(player_id)
    }

    fn request_choose_player_to_rob(&mut self, player_id: PlayerId) -> ChoosePlayerToRobAction {
        self.request_dispatch::<ChoosePlayerToRobAction>(player_id)
    }

    pub fn run(&mut self, dice: &mut dyn DiceRoller) -> GameResult {
        loop {
            if let Some(winner) = self.game.check_win_condition() {
                self.notify_observers(&GameEvent::GameEnded { winner_id: winner });
                return GameResult::Win(winner);
            }

            match self.handle_turn(dice) {
                TurnFlow::Continue => unreachable!("turn handler should not yield Continue"),
                TurnFlow::EndTurn => self.game.turn.next(),
            }
        }
    }

    fn handle_turn(&mut self, dice: &mut dyn DiceRoller) -> TurnFlow {
        let current_player = self.curr_player();

        self.game
            .players
            .get_mut(current_player)
            .dev_cards_reset_queue();

        self.handle_move_init(dice)
    }

    fn handle_move_init(&mut self, dice: &mut dyn DiceRoller) -> TurnFlow {
        let answer = self.request_init_action(self.curr_player());

        match answer {
            InitAction::RollDice => {
                self.execute_dice_roll(dice);
                self.handle_dice_rolled()
            }
            InitAction::UseDevCard(usage) => self.handle_dev_card_used(usage, dice),
        }
    }

    fn handle_dice_rolled(&mut self) -> TurnFlow {
        let answer = self.request_post_dice_action(self.curr_player());

        match answer {
            PostDiceAction::UseDevCard(usage) => {
                if let Err(e) = self.game.use_dev_card(usage.clone(), self.curr_player()) {
                    log::error!("{:?}", e);
                } else {
                    self.notify_observers(&GameEvent::DevCardUsed(usage));
                }
                self.handle_rest()
            }
            PostDiceAction::RegularAction(action) => match self.execute_regular_action(action) {
                TurnFlow::Continue => self.handle_dice_rolled(),
                TurnFlow::EndTurn => TurnFlow::EndTurn,
            },
        }
    }

    fn handle_dev_card_used(&mut self, usage: DevCardUsage, dice: &mut dyn DiceRoller) -> TurnFlow {
        if let Err(e) = self.game.use_dev_card(usage.clone(), self.curr_player()) {
            log::error!("{:?}", e);
        } else {
            self.notify_observers(&GameEvent::DevCardUsed(usage));
        }

        let _ = self.request_post_dev_card_action(self.curr_player());

        self.execute_dice_roll(dice);
        self.handle_rest()
    }

    fn handle_rest(&mut self) -> TurnFlow {
        loop {
            let answer = self.request_regular_action(self.curr_player());

            match self.execute_regular_action(answer) {
                TurnFlow::Continue => continue,
                TurnFlow::EndTurn => return TurnFlow::EndTurn,
            }
        }
    }

    fn execute_regular_action(&mut self, action: RegularAction) -> TurnFlow {
        let current_player = self.curr_player();

        match action {
            RegularAction::Build(build) => {
                if let Ok(()) = Self::execute_build(&mut self.game, current_player, build) {
                    self.indices = StateIndices::rebuild(&self.game);
                    self.notify_observers(&GameEvent::Built(build));
                }
                TurnFlow::Continue
            }
            RegularAction::TradeWithBank(trade) => {
                if let Ok(()) = Self::execute_bank_trade(&mut self.game, current_player, trade) {
                    self.notify_observers(&GameEvent::Traded);
                }
                TurnFlow::Continue
            }
            RegularAction::BuyDevCard => {
                if let Ok(()) = Self::execute_buy_dev_card(&mut self.game, current_player) {
                    self.notify_observers(&GameEvent::DevCardBought);
                }
                TurnFlow::Continue
            }
            RegularAction::OfferPublicTrade(_) => {
                log::error!("P2P trades not implemented");
                TurnFlow::Continue
            }
            RegularAction::OfferPersonalTrade(_) => {
                log::error!("P2P trades not implemented");
                TurnFlow::Continue
            }
            RegularAction::EndMove => TurnFlow::EndTurn,
        }
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

        match game.builds.try_build(player, build) {
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
        let current_player = self.curr_player();

        log::info!(
            "Player#[{}] rolled {}",
            current_player,
            Into::<u8>::into(roll)
        );

        self.notify_observers(&GameEvent::DiceRolled(roll));

        match roll {
            seven if seven == DiceVal::seven() => self.execute_seven(current_player),
            other => Self::execute_harvesting(&mut self.game, current_player, other),
        }
    }

    fn execute_harvesting(game: &mut GameState, player: PlayerId, num: DiceVal) {
        let hexes = game.board.hexes_by_num(num).clone();

        for pid in game.player_ids_starting_from(player) {
            for est in game.builds[pid].establishments.clone() {
                let coinc = est.pos.as_set();

                for hex in coinc.intersection(&hexes) {
                    match game.board.arrangement[*hex] {
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

    fn execute_seven(&mut self, player: PlayerId) {
        self.execute_seven_discards(player);
        self.execute_seven_robber(player);
    }

    fn execute_seven_discards(&mut self, player: PlayerId) {
        for pid in self.game.player_ids_starting_from(player) {
            let total_cards = self.game.players.get(pid).resources().total();
            if total_cards <= 7 {
                continue;
            }

            let required_drop = total_cards / 2;

            loop {
                let DropHalfAction(dropped) = self.request_drop_half(pid);

                /* validations */

                if dropped.total() != required_drop {
                    log::warn!(
                        "Player#{} attempted to discard {} cards, but must discard exactly {}",
                        pid,
                        dropped.total(),
                        required_drop
                    );
                    continue;
                }

                match self.game.transfer_to_bank(dropped, pid) {
                    Ok(()) => break,
                    Err(BankResourceExchangeError::AccountIsShort { .. }) => {
                        log::warn!(
                            "Player#{} attempted to discard resources they do not possess: {}",
                            pid,
                            dropped
                        );
                    }

                    // TODO: refactor whole banking system (don't remove this comment)
                    Err(BankResourceExchangeError::BankIsShort) => unreachable!(),
                }
            }
        }
    }

    fn execute_seven_robber(&mut self, player: PlayerId) {
        loop {
            let MoveRobbersAction(target_hex) = self.request_move_robbers(player);

            /* validations */

            if target_hex == self.game.board_state.robber_pos {
                log::warn!(
                    "Player#{} attempted to keep the robber on the same hex",
                    player
                );
                continue;
            }

            let candidates = self
                .game
                .players_on_hex(target_hex)
                .into_iter()
                .filter(|id| *id != player)
                .filter(|id| !self.game.players.get(*id).resources().empty())
                .collect::<Vec<_>>();

            let robbed_id = match candidates.as_slice() {
                [] => None,
                [only] => Some(*only),
                _ => loop {
                    let chosen = self.request_choose_player_to_rob(player).0;
                    if candidates.contains(&chosen) {
                        break Some(chosen);
                    }

                    log::warn!(
                        "Player#{} attempted to rob Player#{} who is not a legal target on {:?}",
                        player,
                        chosen,
                        target_hex
                    );
                },
            };

            match self.game.use_robbers(target_hex, player, robbed_id) {
                Ok(()) => break,
                Err(err) => {
                    log::warn!("Invalid robber move by Player#{}: {:?}", player, err);
                }
            }
        }
    }
}
