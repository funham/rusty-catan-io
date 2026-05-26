use std::collections::VecDeque;

use catan_bots::bot::BotPolicy;
use catan_core::gameplay::{
    game::{
        engine::GameEngine,
        input::PlayerCommand,
        output::GameOutput,
        projector,
        run::{GameResult, RunOptions},
        state::SetupGameState,
        view::{ContextFactory, SearchFactory, VisibilityConfig},
    },
    primitives::player::PlayerId,
};

use crate::sync_host::{ObserverFrame, OutputObserver};

pub struct SimulationHost {
    engine: GameEngine,
    bots: Vec<Box<dyn BotPolicy>>,
    visibility: VisibilityConfig,
    observers: Vec<Box<dyn OutputObserver>>,
}

impl SimulationHost {
    pub fn new(init: SetupGameState, bots: Vec<Box<dyn BotPolicy>>, options: RunOptions) -> Self {
        Self {
            engine: GameEngine::from_init(init, options),
            bots,
            visibility: VisibilityConfig::default(),
            observers: Vec::new(),
        }
    }

    pub fn add_observer(&mut self, observer: Box<dyn OutputObserver>) {
        self.observers.push(observer);
    }

    pub fn run(&mut self) -> GameResult {
        let mut queue = VecDeque::new();
        let transaction = match self.engine.start() {
            Ok(transaction) => transaction,
            Err(err) => {
                return GameResult::Interrupted {
                    reason: format!("engine start failed: {err:?}"),
                };
            }
        };
        self.project_and_deliver(&transaction, &mut queue);

        let mut steps = 0_u64;
        while self.engine.result().is_none() {
            steps += 1;
            if steps > 1_000_000 {
                return GameResult::Interrupted {
                    reason: "simulation step limit reached".to_owned(),
                };
            }

            let Some(output) = queue.pop_front() else {
                return GameResult::Interrupted {
                    reason: "simulation is waiting without pending bot commands".to_owned(),
                };
            };

            let GameOutput::DecisionOpened(decision) = output else {
                continue;
            };
            let player_id = decision.player_id();
            let Some(command) = self.command_for(player_id, &decision) else {
                return GameResult::Interrupted {
                    reason: format!("bot {player_id} did not produce a command"),
                };
            };

            let Some(response) = decision.respond_command(command) else {
                return GameResult::Interrupted {
                    reason: format!("bot {player_id} produced a command for the wrong decision"),
                };
            };

            let transaction = match self.engine.submit(response) {
                Ok(transaction) => transaction,
                Err(err) => {
                    return GameResult::Interrupted {
                        reason: format!("engine apply failed: {err:?}"),
                    };
                }
            };
            self.project_and_deliver(&transaction, &mut queue);
        }

        self.engine
            .result()
            .cloned()
            .expect("loop exits only when result is available")
    }

    fn command_for(
        &mut self,
        player_id: PlayerId,
        decision: &catan_core::gameplay::game::input::DecisionRequest,
    ) -> Option<PlayerCommand> {
        let policy = self.visibility.player_policy(player_id);
        let factory = ContextFactory {
            state: self.engine.table(),
            index: self.engine.index(),
            visibility: &self.visibility,
            trade_sessions: self.engine.trade_sessions(),
        };
        let search = Some(SearchFactory::new(self.engine.table(), policy, player_id));
        let context = factory.player_decision_context(player_id, search);
        self.bots[player_id.index()].command_for(decision, context)
    }

    fn project_and_deliver(
        &mut self,
        transaction: &catan_core::gameplay::game::event::EventTransaction,
        queue: &mut VecDeque<GameOutput>,
    ) {
        for output in projector::project_transaction(transaction) {
            self.deliver_output(&output);
            queue.push_back(output);
        }
    }

    fn deliver_output(&mut self, output: &GameOutput) {
        let factory = ContextFactory {
            state: self.engine.table(),
            index: self.engine.index(),
            visibility: &self.visibility,
            trade_sessions: self.engine.trade_sessions(),
        };
        for observer in &mut self.observers {
            observer.on_output(ObserverFrame {
                output,
                factory: &factory,
                engine: &self.engine,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catan_bots::lazy::LazyAgent;

    #[test]
    fn lazy_bots_reach_turn_limit() {
        let init = SetupGameState::default();
        let bots = (0..init.board.n_players)
            .map(|id| {
                Box::new(LazyAgent::new(
                    catan_core::gameplay::primitives::player::PlayerId::try_from(id)
                        .expect("test player id should fit"),
                )) as Box<dyn BotPolicy>
            })
            .collect();
        let mut host = SimulationHost::new(
            init,
            bots,
            RunOptions {
                max_turns: Some(3),
                ..RunOptions::default()
            },
        );
        let (stats_observer, stats_handle) = crate::run_stats::RunStatsObserver::new();
        host.add_observer(Box::new(stats_observer));

        assert!(matches!(host.run(), GameResult::LimitReached { turns: 3 }));
        let stats = stats_handle.stats();
        assert_eq!(stats.game_started, 1);
        assert_eq!(stats.turns_started, 3);
        assert_eq!(stats.games_interrupted, 1);
    }
}
