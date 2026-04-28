pub mod algorithm;
pub mod common;
pub mod gameplay;
pub mod math;
pub mod topology;

pub use gameplay::agent;

use crate::{
    gameplay::{
        agent::Agent,
        game::{
            controller::{GameController, GameResult, RunOptions},
            init::GameInitializationState,
            state::GameState,
        },
    },
    math::dice::DiceRoller,
};

// type Agents = [Box<dyn Agent>];
type AgentsOwned = Vec<Box<dyn Agent>>;

pub struct GameInitializer {
    state: GameInitializationState,
    agents: AgentsOwned,
}

pub struct GameRunner {
    state: GameState,
    agents: AgentsOwned,
}

impl GameInitializer {
    pub fn new(state: GameInitializationState, agents: AgentsOwned) -> Self {
        Self { state, agents }
    }

    pub fn init(self) -> GameRunner {
        let mut agents = self.agents;
        let state = GameController::init(self.state, &mut agents);
        GameRunner { state, agents }
    }
}

impl GameRunner {
    pub fn run(self, dice: &mut dyn DiceRoller) -> GameResult {
        let mut controller = GameController::new(self.state, self.agents);
        controller.run(dice)
    }

    pub fn run_with_options(self, dice: &mut dyn DiceRoller, options: RunOptions) -> GameResult {
        let mut controller = GameController::new(self.state, self.agents);
        controller.run_with_options(dice, options)
    }
}
