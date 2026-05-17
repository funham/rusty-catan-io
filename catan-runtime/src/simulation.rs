use std::collections::VecDeque;

use catan_agents::bot::BotPolicy;
use catan_core::gameplay::{
    game::{
        engine::GameEngine,
        init::GameInitializationState,
        input::{GameInput, PlayerCommand},
        output::GameOutput,
        projector,
        run::{GameResult, GameRunStats, RunOptions},
        view::{ContextFactory, SearchFactory, VisibilityConfig},
    },
    primitives::player::PlayerId,
};

pub struct SimulationHost {
    engine: GameEngine,
    bots: Vec<Box<dyn BotPolicy>>,
    visibility: VisibilityConfig,
}

impl SimulationHost {
    pub fn new(
        init: GameInitializationState,
        bots: Vec<Box<dyn BotPolicy>>,
        options: RunOptions,
    ) -> Self {
        Self {
            engine: GameEngine::from_init(init, options),
            bots,
            visibility: VisibilityConfig::default(),
        }
    }

    pub fn with_dice_seed(mut self, seed: u64) -> Self {
        self.engine.set_dice_seed(seed);
        self
    }

    pub fn run(&mut self) -> GameResult {
        let mut queue = VecDeque::new();
        let transition = match self.engine.start() {
            Ok(transition) => transition,
            Err(err) => {
                return GameResult::Interrupted {
                    reason: format!("engine start failed: {err:?}"),
                };
            }
        };
        queue.extend(projector::project_transaction(&transition.transaction));

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
            let player_id = decision.player_id;
            let Some(command) = self.command_for(player_id, &decision) else {
                return GameResult::Interrupted {
                    reason: format!("bot {player_id} did not produce a command"),
                };
            };

            let transition = match self.engine.apply(GameInput::Submit {
                player_id,
                decision_id: decision.id,
                command,
            }) {
                Ok(transition) => transition,
                Err(err) => {
                    return GameResult::Interrupted {
                        reason: format!("engine apply failed: {err:?}"),
                    };
                }
            };
            queue.extend(projector::project_transaction(&transition.transaction));
        }

        self.engine
            .result()
            .cloned()
            .expect("loop exits only when result is available")
    }

    pub fn run_stats(&self) -> GameRunStats {
        self.engine.run_stats()
    }

    fn command_for(
        &mut self,
        player_id: PlayerId,
        decision: &catan_core::gameplay::game::decision::OpenDecision,
    ) -> Option<PlayerCommand> {
        let policy = self.visibility.player_policy(player_id);
        let factory = ContextFactory {
            state: self.engine.state(),
            index: self.engine.index(),
            visibility: &self.visibility,
        };
        let search = Some(SearchFactory::new(self.engine.state(), policy, player_id));
        let context = factory.player_decision_context(player_id, search);
        self.bots[player_id.index()].command_for(decision, context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catan_agents::lazy::LazyAgent;

    #[test]
    fn lazy_bots_reach_turn_limit() {
        let init = GameInitializationState::default();
        let bots = (0..init.board.n_players)
            .map(|id| Box::new(LazyAgent::new(id)) as Box<dyn BotPolicy>)
            .collect();
        let mut host = SimulationHost::new(
            init,
            bots,
            RunOptions {
                max_turns: Some(3),
                ..RunOptions::default()
            },
        );

        assert!(matches!(host.run(), GameResult::LimitReached { turns: 3 }));
    }
}
