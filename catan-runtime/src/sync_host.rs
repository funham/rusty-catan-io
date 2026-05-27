use std::collections::VecDeque;

use catan_bots::bot::BotPolicy;
use catan_core::gameplay::{
    game::{
        engine::GameEngine,
        input::{DecisionRequest, DecisionResponse},
        output::GameOutput,
        projector,
        run::{GameResult, RunOptions},
        state::SetupGameState,
        view::{ContextFactory, PlayerDecisionContext, SearchFactory, VisibilityConfig},
    },
    primitives::player::PlayerId,
};

pub struct SeatFrame<'a> {
    pub player_id: PlayerId,
    pub output: &'a GameOutput,
    pub view: PlayerDecisionContext<'a>,
    pub dev_card_used_this_turn: bool,
}

#[derive(Debug, Clone)]
pub struct SeatCommand {
    pub response: DecisionResponse,
}

#[derive(Debug, Default)]
pub struct SeatCommandBuffer {
    commands: Vec<SeatCommand>,
}

impl SeatCommandBuffer {
    pub fn push(&mut self, command: SeatCommand) {
        self.commands.push(command);
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    fn drain_into(self, queue: &mut VecDeque<SeatCommand>) {
        queue.extend(self.commands);
    }
}

pub trait Seat {
    fn player_id(&self) -> PlayerId;
    fn on_frame(&mut self, frame: SeatFrame<'_>, commands: &mut SeatCommandBuffer);
}

pub struct ObserverFrame<'a> {
    pub output: &'a GameOutput,
    pub factory: &'a ContextFactory<'a>,
    pub engine: &'a GameEngine,
}

pub trait OutputObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>);
}

pub struct BotSeat {
    policy: Box<dyn BotPolicy>,
}

impl BotSeat {
    pub fn new(policy: Box<dyn BotPolicy>) -> Self {
        Self { policy }
    }
}

impl Seat for BotSeat {
    fn player_id(&self) -> PlayerId {
        self.policy.player_id()
    }

    fn on_frame(&mut self, frame: SeatFrame<'_>, commands: &mut SeatCommandBuffer) {
        let GameOutput::DecisionOpened(decision) = frame.output else {
            return;
        };
        if decision.player_id() != self.player_id() {
            return;
        }
        if let Some(command) = self.policy.command_for(decision, frame.view)
            && let Some(response) = decision.respond_command(command)
        {
            commands.push(SeatCommand { response });
        }
    }
}

pub struct SyncGameHost {
    engine: GameEngine,
    seats: Vec<Box<dyn Seat>>,
    visibility: VisibilityConfig,
    outputs: VecDeque<GameOutput>,
    inputs: VecDeque<SeatCommand>,
}

impl SyncGameHost {
    pub fn new(init: SetupGameState, seats: Vec<Box<dyn Seat>>, options: RunOptions) -> Self {
        Self::from_engine(GameEngine::from_init(init, options), seats)
    }

    pub fn from_engine(engine: GameEngine, seats: Vec<Box<dyn Seat>>) -> Self {
        Self {
            engine,
            seats,
            visibility: VisibilityConfig::default(),
            outputs: VecDeque::new(),
            inputs: VecDeque::new(),
        }
    }

    pub fn start(&mut self) {
        if self.engine.is_started() {
            for decision in self.engine.pending_decisions() {
                self.outputs.push_back(GameOutput::DecisionOpened(
                    DecisionRequest::from_open_decision(decision),
                ));
            }
        } else {
            let transaction = self.engine.start().expect("engine start should reduce");
            self.outputs
                .extend(projector::project_transaction(&transaction));
        }
    }

    pub fn submit(&mut self, command: SeatCommand) {
        self.inputs.push_back(command);
    }

    pub fn run_until_waiting(&mut self) -> Option<GameResult> {
        let mut observers: [&mut dyn OutputObserver; 0] = [];
        self.run_until_waiting_observed(&mut observers)
    }

    pub fn run_until_waiting_observed(
        &mut self,
        observers: &mut [&mut dyn OutputObserver],
    ) -> Option<GameResult> {
        loop {
            if let Some(input) = self.inputs.pop_front() {
                let transaction = self
                    .engine
                    .submit(input.response)
                    .expect("engine submit should reduce");
                self.prepend_outputs(projector::project_transaction(&transaction));
                self.enqueue_rejected_pending_decisions(&transaction);
                continue;
            }
            if let Some(output) = self.outputs.pop_front() {
                self.deliver_output(&output, observers);
                continue;
            }
            if let Some(result) = self.engine.result().cloned() {
                return Some(result);
            }
            return None;
        }
    }

    pub fn run_to_result(&mut self) -> GameResult {
        let mut observers: [&mut dyn OutputObserver; 0] = [];
        self.run_to_result_observed(&mut observers)
    }

    pub fn run_to_result_observed(
        &mut self,
        observers: &mut [&mut dyn OutputObserver],
    ) -> GameResult {
        self.run_until_waiting_observed(observers)
            .unwrap_or_else(|| GameResult::Interrupted {
                reason: "host is waiting for external input".to_owned(),
            })
    }

    fn deliver_output(&mut self, output: &GameOutput, observers: &mut [&mut dyn OutputObserver]) {
        let factory = ContextFactory {
            state: self.engine.table(),
            index: self.engine.index(),
            visibility: &self.visibility,
            trade_sessions: self.engine.trade_sessions(),
        };
        for observer in observers.iter_mut() {
            observer.on_output(ObserverFrame {
                output,
                factory: &factory,
                engine: &self.engine,
            });
        }

        for index in 0..self.seats.len() {
            let player_id = self.seats[index].player_id();
            let policy = self.visibility.player_policy(player_id);
            let search = Some(SearchFactory::new(self.engine.table(), policy, player_id));
            let frame = SeatFrame {
                player_id,
                output,
                view: factory.player_decision_context(player_id, search),
                dev_card_used_this_turn: self.engine.dev_card_used_this_turn(),
            };
            let mut buffer = SeatCommandBuffer::default();
            self.seats[index].on_frame(frame, &mut buffer);
            buffer.drain_into(&mut self.inputs);
        }
    }

    fn prepend_outputs<I>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = GameOutput>,
        I::IntoIter: DoubleEndedIterator,
    {
        for output in outputs.into_iter().rev() {
            self.outputs.push_front(output);
        }
    }

    fn enqueue_rejected_pending_decisions(
        &mut self,
        transaction: &catan_core::gameplay::game::event::EventTransaction,
    ) {
        for event in &transaction.events {
            let catan_core::gameplay::game::event::GameEvent::CommandRejected { token, .. } = event
            else {
                continue;
            };
            if let Some(decision) = self
                .engine
                .pending_decisions()
                .find(|decision| decision.id == token.id && decision.player_id == token.player_id)
            {
                self.outputs.push_back(GameOutput::DecisionOpened(
                    DecisionRequest::from_open_decision(decision),
                ));
            }
        }
    }
}

pub fn bot_seat(policy: Box<dyn BotPolicy>) -> Box<dyn Seat> {
    Box::new(BotSeat::new(policy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use catan_bots::{bot::decline_trade_command, greedy::GreedyAgent, lazy::LazyAgent};
    use catan_core::gameplay::{
        game::{
            command::RegularCommand,
            decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
            event::GameEvent,
            input::{DecisionRequest, DecisionResponse, PlayerCommand},
        },
        primitives::{resource::Resource, trade::PlayerTrade},
        random::GameRandom,
    };

    const P0: PlayerId = PlayerId::new(0);

    #[test]
    fn bot_seat_responds_immediately() {
        let init = SetupGameState::default();
        let seats = (0..init.board.n_players)
            .map(|id| {
                bot_seat(Box::new(LazyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )))
            })
            .collect();
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                max_turns: Some(1),
                random: GameRandom::seeded(0),
                ..RunOptions::default()
            },
        );
        let mut stats_observer = crate::run_stats::RunStatsObserver::new();
        let mut observers: [&mut dyn OutputObserver; 1] = [&mut stats_observer];

        host.start();
        assert!(matches!(
            host.run_to_result_observed(&mut observers),
            GameResult::LimitReached { turns: 1 }
        ));
        let stats = stats_observer.stats();
        assert_eq!(stats.game_started, 1);
        assert_eq!(stats.games_interrupted, 1);
    }

    struct RecordingSeat {
        id: PlayerId,
        decision: Option<DecisionRequest>,
    }

    impl Seat for RecordingSeat {
        fn player_id(&self) -> PlayerId {
            self.id
        }

        fn on_frame(&mut self, frame: SeatFrame<'_>, _commands: &mut SeatCommandBuffer) {
            if let GameOutput::DecisionOpened(decision) = frame.output
                && decision.player_id() == self.id
            {
                self.decision = Some(decision.clone());
            }
        }
    }

    #[test]
    fn human_like_seat_can_submit_later() {
        let init = SetupGameState::default();
        let seats: Vec<Box<dyn Seat>> = vec![Box::new(RecordingSeat {
            id: P0,
            decision: None,
        })];
        let mut host = SyncGameHost::new(init, seats, RunOptions::default());

        host.start();
        assert!(host.run_until_waiting().is_none());
    }

    struct InvalidOnceSeat {
        id: PlayerId,
        submitted: bool,
        opened: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl Seat for InvalidOnceSeat {
        fn player_id(&self) -> PlayerId {
            self.id
        }

        fn on_frame(&mut self, frame: SeatFrame<'_>, commands: &mut SeatCommandBuffer) {
            let GameOutput::DecisionOpened(decision) = frame.output else {
                return;
            };
            if decision.player_id() != self.id {
                return;
            }

            self.opened.set(self.opened.get() + 1);
            if self.submitted {
                return;
            }
            self.submitted = true;
            commands.push(SeatCommand {
                response: DecisionResponse {
                    token: decision.token(),
                    command: PlayerCommand::Regular(RegularCommand::EndMove),
                },
            });
        }
    }

    #[test]
    fn rejected_command_reopens_same_pending_decision() {
        let opened = std::rc::Rc::new(std::cell::Cell::new(0));
        let seats: Vec<Box<dyn Seat>> = vec![Box::new(InvalidOnceSeat {
            id: P0,
            submitted: false,
            opened: opened.clone(),
        })];
        let mut host = SyncGameHost::new(SetupGameState::default(), seats, RunOptions::default());

        host.start();
        assert!(host.run_until_waiting().is_none());

        assert_eq!(opened.get(), 2);
    }

    #[test]
    fn trade_response_command_is_representable_for_seats() {
        let _ = decline_trade_command();
        let _ = DecisionKind::TradeResponse {
            session: catan_core::gameplay::game::trade::TradeSessionId(0),
        };
    }

    #[derive(Default)]
    struct CountingObserver {
        outputs: usize,
    }

    impl OutputObserver for CountingObserver {
        fn on_output(&mut self, _frame: ObserverFrame<'_>) {
            self.outputs += 1;
        }
    }

    #[test]
    fn output_observer_receives_engine_outputs() {
        let init = SetupGameState::default();
        let seats = (0..init.board.n_players)
            .map(|id| {
                bot_seat(Box::new(LazyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )))
            })
            .collect();
        let mut observer = CountingObserver::default();
        let mut observers: [&mut dyn OutputObserver; 1] = [&mut observer];
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                random: GameRandom::seeded(0),
                ..RunOptions::default()
            },
        );

        host.start();
        let _ = host.run_until_waiting_observed(&mut observers);

        assert!(observer.outputs > 0);
    }

    #[test]
    fn greedy_bots_emit_player_trade_events() {
        let init = SetupGameState::default();
        let seats = (0..init.board.n_players)
            .map(|id| {
                bot_seat(Box::new(GreedyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )))
            })
            .collect();
        let mut observer = EventRecordingObserver::default();
        let mut observers: [&mut dyn OutputObserver; 1] = [&mut observer];
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                max_turns: Some(100),
                random: GameRandom::seeded(0),
                ..RunOptions::default()
            },
        );

        host.start();
        let _ = host.run_until_waiting_observed(&mut observers);

        let events = &observer.events;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, GameEvent::TradeOpened { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, GameEvent::TradeResponseUpdated { .. }))
        );
        assert!(events.iter().any(|event| matches!(
            event,
            GameEvent::TradeCompleted { .. } | GameEvent::TradeCancelled { .. }
        )));
    }

    struct TradeOpeningSeat {
        id: PlayerId,
        opened_trade: bool,
        owner_action_saw_responses: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl Seat for TradeOpeningSeat {
        fn player_id(&self) -> PlayerId {
            self.id
        }

        fn on_frame(&mut self, frame: SeatFrame<'_>, commands: &mut SeatCommandBuffer) {
            let GameOutput::DecisionOpened(decision) = frame.output else {
                return;
            };
            if decision.player_id() != self.id {
                return;
            }

            match decision.kind() {
                DecisionKind::RegularCommand if !self.opened_trade => {
                    self.opened_trade = true;
                    let command = PlayerCommand::Regular(RegularCommand::OfferTrade(PlayerTrade {
                        give: Resource::Brick.into(),
                        take: Resource::Ore.into(),
                    }));
                    let response = decision
                        .respond_command(command)
                        .expect("regular trade command should match regular decision");
                    commands.push(SeatCommand { response });
                }
                DecisionKind::RegularCommand => {
                    let response = decision
                        .respond_command(PlayerCommand::Regular(RegularCommand::EndMove))
                        .expect("end move should match regular decision");
                    commands.push(SeatCommand { response });
                }
                DecisionKind::TradeOwnerAction { .. } => {
                    let response_count = frame
                        .view
                        .public
                        .trade_sessions
                        .last()
                        .map(|session| {
                            session
                                .responses
                                .iter()
                                .filter(|response| {
                                    !matches!(
                                        response,
                                        Some(
                                            catan_core::gameplay::game::trade::TradeResponseState::Waiting
                                        ) | None
                                    )
                                })
                                .count()
                        })
                        .unwrap_or_default();
                    self.owner_action_saw_responses.set(response_count);
                    let response = decision
                        .respond_command(PlayerCommand::Trade(
                            catan_core::gameplay::game::input::TradeCommand::Cancel,
                        ))
                        .expect("cancel should match trade owner decision");
                    commands.push(SeatCommand { response });
                }
                _ => {}
            }
        }
    }

    #[test]
    fn queued_bot_trade_responses_are_applied_before_owner_prompt() {
        let mut state = SetupGameState::default().finish();
        *state.players.get_mut(P0).resources() += Resource::Brick.into();
        let mut engine = GameEngine::new(state);
        engine
            .apply_event(&GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(0),
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                lifetime: DecisionLifetime::OneShot,
            }))
            .expect("test decision should apply");

        let owner_action_saw_responses = std::rc::Rc::new(std::cell::Cell::new(0));
        let seats: Vec<Box<dyn Seat>> = vec![
            Box::new(TradeOpeningSeat {
                id: P0,
                opened_trade: false,
                owner_action_saw_responses: owner_action_saw_responses.clone(),
            }),
            bot_seat(Box::new(LazyAgent::new(PlayerId::new(1)))),
            bot_seat(Box::new(LazyAgent::new(PlayerId::new(2)))),
            bot_seat(Box::new(LazyAgent::new(PlayerId::new(3)))),
        ];
        let mut host = SyncGameHost::from_engine(engine, seats);

        host.start();
        let _ = host.run_until_waiting();

        assert_eq!(owner_action_saw_responses.get(), 3);
    }

    #[derive(Default)]
    struct EventRecordingObserver {
        events: Vec<GameEvent>,
    }

    impl OutputObserver for EventRecordingObserver {
        fn on_output(&mut self, frame: ObserverFrame<'_>) {
            if let GameOutput::Event(record) = frame.output {
                self.events.push(record.event.clone());
            }
        }
    }

    #[test]
    fn terminal_outputs_are_delivered_before_host_returns_result() {
        let init = SetupGameState::default();
        let seats = (0..init.board.n_players)
            .map(|id| {
                bot_seat(Box::new(LazyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )))
            })
            .collect();
        let mut observer = EventRecordingObserver::default();
        let mut observers: [&mut dyn OutputObserver; 1] = [&mut observer];
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                max_turns: Some(0),
                random: GameRandom::seeded(0),
                ..RunOptions::default()
            },
        );

        host.start();
        let _ = host.run_to_result_observed(&mut observers);

        assert!(observer.events.iter().any(|event| matches!(
            event,
            GameEvent::GameEnded {
                result: GameResult::LimitReached { turns: 0 },
            }
        )));
    }
}
