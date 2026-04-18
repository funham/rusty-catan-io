use serde::{Deserialize, Serialize};

pub mod common;
pub mod gameplay;
pub mod math;
pub mod topology;

pub use gameplay::agent;

use crate::{
    gameplay::{
        agent::Agent,
        game::{
            controller::GameController,
            init::GameInitializationState,
            state::{GameSnapshot, GameState},
        },
        primitives::{build::Build, player::PlayerId, resource::ResourceCollection},
    },
    math::dice::{DiceRoller, DiceVal},
    topology::Hex,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum GameEvent {
    GameStarted { snapshot: GameSnapshot },
    TurnStarted { snapshot: GameSnapshot },
    DiceRolled {
        player_id: PlayerId,
        value: DiceVal,
        snapshot: GameSnapshot,
    },
    BuildPlaced {
        player_id: PlayerId,
        build: Build,
        snapshot: GameSnapshot,
    },
    PlayerDiscarded {
        player_id: PlayerId,
        discarded: ResourceCollection,
        snapshot: GameSnapshot,
    },
    RobberMoved {
        player_id: PlayerId,
        hex: Hex,
        robbed_id: Option<PlayerId>,
        snapshot: GameSnapshot,
    },
    GameFinished {
        winner_id: PlayerId,
        snapshot: GameSnapshot,
    },
}

pub trait GameObserver {
    fn on_event(&mut self, event: &GameEvent);
}

#[derive(Debug, Default)]
pub struct NoopObserver;

impl GameObserver for NoopObserver {
    fn on_event(&mut self, _: &GameEvent) {}
}

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

    pub fn init_game_with_observer(self, observer: &mut dyn GameObserver) -> GameRunner {
        let mut agents = self.agents;
        let state = GameController::init_with_observer(self.state, &mut agents, observer);

        GameRunner { state, agents }
    }
}

impl GameRunner {
    pub fn run(&mut self, dice: &mut dyn DiceRoller) {
        let mut observer = NoopObserver;
        GameController::run_with_observer(&mut self.state, &mut self.agents, dice, &mut observer);
    }

    pub fn run_with_observer(
        &mut self,
        dice: &mut dyn DiceRoller,
        observer: &mut dyn GameObserver,
    ) {
        GameController::run_with_observer(&mut self.state, &mut self.agents, dice, observer);
    }
}
