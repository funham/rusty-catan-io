use super::state::{BuildActionError, BuyDevCardError, DevCardUsageError, GameState};
use crate::agent::action::{
    self, ChoosePlayerToRobAction, DecisionRequest, DropHalfAction, InitAction, InitStageAction,
    MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction,
};
use crate::gameplay::agent::agent::Agent;
use crate::gameplay::game::event::{
    GameEndPlayerStats, GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
};
use crate::gameplay::game::index::GameIndex;
use crate::gameplay::game::init::GameInitializationState;
use crate::gameplay::game::query::GameQuery;
use crate::gameplay::game::view::{ContextFactory, SearchFactory, VisibilityConfig};
use crate::gameplay::primitives::bank::BankResourceExchangeError;
use crate::gameplay::primitives::build::{BuildingError, Establishment, EstablishmentType};
use crate::gameplay::primitives::dev_card::{DevCardUsage, UsableDevCard};
use crate::gameplay::primitives::player::PlayerId;
use crate::gameplay::primitives::resource::ResourceCollection;
use crate::gameplay::primitives::trade::{BankTrade, BankTradeKind};
use crate::gameplay::primitives::turn::GameTurn;
use crate::gameplay::primitives::{PortKind, Tile};
use crate::topology::Hex;
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
    pub max_invalid_actions: Option<u64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_turns: Some(500),
            max_invalid_actions: Some(10),
        }
    }
}

enum TurnFlow {
    Continue,
    EndTurn,
    GameEnded(PlayerId),
    Interrupted { reason: String },
}

#[derive(Debug)]
enum BankTradeExecutionError {
    MissingPort(PortKind),
    Exchange(BankResourceExchangeError),
}

impl std::fmt::Display for BankTradeExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPort(port) => write!(f, "missing port {port:?}"),
            Self::Exchange(err) => write!(f, "{err:?}"),
        }
    }
}

pub struct GameController {
    observers: Vec<Box<dyn GameObserver>>,
    game: GameState,
    index: GameIndex,
    players: Vec<Box<dyn Agent>>,
    visibility: VisibilityConfig,
    invalid_actions: u64,
    max_invalid_actions: Option<u64>,
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
            invalid_actions: 0,
            max_invalid_actions: RunOptions::default().max_invalid_actions,
        }
    }

    pub fn add_observer(&mut self, observer: Box<dyn GameObserver>) {
        log::trace!("Adding observer of kind: {:?}", observer.kind());
        self.observers.push(observer);
    }

    pub fn init(
        game_init: GameInitializationState,
        players: &mut Vec<Box<dyn Agent>>,
    ) -> GameState {
        Self::init_with_observers(game_init, players, &mut [])
    }

    pub fn init_with_observers(
        mut game_init: GameInitializationState,
        players: &mut Vec<Box<dyn Agent>>,
        observers: &mut [Box<dyn GameObserver>],
    ) -> GameState {
        log::trace!("Initializing game with {} players", players.len());
        Self::notify_observers_for_state(
            &game_init.clone().finish(),
            observers,
            &GameEvent::GameStarted,
        );

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

                let establishment = Establishment {
                    pos: action.establishment_position,
                    stage: EstablishmentType::Settlement,
                };

                match game_init
                    .builds
                    .try_init_place(player_id, action.road, establishment)
                {
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
                        if game_init.turn.get_rounds_played() == 1 {
                            Self::grant_second_initial_resources(
                                &mut game_init,
                                player_id,
                                establishment,
                            );
                        }
                        Self::notify_observers_for_state(
                            &game_init.clone().finish(),
                            observers,
                            &GameEvent::InitialPlacementBuilt {
                                player_id,
                                settlement: establishment.pos,
                                road: action.road,
                            },
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

    fn notify_observers_for_state(
        game: &GameState,
        observers: &mut [Box<dyn GameObserver>],
        event: &GameEvent,
    ) {
        log::trace!(
            "Notifying {} initialization observers of event: {:?}",
            observers.len(),
            event
        );
        let index = GameIndex::rebuild(game);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: game,
            index: &index,
            visibility: &visibility,
        };

        for observer in observers.iter_mut() {
            log::trace!(
                "Notifying initialization observer of kind {:?}",
                observer.kind()
            );
            log::trace!(
                target: "catan_core::observer_flow",
                "init emit event={:?} kind={:?} {}",
                event,
                observer.kind(),
                observer_state_summary(game)
            );
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

    fn grant_second_initial_resources(
        game_init: &mut GameInitializationState,
        player_id: PlayerId,
        settlement: Establishment,
    ) {
        let mut resources = ResourceCollection::ZERO;
        for hex in settlement
            .pos
            .as_set()
            .into_iter()
            .filter(|hex| hex.norm() <= game_init.board.arrangement.radius() as usize)
        {
            if let Tile::Resource { resource, .. } = game_init.board.arrangement[hex] {
                resources += &resource.into();
            }
        }

        let mut player = game_init.players.get_mut(player_id);
        let _ = ResourceCollection::transfer(
            &mut game_init.bank.resources,
            player.resources(),
            resources,
        );
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
            log::trace!(
                target: "catan_core::observer_flow",
                "game emit event={:?} kind={:?} {}",
                event,
                observer.kind(),
                observer_state_summary(game)
            );
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

    fn request_choose_player_to_rob(
        &mut self,
        player_id: PlayerId,
        robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        log::trace!(
            "Requesting choose-player-to-rob action from player {}",
            player_id
        );
        let policy = self.visibility.player_policy(player_id);
        let search = Some(SearchFactory::new(&self.game, policy, player_id));
        let factory = ContextFactory {
            state: &self.game,
            index: &self.index,
            visibility: &self.visibility,
        };
        let context = factory.player_decision_context(player_id, search);

        self.players[player_id]
            .as_mut()
            .choose_player_to_rob(context, robber_pos)
    }

    pub fn run(&mut self, dice: &mut dyn DiceRoller) -> GameResult {
        log::trace!("Starting game run with default options");
        self.run_with_options(
            dice,
            RunOptions {
                max_turns: None,
                ..RunOptions::default()
            },
        )
    }

    pub fn run_with_options(
        &mut self,
        dice: &mut dyn DiceRoller,
        options: RunOptions,
    ) -> GameResult {
        log::trace!("Starting game run with options: {:?}", options);
        self.invalid_actions = 0;
        self.max_invalid_actions = options.max_invalid_actions;
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
                self.notify_observers(&GameEvent::GameEnded {
                    winner_id: winner,
                    turn_no,
                    stats: self.game_end_stats(),
                });
                return GameResult::Win(winner);
            }

            match self.handle_turn(dice) {
                TurnFlow::Continue => unreachable!("turn handler should not yield Continue"),
                TurnFlow::GameEnded(winner) => {
                    log::trace!("Game ended during turn with winner: {}", winner);
                    self.notify_observers(&GameEvent::GameEnded {
                        winner_id: winner,
                        turn_no,
                        stats: self.game_end_stats(),
                    });
                    return GameResult::Win(winner);
                }
                TurnFlow::Interrupted { reason } => {
                    log::error!("Game interrupted: {reason}");
                    self.notify_observers(&GameEvent::GameInterrupted {
                        reason: reason.clone(),
                    });
                    return GameResult::Interrupted { reason };
                }
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
                if let TurnFlow::Interrupted { reason } = self.execute_dice_roll(dice) {
                    return TurnFlow::Interrupted { reason };
                }
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
                if let Err(err) = self.execute_dev_card(usage) {
                    log::error!(
                        "Invalid post-dice dev card action by Player#{}: usage={:?}, error={:?}",
                        self.curr_player(),
                        usage,
                        err
                    );
                    if let Some(flow) = self.record_invalid_action("post_dice_dev_card") {
                        return flow;
                    }
                    log::warn!("Dev card usage failed, retrying post-dice handling");
                    return self.handle_dice_rolled();
                }
                if let Some(flow) = self.win_flow_if_satisfied() {
                    return flow;
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
                    TurnFlow::GameEnded(winner) => TurnFlow::GameEnded(winner),
                    TurnFlow::Interrupted { reason } => TurnFlow::Interrupted { reason },
                }
            }
        }
    }

    fn handle_dev_card_used(&mut self, usage: DevCardUsage, dice: &mut dyn DiceRoller) -> TurnFlow {
        log::trace!("Handling dev card usage: {:?}", usage);
        if let Err(err) = self.execute_dev_card(usage) {
            log::error!(
                "Invalid dev card action by Player#{}: usage={:?}, error={:?}",
                self.curr_player(),
                usage,
                err
            );
            if let Some(flow) = self.record_invalid_action("dev_card") {
                return flow;
            }
            log::warn!("Dev card usage failed, retrying move init");
            return self.handle_move_init(dice);
        }
        if let Some(flow) = self.win_flow_if_satisfied() {
            return flow;
        }

        log::trace!("Dev card used successfully, requesting post-dev-card action");
        let _ = self.request_post_dev_card_action(self.curr_player());

        if let TurnFlow::Interrupted { reason } = self.execute_dice_roll(dice) {
            return TurnFlow::Interrupted { reason };
        }
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
                TurnFlow::GameEnded(winner) => return TurnFlow::GameEnded(winner),
                TurnFlow::Interrupted { reason } => return TurnFlow::Interrupted { reason },
            }
        }
    }

    fn execute_dev_card(&mut self, usage: DevCardUsage) -> Result<(), DevCardUsageError> {
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
                Ok(())
            }
            Err(err) => Err(err),
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
                match self.execute_build(current_player, build) {
                    Ok(()) => {
                        self.index = GameIndex::rebuild(&self.game);
                        self.notify_observers(&GameEvent::Built {
                            player_id: current_player,
                            build,
                        });
                        if let Some(flow) = self.win_flow_if_satisfied() {
                            return flow;
                        }
                    }
                    Err(err) => {
                        log::error!(
                            "Invalid build action by Player#{}: build={:?}, error={:?}",
                            current_player,
                            build,
                            err
                        );
                        if let Some(flow) = self.record_invalid_action("build") {
                            return flow;
                        }
                    }
                }
                TurnFlow::Continue
            }
            RegularAction::TradeWithBank(trade) => {
                log::trace!("Player trading with bank: {:?}", trade);
                match self.execute_trade_with_bank(current_player, trade) {
                    Ok(()) => {
                        self.notify_observers(&GameEvent::Traded {
                            player_id: current_player,
                        });
                    }
                    Err(err) => {
                        log::error!(
                            "Invalid bank trade action by Player#{}: trade={:?}, error={}",
                            current_player,
                            trade,
                            err
                        );
                        if let Some(flow) = self.record_invalid_action("bank_trade") {
                            return flow;
                        }
                    }
                }
                TurnFlow::Continue
            }
            RegularAction::BuyDevCard => {
                log::trace!("Player buying dev card");
                match self.execute_buy_dev_card(current_player) {
                    Ok(()) => {
                        self.notify_observers(&GameEvent::DevCardBought {
                            player_id: current_player,
                        });
                        if let Some(flow) = self.win_flow_if_satisfied() {
                            return flow;
                        }
                    }
                    Err(err) => {
                        log::error!(
                            "Invalid buy-dev-card action by Player#{}: error={:?}",
                            current_player,
                            err
                        );
                        if let Some(flow) = self.record_invalid_action("buy_dev_card") {
                            return flow;
                        }
                    }
                }
                TurnFlow::Continue
            }
            RegularAction::OfferPublicTrade(_) => {
                log::error!(
                    "Invalid public trade action by Player#{}: P2P trades not implemented",
                    current_player
                );
                if let Some(flow) = self.record_invalid_action("public_trade") {
                    return flow;
                }
                TurnFlow::Continue
            }
            RegularAction::OfferPersonalTrade(_) => {
                log::error!(
                    "Invalid personal trade action by Player#{}: P2P trades not implemented",
                    current_player
                );
                if let Some(flow) = self.record_invalid_action("personal_trade") {
                    return flow;
                }
                TurnFlow::Continue
            }
            RegularAction::EndMove => {
                log::trace!("Player ending move");
                TurnFlow::EndTurn
            }
        }
    }

    fn win_flow_if_satisfied(&self) -> Option<TurnFlow> {
        GameQuery::new(&self.game, &self.index)
            .check_win_condition()
            .map(TurnFlow::GameEnded)
    }

    fn record_invalid_action(&mut self, action_kind: &str) -> Option<TurnFlow> {
        self.invalid_actions += 1;
        log::warn!(
            "Invalid action count after {}: {}/{}",
            action_kind,
            self.invalid_actions,
            self.max_invalid_actions
                .map(|limit| limit.to_string())
                .unwrap_or_else(|| "unlimited".to_owned())
        );

        if let Some(limit) = self.max_invalid_actions
            && self.invalid_actions >= limit
        {
            let reason = format!("invalid action limit reached: {limit}");
            return Some(TurnFlow::Interrupted { reason });
        }

        None
    }

    fn execute_trade_with_bank(
        &mut self,
        player: PlayerId,
        trade: BankTrade,
    ) -> Result<(), BankTradeExecutionError> {
        log::trace!("Executing bank trade for player {}: {:?}", player, trade);

        let index = GameIndex::rebuild(&self.game);

        let ports = &index.ports_aquired[player];
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
                return Err(BankTradeExecutionError::MissingPort(required_port));
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
                Err(BankTradeExecutionError::Exchange(err))
            }
        }
    }

    fn execute_build(
        &mut self,
        player: PlayerId,
        build: crate::gameplay::primitives::build::Build,
    ) -> Result<(), BuildActionError> {
        log::trace!("Executing build for player {}: {:?}", player, build);
        match self.game.build(player, build) {
            Ok(()) => {
                log::trace!("Build successful");
                Ok(())
            }
            Err(err) => match err {
                BuildActionError::AccountIsShort { id } => {
                    log::warn!(
                        "Can't build {}: Player#{} has {}",
                        Into::<&str>::into(build),
                        id,
                        self.game.players.get(id).resources(),
                    );
                    Err(BuildActionError::AccountIsShort { id })
                }
                BuildActionError::InvalidPlacement(err) => {
                    log::warn!("Couldn't build {}: {:?}", Into::<&str>::into(build), err);
                    Err(BuildActionError::InvalidPlacement(err))
                }
                BuildActionError::OutOfPieces => {
                    log::warn!(
                        "couldn't build another {}: {player} ran out of these",
                        Into::<&str>::into(build)
                    );
                    Err(BuildActionError::OutOfPieces)
                }
            },
        }
    }

    fn execute_buy_dev_card(&mut self, player: PlayerId) -> Result<(), BuyDevCardError> {
        log::trace!("Executing buy dev card for player {}", player);
        match self.game.buy_dev_card(player) {
            Ok(()) => {
                log::trace!("Dev card purchase successful");
                Ok(())
            }
            Err(BuyDevCardError::BankIsShort) => {
                log::warn!("Bank out of dev cards");
                Err(BuyDevCardError::BankIsShort)
            }
            Err(BuyDevCardError::AccountIsShort { id }) => {
                log::warn!("Player#{} can't afford dev card", id);
                Err(BuyDevCardError::AccountIsShort { id })
            }
        }
    }

    fn execute_dice_roll(&mut self, dice: &mut dyn DiceRoller) -> TurnFlow {
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
                self.notify_observers(&GameEvent::ResourcesDistributed);
                TurnFlow::Continue
            }
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

    fn execute_seven(&mut self, player: PlayerId) -> TurnFlow {
        log::trace!("Executing seven handling for player {}", player);
        if let TurnFlow::Interrupted { reason } = self.execute_seven_discards(player) {
            return TurnFlow::Interrupted { reason };
        }
        self.execute_seven_robber(player)
    }

    fn execute_seven_discards(&mut self, player: PlayerId) -> TurnFlow {
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
                    log::error!(
                        "Player#{} attempted to discard {} cards, but must discard exactly {}",
                        pid,
                        dropped.total(),
                        required_drop
                    );
                    if let Some(flow) = self.record_invalid_action("discard") {
                        return flow;
                    }
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
                        log::error!(
                            "Player#{} attempted to discard resources they do not possess: {}",
                            pid,
                            dropped
                        );
                        if let Some(flow) = self.record_invalid_action("discard") {
                            return flow;
                        }
                    }

                    // TODO: refactor whole banking system (don't remove this comment)
                    Err(BankResourceExchangeError::BankIsShort) => unreachable!(),
                }
            }
        }
        TurnFlow::Continue
    }

    fn execute_seven_robber(&mut self, player: PlayerId) -> TurnFlow {
        log::trace!("Executing seven robber movement for player {}", player);
        loop {
            let MoveRobbersAction(target_hex) = self.request_move_robbers(player);

            /* validations */

            if target_hex == self.game.board_state.robber_pos {
                log::error!(
                    "Player#{} attempted to keep the robber on the same hex",
                    player
                );
                if let Some(flow) = self.record_invalid_action("robber_move") {
                    return flow;
                }
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
                    let ChoosePlayerToRobAction(chosen) =
                        self.request_choose_player_to_rob(player, target_hex);
                    if candidates.contains(&chosen) {
                        log::trace!("Player {} chose to rob player {}", player, chosen);
                        break Some(chosen);
                    }

                    log::error!(
                        "Player#{} attempted to rob Player#{} who is not a legal target on {:?}",
                        player,
                        chosen,
                        target_hex
                    );
                    if let Some(flow) = self.record_invalid_action("robber_target") {
                        return flow;
                    }
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
                    return TurnFlow::Continue;
                }
                Err(err) => {
                    log::error!("Invalid robber move by Player#{}: {:?}", player, err);
                    if let Some(flow) = self.record_invalid_action("robber_move") {
                        return flow;
                    }
                }
            }
        }
    }

    fn query(&self) -> GameQuery<'_> {
        GameQuery::new(&self.game, &self.index)
    }

    fn game_end_stats(&self) -> Vec<GameEndPlayerStats> {
        let query = self.query();
        (0..self.game.players.count())
            .map(|player_id| {
                let build_and_dev_card_vp = query.count_dev_card_build_vp(player_id);
                let has_longest_road = query.longest_road_owner() == Some(player_id);
                let has_largest_army = query.largest_army_owner() == Some(player_id);
                let award_vp = u16::from(has_longest_road) * 2 + u16::from(has_largest_army) * 3;
                let builds = self.game.builds.by_player(player_id);
                GameEndPlayerStats {
                    player_id,
                    total_vp: build_and_dev_card_vp + award_vp,
                    build_and_dev_card_vp,
                    award_vp,
                    settlements: builds.settlements_count() as u16,
                    cities: builds.cities_count() as u16,
                    roads: builds.roads_count() as u16,
                    longest_road_length: query.count_max_tract_length(player_id),
                    knights_used: self.game.players.get(player_id).dev_cards().used
                        [UsableDevCard::Knight],
                    has_longest_road,
                    has_largest_army,
                }
            })
            .collect()
    }
}

fn observer_state_summary(game: &GameState) -> String {
    let settlements: usize = (0..game.players.count())
        .map(|player_id| game.builds.by_player(player_id).settlements_count())
        .sum();
    let roads: usize = (0..game.players.count())
        .map(|player_id| game.builds.by_player(player_id).roads_count())
        .sum();
    let resources = game
        .players
        .iter()
        .enumerate()
        .map(|(player_id, player)| format!("p{player_id}:{}", player.resources().total()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "turn={} builds S:{settlements} R:{roads}; resources [{resources}]",
        game.turn.get_turns_played()
    )
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::{GameController, GameResult, RunOptions, TurnFlow};
    use crate::agent::action::{
        ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction, MoveRobbersAction,
        PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
    };
    use crate::gameplay::agent::agent::PlayerRuntime;
    use crate::gameplay::{
        game::{
            event::{
                GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
                PlayerNotification,
            },
            init::GameInitializationState,
            legal,
            state::GameState,
            view::PlayerDecisionContext,
        },
        primitives::{
            Tile,
            build::{Build, Establishment, EstablishmentType},
            dev_card::DevCardKind,
            player::PlayerId,
            resource::ResourceCollection,
        },
    };
    use crate::math::dice::{DiceRoller, DiceVal};
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

    #[test]
    fn second_initial_settlement_grants_adjacent_resources() {
        let mut init = GameInitializationState::default();
        let (settlement, _) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .find(|(settlement, _)| {
                settlement.pos.as_set().into_iter().any(|hex| {
                    hex.norm() <= init.board.arrangement.radius() as usize
                        && matches!(init.board.arrangement[hex], Tile::Resource { .. })
                })
            })
            .expect("default board should have a resource-adjacent initial placement");

        let expected = settlement.pos.as_set().into_iter().fold(
            ResourceCollection::ZERO,
            |mut resources, hex| {
                if hex.norm() <= init.board.arrangement.radius() as usize
                    && let Tile::Resource { resource, .. } = init.board.arrangement[hex]
                {
                    resources += &resource.into();
                }
                resources
            },
        );

        GameController::grant_second_initial_resources(&mut init, 0, settlement);

        assert_eq!(*init.players.get(0).resources(), expected);
    }

    #[test]
    fn turn_flow_interrupts_immediately_when_build_reaches_ten_vp() {
        let mut init = GameInitializationState::default();
        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .next()
            .expect("default board should have initial placements");
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");
        let mut state = init.finish();
        for _ in 0..8 {
            state
                .players
                .get_mut(0)
                .dev_cards_add(DevCardKind::VictoryPoint);
        }
        state
            .transfer_from_bank(
                ResourceCollection {
                    wheat: 2,
                    ore: 3,
                    ..ResourceCollection::ZERO
                },
                0,
            )
            .expect("bank should fund city build");

        let city = Build::Establishment(Establishment {
            pos: settlement.pos,
            stage: EstablishmentType::City,
        });
        let mut controller = GameController::new(state, Vec::new());

        let flow = controller.execute_regular_action(RegularAction::Build(city));

        assert!(matches!(flow, TurnFlow::GameEnded(0)));
    }

    #[derive(Debug)]
    struct FixedDice(DiceVal);

    impl DiceRoller for FixedDice {
        fn roll(&mut self) -> DiceVal {
            self.0
        }
    }

    struct InvalidBuyDevCardAgent {
        id: PlayerId,
        invalid_actions_before_end: Option<u64>,
    }

    impl PlayerNotification for InvalidBuyDevCardAgent {}

    impl PlayerRuntime for InvalidBuyDevCardAgent {
        fn player_id(&self) -> PlayerId {
            self.id
        }

        fn init_stage_action(&mut self, _context: PlayerDecisionContext<'_>) -> InitStageAction {
            unreachable!("initial stage is not used in this controller test")
        }

        fn init_action(&mut self, _context: PlayerDecisionContext<'_>) -> InitAction {
            InitAction::RollDice
        }

        fn after_dice_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDiceAction {
            match self.invalid_actions_before_end {
                Some(0) => PostDiceAction::RegularAction(RegularAction::EndMove),
                Some(ref mut remaining) => {
                    *remaining -= 1;
                    PostDiceAction::RegularAction(RegularAction::BuyDevCard)
                }
                None => PostDiceAction::RegularAction(RegularAction::BuyDevCard),
            }
        }

        fn after_dev_card_action(
            &mut self,
            _context: PlayerDecisionContext<'_>,
        ) -> PostDevCardAction {
            PostDevCardAction::RollDice
        }

        fn regular_action(&mut self, _context: PlayerDecisionContext<'_>) -> RegularAction {
            RegularAction::EndMove
        }

        fn move_robbers(&mut self, _context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
            unreachable!("fixed dice never rolls seven")
        }

        fn choose_player_to_rob(
            &mut self,
            _context: PlayerDecisionContext<'_>,
            _robber_pos: Hex,
        ) -> ChoosePlayerToRobAction {
            unreachable!("fixed dice never rolls seven")
        }

        fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
            TradeAnswer::Decline
        }

        fn drop_half(&mut self, _context: PlayerDecisionContext<'_>) -> DropHalfAction {
            unreachable!("fixed dice never rolls seven")
        }
    }

    struct RecordingObserver {
        events: Rc<RefCell<Vec<GameEvent>>>,
    }

    impl GameObserver for RecordingObserver {
        fn kind(&self) -> ObserverKind {
            ObserverKind::Spectator
        }

        fn on_event(&mut self, event: &GameEvent, _context: ObserverNotificationContext<'_>) {
            self.events.borrow_mut().push(event.clone());
        }
    }

    #[derive(Debug, Clone)]
    struct OmniscientRecord {
        event: GameEvent,
        settlements: usize,
        roads: usize,
        player_resource_totals: Vec<u16>,
    }

    struct RecordingOmniscientObserver {
        records: Rc<RefCell<Vec<OmniscientRecord>>>,
    }

    impl GameObserver for RecordingOmniscientObserver {
        fn kind(&self) -> ObserverKind {
            ObserverKind::Omniscient
        }

        fn on_event(&mut self, event: &GameEvent, context: ObserverNotificationContext<'_>) {
            let ObserverNotificationContext::Omniscient { full, .. } = context else {
                panic!("omniscient observer should receive omniscient context");
            };
            let settlements = (0..full.state.players.count())
                .map(|player_id| full.state.builds.by_player(player_id).settlements_count())
                .sum();
            let roads = (0..full.state.players.count())
                .map(|player_id| full.state.builds.by_player(player_id).roads_count())
                .sum();
            let player_resource_totals = full
                .state
                .players
                .iter()
                .map(|player| player.resources().total())
                .collect();

            self.records.borrow_mut().push(OmniscientRecord {
                event: event.clone(),
                settlements,
                roads,
                player_resource_totals,
            });
        }
    }

    struct LegalInitAgent {
        id: PlayerId,
    }

    impl PlayerNotification for LegalInitAgent {}

    impl PlayerRuntime for LegalInitAgent {
        fn player_id(&self) -> PlayerId {
            self.id
        }

        fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
            let (establishment, road) = legal::legal_initial_placements(&context)
                .into_iter()
                .next()
                .expect("default board should have legal initial placements");
            InitStageAction {
                establishment_position: establishment.pos,
                road,
            }
        }

        fn init_action(&mut self, _context: PlayerDecisionContext<'_>) -> InitAction {
            unreachable!("not used by initialization test")
        }

        fn after_dice_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDiceAction {
            unreachable!("not used by initialization test")
        }

        fn after_dev_card_action(
            &mut self,
            _context: PlayerDecisionContext<'_>,
        ) -> PostDevCardAction {
            unreachable!("not used by initialization test")
        }

        fn regular_action(&mut self, _context: PlayerDecisionContext<'_>) -> RegularAction {
            unreachable!("not used by initialization test")
        }

        fn move_robbers(&mut self, _context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
            unreachable!("not used by initialization test")
        }

        fn choose_player_to_rob(
            &mut self,
            _context: PlayerDecisionContext<'_>,
            _robber_pos: Hex,
        ) -> ChoosePlayerToRobAction {
            unreachable!("not used by initialization test")
        }

        fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
            unreachable!("not used by initialization test")
        }

        fn drop_half(&mut self, _context: PlayerDecisionContext<'_>) -> DropHalfAction {
            unreachable!("not used by initialization test")
        }
    }

    fn invalid_agents(
        first_invalid_actions_before_end: Option<u64>,
    ) -> Vec<Box<dyn crate::agent::Agent>> {
        (0..4)
            .map(|id| {
                Box::new(InvalidBuyDevCardAgent {
                    id,
                    invalid_actions_before_end: if id == 0 {
                        first_invalid_actions_before_end
                    } else {
                        Some(0)
                    },
                }) as Box<dyn crate::agent::Agent>
            })
            .collect()
    }

    #[test]
    fn init_with_observers_reports_initial_state_and_placements() {
        let mut agents = (0..4)
            .map(|id| Box::new(LegalInitAgent { id }) as Box<dyn crate::agent::Agent>)
            .collect::<Vec<_>>();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut observers = vec![Box::new(RecordingObserver {
            events: events.clone(),
        }) as Box<dyn GameObserver>];

        let state = GameController::init_with_observers(
            GameInitializationState::default(),
            &mut agents,
            &mut observers,
        );
        let events = events.borrow();

        assert!(matches!(events.first(), Some(GameEvent::GameStarted)));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, GameEvent::InitialPlacementBuilt { .. }))
                .count(),
            8
        );
        assert_eq!(state.builds.players().len(), 4);
    }

    #[test]
    fn omniscient_observer_receives_full_state_updates_during_initialization() {
        let mut agents = (0..4)
            .map(|id| Box::new(LegalInitAgent { id }) as Box<dyn crate::agent::Agent>)
            .collect::<Vec<_>>();
        let records = Rc::new(RefCell::new(Vec::new()));
        let mut observers = vec![Box::new(RecordingOmniscientObserver {
            records: records.clone(),
        }) as Box<dyn GameObserver>];

        GameController::init_with_observers(
            GameInitializationState::default(),
            &mut agents,
            &mut observers,
        );
        let records = records.borrow();

        assert!(matches!(
            records.first().map(|record| &record.event),
            Some(GameEvent::GameStarted)
        ));
        assert_eq!(records.first().unwrap().settlements, 0);
        assert_eq!(records.first().unwrap().roads, 0);
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(record.event, GameEvent::InitialPlacementBuilt { .. }))
                .count(),
            8
        );
        assert_eq!(records.last().unwrap().settlements, 8);
        assert_eq!(records.last().unwrap().roads, 8);
        assert!(
            records
                .iter()
                .any(|record| record.player_resource_totals.iter().any(|total| *total > 0)),
            "second setup round should grant at least one player initial resources"
        );
    }

    #[test]
    fn omniscient_observer_receives_full_state_updates_during_gameplay() {
        let (state, _, target_num) = game_with_settlement_on_numbered_hex();
        let records = Rc::new(RefCell::new(Vec::new()));
        let mut controller = GameController::new(state, invalid_agents(Some(0)));
        controller.add_observer(Box::new(RecordingOmniscientObserver {
            records: records.clone(),
        }));

        let result = controller.run_with_options(
            &mut FixedDice(target_num),
            RunOptions {
                max_turns: Some(1),
                max_invalid_actions: Some(10),
            },
        );
        let records = records.borrow();

        assert_eq!(result, GameResult::LimitReached { turns: 1 });
        assert!(
            records
                .iter()
                .any(|record| matches!(record.event, GameEvent::ResourcesDistributed))
        );
        assert!(
            records
                .iter()
                .any(|record| record.player_resource_totals.first() == Some(&1)),
            "omniscient observer should see player 0 receive harvested resources"
        );
    }

    #[test]
    fn invalid_action_limit_interrupts_game() {
        let state = GameInitializationState::default().finish();
        let mut controller = GameController::new(state, invalid_agents(None));
        let events = Rc::new(RefCell::new(Vec::new()));
        controller.add_observer(Box::new(RecordingObserver {
            events: events.clone(),
        }));
        let mut dice = FixedDice(DiceVal::try_from(8).unwrap());

        let result = controller.run_with_options(
            &mut dice,
            RunOptions {
                max_turns: Some(10),
                max_invalid_actions: Some(10),
            },
        );

        assert_eq!(
            result,
            GameResult::Interrupted {
                reason: "invalid action limit reached: 10".to_owned()
            }
        );
        assert!(events.borrow().iter().any(|event| {
            matches!(
                event,
                GameEvent::GameInterrupted { reason }
                    if reason == "invalid action limit reached: 10"
            )
        }));
    }

    #[test]
    fn invalid_actions_below_limit_do_not_interrupt() {
        let state = GameInitializationState::default().finish();
        let mut controller = GameController::new(state, invalid_agents(Some(9)));
        let mut dice = FixedDice(DiceVal::try_from(8).unwrap());

        let result = controller.run_with_options(
            &mut dice,
            RunOptions {
                max_turns: Some(1),
                max_invalid_actions: Some(10),
            },
        );

        assert_eq!(result, GameResult::LimitReached { turns: 1 });
    }
}
