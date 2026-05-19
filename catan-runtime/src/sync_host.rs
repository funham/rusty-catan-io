use std::collections::VecDeque;

use catan_agents::bot::BotPolicy;
use catan_core::gameplay::{
    game::{
        engine::GameEngine,
        init::GameInitializationState,
        input::{DecisionRequest, DecisionResponse},
        output::GameOutput,
        projector,
        run::{GameResult, RunOptions},
        view::{ContextFactory, PlayerDecisionContext, SearchFactory, VisibilityConfig},
    },
    primitives::player::PlayerId,
};

pub struct SeatFrame<'a> {
    pub player_id: PlayerId,
    pub output: &'a GameOutput,
    pub view: PlayerDecisionContext<'a>,
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
        if let Some(command) = self.policy.command_for(decision, frame.view) {
            if let Some(response) = decision.respond_command(command) {
                commands.push(SeatCommand { response });
            }
        }
    }
}

pub struct SyncGameHost {
    engine: GameEngine,
    seats: Vec<Box<dyn Seat>>,
    observers: Vec<Box<dyn OutputObserver>>,
    visibility: VisibilityConfig,
    outputs: VecDeque<GameOutput>,
    inputs: VecDeque<SeatCommand>,
}

impl SyncGameHost {
    pub fn new(
        init: GameInitializationState,
        seats: Vec<Box<dyn Seat>>,
        options: RunOptions,
    ) -> Self {
        Self::from_engine(GameEngine::from_init(init, options), seats)
    }

    pub fn from_engine(engine: GameEngine, seats: Vec<Box<dyn Seat>>) -> Self {
        Self {
            engine,
            seats,
            observers: Vec::new(),
            visibility: VisibilityConfig::default(),
            outputs: VecDeque::new(),
            inputs: VecDeque::new(),
        }
    }

    pub fn add_observer(&mut self, observer: Box<dyn OutputObserver>) {
        self.observers.push(observer);
    }

    pub fn start(&mut self) {
        if self.engine.is_started() {
            for decision in self.engine.pending_decisions() {
                self.outputs.push_back(GameOutput::DecisionOpened(
                    DecisionRequest::from_open_decision(decision),
                ));
            }
        } else {
            let transition = self.engine.start().expect("engine start should reduce");
            self.outputs
                .extend(projector::project_transaction(&transition.transaction));
        }
    }

    pub fn submit(&mut self, command: SeatCommand) {
        self.inputs.push_back(command);
    }

    pub fn run_until_waiting(&mut self) -> Option<GameResult> {
        loop {
            if let Some(output) = self.outputs.pop_front() {
                self.deliver_output(&output);
                continue;
            }
            if let Some(result) = self.engine.result().cloned() {
                return Some(result);
            }
            if let Some(input) = self.inputs.pop_front() {
                let transition = self
                    .engine
                    .submit(input.response)
                    .expect("engine submit should reduce");
                self.outputs
                    .extend(projector::project_transaction(&transition.transaction));
                continue;
            }
            return None;
        }
    }

    pub fn run_to_result(&mut self) -> GameResult {
        loop {
            if let Some(result) = self.run_until_waiting() {
                return result;
            }
            return GameResult::Interrupted {
                reason: "host is waiting for external input".to_owned(),
            };
        }
    }

    fn deliver_output(&mut self, output: &GameOutput) {
        let factory = ContextFactory {
            state: self.engine.table(),
            index: self.engine.index(),
            visibility: &self.visibility,
        };
        for observer in &mut self.observers {
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
            };
            let mut buffer = SeatCommandBuffer::default();
            self.seats[index].on_frame(frame, &mut buffer);
            buffer.drain_into(&mut self.inputs);
        }
    }

    pub fn run_stats(&self) -> catan_core::gameplay::game::run::GameRunStats {
        self.engine.run_stats()
    }
}

pub fn bot_seat(policy: Box<dyn BotPolicy>) -> Box<dyn Seat> {
    Box::new(BotSeat::new(policy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use catan_agents::{bot::decline_trade_command, lazy::LazyAgent};
    use catan_core::gameplay::{
        game::{decision::DecisionKind, event::GameEvent, input::DecisionRequest},
        random::GameRandom,
    };

    const P0: PlayerId = PlayerId::new(0);

    #[test]
    fn bot_seat_responds_immediately() {
        let init = GameInitializationState::default();
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

        host.start();
        assert!(matches!(
            host.run_to_result(),
            GameResult::LimitReached { turns: 1 }
        ));
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
        let init = GameInitializationState::default();
        let seats: Vec<Box<dyn Seat>> = vec![Box::new(RecordingSeat {
            id: P0,
            decision: None,
        })];
        let mut host = SyncGameHost::new(init, seats, RunOptions::default());

        host.start();
        assert!(host.run_until_waiting().is_none());
    }

    #[test]
    fn trade_response_command_is_representable_for_seats() {
        let _ = decline_trade_command();
        let _ = DecisionKind::TradeResponse {
            session: catan_core::gameplay::game::trade::TradeSessionId(0),
        };
    }

    struct CountingObserver {
        outputs: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl OutputObserver for CountingObserver {
        fn on_output(&mut self, _frame: ObserverFrame<'_>) {
            self.outputs.set(self.outputs.get() + 1);
        }
    }

    #[test]
    fn output_observer_receives_engine_outputs() {
        let init = GameInitializationState::default();
        let seats = (0..init.board.n_players)
            .map(|id| {
                bot_seat(Box::new(LazyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )))
            })
            .collect();
        let output_count = std::rc::Rc::new(std::cell::Cell::new(0));
        let observer = Box::new(CountingObserver {
            outputs: output_count.clone(),
        });
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                random: GameRandom::seeded(0),
                ..RunOptions::default()
            },
        );

        host.add_observer(observer);
        host.start();
        let _ = host.run_until_waiting();

        assert!(output_count.get() > 0);
    }

    struct EventRecordingObserver {
        events: std::rc::Rc<std::cell::RefCell<Vec<GameEvent>>>,
    }

    impl OutputObserver for EventRecordingObserver {
        fn on_output(&mut self, frame: ObserverFrame<'_>) {
            if let GameOutput::Event(record) = frame.output {
                self.events.borrow_mut().push(record.event.clone());
            }
        }
    }

    #[test]
    fn terminal_outputs_are_delivered_before_host_returns_result() {
        let init = GameInitializationState::default();
        let seats = (0..init.board.n_players)
            .map(|id| {
                bot_seat(Box::new(LazyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )))
            })
            .collect();
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let observer = Box::new(EventRecordingObserver {
            events: events.clone(),
        });
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                max_turns: Some(0),
                random: GameRandom::seeded(0),
                ..RunOptions::default()
            },
        );

        host.add_observer(observer);
        host.start();
        let _ = host.run_to_result();

        assert!(events.borrow().iter().any(|event| matches!(
            event,
            GameEvent::GameFinished {
                result: GameResult::LimitReached { turns: 0 },
                stats: None,
            }
        )));
    }
}
