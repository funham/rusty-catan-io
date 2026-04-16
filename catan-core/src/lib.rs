use serde::{Deserialize, Serialize};

pub mod common;
pub mod gameplay;
pub mod math;
pub mod topology;

pub use gameplay::agent;

use crate::{
    gameplay::{
        agent::Agent,
        game::{controller::GameController, init::GameInitializationState, state::GameState},
    },
    math::dice::DiceRoller,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum GameEvent {}

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

    pub fn init_game(self) -> GameRunner {
        let mut agents = self.agents;
        let state = GameController::init(self.state, &mut agents);

        GameRunner { state, agents }
    }
}

impl GameRunner {
    pub fn run(&mut self, dice: &mut dyn DiceRoller) {
        GameController::run(&mut self.state, &mut self.agents, dice);
    }
}
