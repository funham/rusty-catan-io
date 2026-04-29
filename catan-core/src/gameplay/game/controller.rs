use super::state::{BuildActionError, BuyDevCardError, GameState};
use crate::agent::action::{
    self, ChoosePlayerToRobAction, DecisionRequest, DropHalfAction, InitAction, InitStageAction,
    MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction,
};
use crate::gameplay::agent::agent::Agent;
use crate::gameplay::game::event::{
    GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
};
use crate::gameplay::game::index::GameIndex;
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::game::query::GameQuery;
use crate::gameplay::game::view::{ContextFactory, SearchFactory, VisibilityConfig};
use crate::gameplay::primitives::bank::BankResourceExchangeError;
use crate::gameplay::primitives::build::{BuildingError, Establishment, EstablishmentType};
use crate::gameplay::primitives::dev_card::DevCardUsage;
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::trade::{BankTrade, BankTradeKind};
use crate::gameplay::primitives::turn::GameTurn;
use crate::gameplay::primitives::{PortKind, Tile};
use crate::{math::dice::DiceRoller, math::dice::DiceVal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameResult {
    Win(PlayerId),
    Interrupted { reason: String },
    LimitReached { turns: u64 },
}

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub max_turns: Option<u64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_turns: Some(500),
        }
    }
}

enum TurnFlow {
    Continue,
    EndTurn,
}

pub struct GameController {
    observers: Vec<Box<dyn GameObserver>>,
    game: GameState,
    index: GameIndex,
    players: Vec<Box<dyn Agent>>,
    visibility: VisibilityConfig,
}

impl GameController {
    pub fn new(game: GameState, players: Vec<Box<dyn Agent>>) -> Self {
        log::trace!("Creating new GameController with {} players", players.len());
        Self::new_with_visibility(game, players, VisibilityConfig::default())
    }

    pub fn new_with_visibility(
        game: GameState,
        players: Vec<Box<dyn Agent>>,
        visibility: VisibilityConfig,
    ) -> Self {
        log::trace!("Creating new GameController with visibility config");
        let index = GameIndex::rebuild(&game);

        Self {
            observers: Vec::new(),
            game,
            index,
            players,
            visibility,
        }
    }

    pub fn add_observer(&mut self, observer: Box<dyn GameObserver>) {
        log::trace!("Adding observer of kind: {:?}", observer.kind());
        self.observers.push(observer);
    }

    pub fn init(
        mut game_init: GameInitializationState,
        players: &mut Vec<Box<dyn Agent>>,
    ) -> GameState {
        log::trace!("Initializing game with {} players", players.len());

        while game_init.turn.get_rounds_played() < 2 {
            let player_id = game_init.turn.get_turn_index();
            log::trace!(
                "Init round {}, player {}",
                game_init.turn.get_rounds_played(),
                player_id
            );

            loop {
                let state = game_init.clone().finish();
                let index = GameIndex::rebuild(&state);
                let factory = ContextFactory {
                    state: &state,
                    index: &index,
                    visibility: &VisibilityConfig::default(),
                };

                let action = InitStageAction::request(
                    players[player_id].as_mut(),
                    factory.player_decision_context(player_id, None),
                );
                log::trace!(
                    "Player {} requested init placement: road={:?}, settlement={:?}",
                    player_id,
                    action.road,
                    action.establishment_position
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
                                log::error!("invalid initial road placement {:?}", err);
                                log::warn!(
                                    "Player {} attempted invalid road placement, retrying",
                                    player_id
                                );
                            }
                            BuildingError::InitSettlement(_) => {
                                log::error!("invalid initial settlement placement {:?}", err);
                                log::warn!(
                                    "Player {} attempted invalid settlement placement, retrying",
                                    player_id
                                );
                            }
                            _ => unreachable!(),
                        };

                        continue;
                    }
                    Ok(()) => {
                        log::trace!(
                            "Player {} successfully placed initial settlement and road",
                            player_id
                        );
                        break;
                    }
                }
            }

            game_init.turn.next();
        }

        let n_players = game_init.board.n_players as u8;
        log::trace!("Game initialization complete, {} players", n_players);

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
        log::trace!("Notifying observers of event: {:?}", event);
        log::info!("Event: {:?}", event);

        let (game, index, visibility, players, observers) = (
            &self.game,
            &self.index,
            &self.visibility,
            &mut self.players,
            &mut self.observers,
        );
        let factory = ContextFactory {
            state: game,
            index,
            visibility,
        };

        log::trace!("Notifying {} players", players.len());
        for player in players.iter_mut() {
            let player_id = player.player_id();
            let cx = factory.player_notification_context(player_id);
            player.on_event(event, cx);
        }

        log::trace!("Notifying {} observers", observers.len());
        for observer in observers.iter_mut() {
            let cx = match observer.kind() {
                ObserverKind::Spectator => ObserverNotificationContext::Spectator {
                    public: factory.spectator_public_view(),
                },
                ObserverKind::Player(player_id) => ObserverNotificationContext::Player {
                    public: factory.public_view(visibility.player_policy(player_id)),
                    private: factory.private_view(player_id),
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
        let player = self.game.turn.get_turn_index();
        log::trace!("Current player: {}", player);
        player
    }

    fn request_dispatch<R: action::DecisionRequest>(&mut self, player_id: PlayerId) -> R {
        log::trace!("Dispatching decision request for player {}", player_id);
        let policy = self.visibility.player_policy(player_id);
        let search = Some(SearchFactory::new(&self.game, policy, player_id));
        let factory = ContextFactory {
            state: &self.game,
            index: &self.index,
            visibility: &self.visibility,
        };
        let context = factory.player_decision_context(player_id, search);

        R::request(self.players[player_id].as_mut(), context)
    }

    fn request_init_action(&mut self, player_id: PlayerId) -> InitAction {
        log::trace!("Requesting init action from player {}", player_id);
        self.request_dispatch::<InitAction>(player_id)
    }

    fn request_post_dice_action(&mut self, player_id: PlayerId) -> PostDiceAction {
        log::trace!("Requesting post-dice action from player {}", player_id);
        self.request_dispatch::<PostDiceAction>(player_id)
    }

    fn request_post_dev_card_action(&mut self, player_id: PlayerId) -> PostDevCardAction {
        log::trace!("Requesting post-dev-card action from player {}", player_id);
        self.request_dispatch::<PostDevCardAction>(player_id)
    }

    fn request_regular_action(&mut self, player_id: PlayerId) -> RegularAction {
        log::trace!("Requesting regular action from player {}", player_id);
        self.request_dispatch::<RegularAction>(player_id)
    }

    fn request_drop_half(&mut self, player_id: PlayerId) -> DropHalfAction {
        log::trace!("Requesting drop-half action from player {}", player_id);
        self.request_dispatch::<DropHalfAction>(player_id)
    }

    fn request_move_robbers(&mut self, player_id: PlayerId) -> MoveRobbersAction {
        log::trace!("Requesting move-robbers action from player {}", player_id);
        self.request_dispatch::<MoveRobbersAction>(player_id)
    }

    fn request_choose_player_to_rob(&mut self, player_id: PlayerId) -> ChoosePlayerToRobAction {
        log::trace!(
            "Requesting choose-player-to-rob action from player {}",
            player_id
        );
        self.request_dispatch::<ChoosePlayerToRobAction>(player_id)
    }

    pub fn run(&mut self, dice: &mut dyn DiceRoller) -> GameResult {
        log::trace!("Starting game run with default options");
        self.run_with_options(dice, RunOptions { max_turns: None })
    }

    pub fn run_with_options(
        &mut self,
        dice: &mut dyn DiceRoller,
        options: RunOptions,
    ) -> GameResult {
        log::trace!("Starting game run with options: {:?}", options);
        self.notify_observers(&GameEvent::GameStarted);
        loop {
            let turn_no = self.game.turn.get_turns_played();
            log::trace!("Starting turn {}", turn_no);

            if let Some(max_turns) = options.max_turns
                && turn_no >= max_turns
            {
                log::warn!("Turn limit reached ({}), stopping game", max_turns);
                self.notify_observers(&GameEvent::GameInterrupted {
                    reason: format!("turn limit reached ({max_turns})"),
                });
                return GameResult::LimitReached { turns: turn_no };
            }

            if let Some(winner) = GameQuery::new(&self.game, &self.index).check_win_condition() {
                log::trace!("Game ended with winner: {}", winner);
                self.notify_observers(&GameEvent::GameEnded { winner_id: winner });
                return GameResult::Win(winner);
            }

            match self.handle_turn(dice) {
                TurnFlow::Continue => unreachable!("turn handler should not yield Continue"),
                TurnFlow::EndTurn => {
                    let player_id = self.curr_player();
                    log::trace!(
                        "Ending turn for player {}, turn number {}",
                        player_id,
                        turn_no
                    );
                    self.notify_observers(&GameEvent::TurnEnded { player_id, turn_no });
                    self.game.turn.next();
                }
            }
        }
    }

    fn handle_turn(&mut self, dice: &mut dyn DiceRoller) -> TurnFlow {
        let current_player = self.curr_player();
        log::trace!("Handling turn for player {}", current_player);

        self.game
            .players
            .get_mut(current_player)
            .dev_cards_reset_queue();

        self.notify_observers(&GameEvent::TurnStarted {
            player_id: current_player,
            turn_no: self.game.turn.get_turns_played(),
        });

        self.handle_move_init(dice)
    }

    fn handle_move_init(&mut self, dice: &mut dyn DiceRoller) -> TurnFlow {
        log::trace!("Handling move init for player {}", self.curr_player());
        let answer = self.request_init_action(self.curr_player());

        match answer {
            InitAction::RollDice => {
                log::trace!("Player chose to roll dice");
                self.execute_dice_roll(dice);
                self.handle_dice_rolled()
            }
            InitAction::UseDevCard(usage) => {
                log::trace!("Player chose to use dev card: {:?}", usage);
                self.handle_dev_card_used(usage, dice)
            }
        }
    }

    fn handle_dice_rolled(&mut self) -> TurnFlow {
        log::trace!("Handling post-dice state for player {}", self.curr_player());
        let answer = self.request_post_dice_action(self.curr_player());

        match answer {
            PostDiceAction::UseDevCard(usage) => {
                log::trace!("Player chose to use dev card after rolling: {:?}", usage);
                if !self.execute_dev_card(usage) {
                    log::warn!("Dev card usage failed, retrying post-dice handling");
                    return self.handle_dice_rolled();
                }
                self.handle_rest()
            }
            PostDiceAction::RegularAction(action) => {
                log::trace!("Player chose regular action after rolling: {:?}", action);
                match self.execute_regular_action(action) {
                    TurnFlow::Continue => {
                        log::trace!("Regular action returned Continue, staying in post-dice state");
                        self.handle_dice_rolled()
                    }
                    TurnFlow::EndTurn => {
                        log::trace!("Regular action returned EndTurn");
                        TurnFlow::EndTurn
                    }
                }
            }
        }
    }

    fn handle_dev_card_used(&mut self, usage: DevCardUsage, dice: &mut dyn DiceRoller) -> TurnFlow {
        log::trace!("Handling dev card usage: {:?}", usage);
        if !self.execute_dev_card(usage) {
            log::warn!("Dev card usage failed, retrying move init");
            return self.handle_move_init(dice);
        }

        log::trace!("Dev card used successfully, requesting post-dev-card action");
        let _ = self.request_post_dev_card_action(self.curr_player());

        self.execute_dice_roll(dice);
        self.handle_rest()
    }

    fn handle_rest(&mut self) -> TurnFlow {
        log::trace!("Entering rest loop for player {}", self.curr_player());
        loop {
            let answer = self.request_regular_action(self.curr_player());
            log::trace!("Player chose regular action: {:?}", answer);

            match self.execute_regular_action(answer) {
                TurnFlow::Continue => {
                    log::trace!("Action returned Continue, continuing rest loop");
                    continue;
                }
                TurnFlow::EndTurn => {
                    log::trace!("Action returned EndTurn, exiting rest loop");
                    return TurnFlow::EndTurn;
                }
            }
        }
    }

    fn execute_dev_card(&mut self, usage: DevCardUsage) -> bool {
        let player_id = self.curr_player();
        log::trace!("Executing dev card for player {}: {:?}", player_id, usage);

        match self.game.use_dev_card(usage.clone(), player_id) {
            Ok(()) => {
                log::trace!("Dev card executed successfully");
                self.index = GameIndex::rebuild(&self.game);
                self.notify_observers(&GameEvent::DevCardUsed {
                    player_id,
                    usage: usage.clone(),
                });
                if let DevCardUsage::Knight { rob_hex, robbed_id } = usage {
                    log::trace!(
                        "Knight card moved robber to {:?}, robbed player: {:?}",
                        rob_hex,
                        robbed_id
                    );
                    self.notify_observers(&GameEvent::RobberMoved {
                        player_id,
                        hex: rob_hex,
                        robbed_id,
                    });
                }
                true
            }
            Err(err) => {
                log::warn!("Invalid dev card use by Player#{}: {:?}", player_id, err);
                false
            }
        }
    }

    fn execute_regular_action(&mut self, action: RegularAction) -> TurnFlow {
        let current_player = self.curr_player();
        log::trace!(
            "Executing regular action for player {}: {:?}",
            current_player,
            action
        );

        match action {
            RegularAction::Build(build) => {
                log::trace!("Player building: {:?}", build);
                if let Ok(()) = self.execute_build(current_player, build) {
                    self.index = GameIndex::rebuild(&self.game);
                    self.notify_observers(&GameEvent::Built {
                        player_id: current_player,
                        build,
                    });
                }
                TurnFlow::Continue
            }
            RegularAction::TradeWithBank(trade) => {
                log::trace!("Player trading with bank: {:?}", trade);
                if let Ok(()) = self.execute_trade_with_bank(current_player, trade) {
                    self.notify_observers(&GameEvent::Traded {
                        player_id: current_player,
                    });
                }
                TurnFlow::Continue
            }
            RegularAction::BuyDevCard => {
                log::trace!("Player buying dev card");
                if let Ok(()) = self.execute_buy_dev_card(current_player) {
                    self.notify_observers(&GameEvent::DevCardBought {
                        player_id: current_player,
                    });
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
            RegularAction::EndMove => {
                log::trace!("Player ending move");
                TurnFlow::EndTurn
            }
        }
    }

    fn execute_trade_with_bank(&mut self, player: PlayerId, trade: BankTrade) -> Result<(), ()> {
        log::trace!("Executing bank trade for player {}: {:?}", player, trade);
        let ports = self.game.board.ports_aquired(player);
        let required_port = match trade.kind {
            BankTradeKind::BankGeneric => None,
            BankTradeKind::PortGeneric => Some(PortKind::Universal),
            BankTradeKind::PortSpecific => Some(PortKind::Special(trade.give)),
        };

        if let Some(required_port) = required_port {
            if !ports.contains(&required_port) {
                log::warn!(
                    "Player#{} can't use {:?} bank trade without {:?}",
                    player,
                    trade.kind,
                    required_port
                );
                return Err(());
            }
        }

        match self.game.trade_with_bank(player, trade) {
            Ok(()) => {
                log::trace!("Bank trade successful for player {}", player);
                Ok(())
            }
            Err(err) => {
                match err {
                    BankResourceExchangeError::BankIsShort => {
                        log::error!("bank doesn't posess {:?}", trade.take)
                    }
                    BankResourceExchangeError::AccountIsShort { account, short } => {
                        log::error!("{account} doesn't posess {short}")
                    }
                };
                Err(())
            }
        }
    }

    fn execute_build(
        &mut self,
        player: PlayerId,
        build: crate::gameplay::primitives::build::Build,
    ) -> Result<(), ()> {
        log::trace!("Executing build for player {}: {:?}", player, build);
        match self.game.build(player, build) {
            Ok(()) => {
                log::trace!("Build successful");
                Ok(())
            }
            Err(BuildActionError::AccountIsShort { id }) => {
                log::warn!(
                    "Can't build {}: Player#{} has {}",
                    Into::<&str>::into(build),
                    id,
                    self.game.players.get(id).resources(),
                );
                Err(())
            }
            Err(BuildActionError::InvalidPlacement(err)) => {
                log::warn!("Couldn't build {}: {:?}", Into::<&str>::into(build), err);
                Err(())
            }
        }
    }

    fn execute_buy_dev_card(&mut self, player: PlayerId) -> Result<(), ()> {
        log::trace!("Executing buy dev card for player {}", player);
        match self.game.buy_dev_card(player) {
            Ok(()) => {
                log::trace!("Dev card purchase successful");
                Ok(())
            }
            Err(BuyDevCardError::BankIsShort) => {
                log::warn!("Bank out of dev cards");
                Err(())
            }
            Err(BuyDevCardError::AccountIsShort { id }) => {
                log::warn!("Player#{} can't afford dev card", id);
                Err(())
            }
        }
    }

    fn execute_dice_roll(&mut self, dice: &mut dyn DiceRoller) {
        let roll = dice.roll();
        let current_player = self.curr_player();

        log::trace!(
            "Player {} rolled {}",
            current_player,
            Into::<u8>::into(roll)
        );
        log::info!(
            "Player#[{}] rolled {}",
            current_player,
            Into::<u8>::into(roll)
        );

        self.notify_observers(&GameEvent::DiceRolled {
            player_id: current_player,
            value: roll,
        });

        match roll {
            seven if seven == DiceVal::seven() => {
                log::trace!("Rolled a 7, executing seven handling");
                self.execute_seven(current_player)
            }
            other => {
                log::trace!("Rolled {}, executing harvesting", Into::<u8>::into(other));
                Self::execute_harvesting(&mut self.game, current_player, other);
            }
        }

        if roll != DiceVal::seven() {
            self.notify_observers(&GameEvent::ResourcesDistributed);
        }
    }

    fn execute_harvesting(game: &mut GameState, player: PlayerId, num: DiceVal) {
        log::trace!(
            "Executing harvesting for dice roll {}",
            Into::<u8>::into(num)
        );
        let hexes = game.board.hexes_by_num(num).clone();

        let player_ids = {
            let index = GameIndex::rebuild(game);
            GameQuery::new(game, &index).player_ids_starting_from(player)
        };
        log::trace!("Harvesting order: {:?}", player_ids);

        for pid in player_ids {
            for est in game.builds[pid].establishments.clone() {
                let coinc = est.pos.as_set();

                for hex in coinc.intersection(&hexes) {
                    if *hex == game.board_state.robber_pos {
                        log::trace!("Hex {:?} is blocked by robber, skipping", hex);
                        continue;
                    }

                    match game.board.arrangement[*hex] {
                        Tile::Resource { resource, .. } => {
                            let amount = est.stage.harvest_amount() as u16;
                            log::trace!(
                                "Player {} gets {} of {:?} from hex {:?}",
                                pid,
                                amount,
                                resource,
                                hex
                            );
                            let _ = game.transfer_from_bank((resource, amount).into(), pid);
                        }
                        Tile::Desert => {
                            log::trace!("Hex {:?} is desert, no resource", hex);
                        }
                        Tile::River { .. } => {
                            log::trace!("Hex {:?} is river, no resource", hex);
                        }
                    }
                }
            }
        }
    }

    fn execute_seven(&mut self, player: PlayerId) {
        log::trace!("Executing seven handling for player {}", player);
        self.execute_seven_discards(player);
        self.execute_seven_robber(player);
    }

    fn execute_seven_discards(&mut self, player: PlayerId) {
        log::trace!("Executing seven discards starting from player {}", player);
        for pid in GameQuery::new(&self.game, &self.index).player_ids_starting_from(player) {
            let total_cards = self.game.players.get(pid).resources().total();
            if total_cards <= 7 {
                log::trace!(
                    "Player {} has {} cards, no discard needed",
                    pid,
                    total_cards
                );
                continue;
            }

            let required_drop = total_cards / 2;
            log::trace!(
                "Player {} has {} cards, must discard {}",
                pid,
                total_cards,
                required_drop
            );

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
                    Ok(()) => {
                        log::trace!("Player {} successfully discarded {}", pid, dropped);
                        self.notify_observers(&GameEvent::PlayerDiscarded {
                            player_id: pid,
                            resources: dropped,
                        });
                        break;
                    }
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
        log::trace!("Executing seven robber movement for player {}", player);
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
                .query()
                .players_on_hex(target_hex)
                .into_iter()
                .filter(|id| *id != player)
                .filter(|id| !self.game.players.get(*id).resources().is_empty())
                .collect::<Vec<_>>();

            log::trace!(
                "Players on hex {:?} that can be robbed: {:?}",
                target_hex,
                candidates
            );

            let robbed_id = match candidates.as_slice() {
                [] => {
                    log::trace!("No players to rob on hex {:?}", target_hex);
                    None
                }
                [only] => {
                    log::trace!("Only one candidate to rob: {}", only);
                    Some(*only)
                }
                _ => loop {
                    let chosen = self.request_choose_player_to_rob(player).0;
                    if candidates.contains(&chosen) {
                        log::trace!("Player {} chose to rob player {}", player, chosen);
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
                Ok(()) => {
                    log::trace!(
                        "Robber moved to {:?}, robbed player: {:?}",
                        target_hex,
                        robbed_id
                    );
                    self.notify_observers(&GameEvent::RobberMoved {
                        player_id: player,
                        hex: target_hex,
                        robbed_id,
                    });
                    break;
                }
                Err(err) => {
                    log::warn!("Invalid robber move by Player#{}: {:?}", player, err);
                }
            }
        }
    }

    fn query(&self) -> GameQuery<'_> {
        GameQuery::new(&self.game, &self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::GameController;
    use crate::gameplay::{
        game::init::GameInitializationState,
        game::state::GameState,
        primitives::{Tile, build::EstablishmentType},
    };
    use crate::topology::Hex;

    fn game_with_settlement_on_numbered_hex() -> (GameState, Hex, crate::math::dice::DiceVal) {
        let mut init = GameInitializationState::default();
        let (target_hex, target_num) = init
            .board
            .arrangement
            .hex_enum_iter()
            .find_map(|(hex, tile)| match tile {
                Tile::Resource { number, .. } => Some((hex, number)),
                Tile::Desert => None,
                Tile::River { .. } => None,
            })
            .expect("default board should include resource tiles");

        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .find(|(settlement, _)| settlement.pos.as_set().contains(&target_hex))
            .expect("target resource hex should have a legal adjacent settlement");

        assert_eq!(settlement.stage, EstablishmentType::Settlement);
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");

        (init.finish(), target_hex, target_num)
    }

    #[test]
    fn harvest_pays_resources_without_robber() {
        let (mut game, _, target_num) = game_with_settlement_on_numbered_hex();

        GameController::execute_harvesting(&mut game, 0, target_num);

        assert_eq!(game.players.get(0).resources().total(), 1);
    }

    #[test]
    fn robber_blocks_resource_harvest() {
        let (mut game, target_hex, target_num) = game_with_settlement_on_numbered_hex();
        game.board_state.robber_pos = target_hex;

        GameController::execute_harvesting(&mut game, 0, target_num);

        assert_eq!(game.players.get(0).resources().total(), 0);
    }
}
