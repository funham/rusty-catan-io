use std::collections::VecDeque;

use catan_agents::bot::BotPolicy;
use catan_core::gameplay::{
    game::{
        decision::DecisionId,
        engine::GameEngine,
        init::GameInitializationState,
        input::{GameInput, PlayerCommand},
        output::{GameOutput, VecOutputSink},
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
    pub player_id: PlayerId,
    pub decision_id: DecisionId,
    pub command: PlayerCommand,
}

#[derive(Debug, Default)]
pub struct SeatCommandBuffer {
    commands: Vec<SeatCommand>,
}

impl SeatCommandBuffer {
    pub fn push(&mut self, command: SeatCommand) {
        self.commands.push(command);
    }

    fn drain_into(self, queue: &mut VecDeque<SeatCommand>) {
        queue.extend(self.commands);
    }
}

pub trait Seat {
    fn player_id(&self) -> PlayerId;
    fn on_frame(&mut self, frame: SeatFrame<'_>, commands: &mut SeatCommandBuffer);
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
        if decision.player_id != self.player_id() {
            return;
        }
        if let Some(command) = self.policy.command_for(decision, frame.view) {
            commands.push(SeatCommand {
                player_id: decision.player_id,
                decision_id: decision.id,
                command,
            });
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
    pub fn new(
        init: GameInitializationState,
        seats: Vec<Box<dyn Seat>>,
        options: RunOptions,
    ) -> Self {
        Self {
            engine: GameEngine::from_init(init, options),
            seats,
            visibility: VisibilityConfig::default(),
            outputs: VecDeque::new(),
            inputs: VecDeque::new(),
        }
    }

    pub fn with_dice_seed(mut self, seed: u64) -> Self {
        self.engine.set_dice_seed(seed);
        self
    }

    pub fn start(&mut self) {
        let mut sink = VecOutputSink::default();
        self.engine.start(&mut sink);
        self.outputs.extend(sink.into_vec());
    }

    pub fn submit(&mut self, command: SeatCommand) {
        self.inputs.push_back(command);
    }

    pub fn run_until_waiting(&mut self) -> Option<GameResult> {
        loop {
            if let Some(result) = self.engine.result().cloned() {
                return Some(result);
            }
            if let Some(output) = self.outputs.pop_front() {
                self.deliver_output(&output);
                continue;
            }
            if let Some(input) = self.inputs.pop_front() {
                let mut sink = VecOutputSink::default();
                self.engine.apply(
                    GameInput::Submit {
                        player_id: input.player_id,
                        decision_id: input.decision_id,
                        command: input.command,
                    },
                    &mut sink,
                );
                self.outputs.extend(sink.into_vec());
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
        for index in 0..self.seats.len() {
            let player_id = self.seats[index].player_id();
            let policy = self.visibility.player_policy(player_id);
            let factory = ContextFactory {
                state: self.engine.state(),
                index: self.engine.index(),
                visibility: &self.visibility,
            };
            let search = Some(SearchFactory::new(self.engine.state(), policy, player_id));
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
    use catan_core::gameplay::game::decision::{DecisionKind, OpenDecision};

    #[test]
    fn bot_seat_responds_immediately() {
        let init = GameInitializationState::default();
        let seats = (0..init.board.n_players)
            .map(|id| bot_seat(Box::new(LazyAgent::new(id))))
            .collect();
        let mut host = SyncGameHost::new(
            init,
            seats,
            RunOptions {
                max_turns: Some(1),
                ..RunOptions::default()
            },
        )
        .with_dice_seed(0);

        host.start();
        assert!(matches!(
            host.run_to_result(),
            GameResult::LimitReached { turns: 1 }
        ));
    }

    struct RecordingSeat {
        id: PlayerId,
        decision: Option<OpenDecision>,
    }

    impl Seat for RecordingSeat {
        fn player_id(&self) -> PlayerId {
            self.id
        }

        fn on_frame(&mut self, frame: SeatFrame<'_>, _commands: &mut SeatCommandBuffer) {
            if let GameOutput::DecisionOpened(decision) = frame.output
                && decision.player_id == self.id
            {
                self.decision = Some(decision.clone());
            }
        }
    }

    #[test]
    fn human_like_seat_can_submit_later() {
        let init = GameInitializationState::default();
        let seats: Vec<Box<dyn Seat>> = vec![Box::new(RecordingSeat {
            id: 0,
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
}
