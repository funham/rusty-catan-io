use serde::{Deserialize, Serialize};

pub mod common;
pub mod gameplay;
pub mod math;
pub mod topology;

pub use gameplay::agent;

use crate::gameplay::{
    agent::Agent,
    game::{init::GameInitializationState, state::GameState},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum GameEvent {}

type Agents = [Box<dyn Agent>];
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
        todo!()
    }

    pub fn init_game(&mut self) -> GameRunner {
        todo!("init game logic");

        GameRunner {
            state: self.state.promote(),
            agents: self.agents,
        }
    }
}

impl GameRunner {
    pub fn run(&mut self) {
        todo!()
    }
}

/*

use clap::Parser;

use crate::gameplay::{
    agent::agent::Agent,
    field::state::FieldBuildParam,
    game::{
        controller::{GameController, GameResult},
        init::GameInitializationState,
        state::GameState,
    },
};

/// A simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// strategy names
    #[arg(short, long)]
    pub strategies: Vec<String>,

    /// field size
    #[arg(short, long)]
    pub field_size: usize,

    /// filename from which field arrangement will be taken
    #[arg(short, long)]
    pub arrangement: String,

    /// dice option
    #[arg(short, long)]
    pub dice: String,
}

pub struct GameStarter {
    strats: Vec<Box<dyn Agent>>,
    game: GameState,
}

impl GameStarter {
    pub fn new(args: Args) -> Self {
        let n_players = args.strategies.len();
        let mut strats: Vec<Box<dyn Agent>> = Vec::new();
        for strat_name in args.strategies {
            strats.push(agent::agent::AgentFactory::fetch(&strat_name));
        }

        let field_build_param = FieldBuildParam::try_new(
            n_players,
            6,
            todo!("read from file"),
            todo!("read from file"),
        )
        .expect("Couldn't build the field: invalid arguments");

        let game_init = GameInitializationState::new(field_build_param);
        let game = GameController::init(game_init, &mut strats, todo!());

        Self { strats, game }
    }

    pub fn run(mut self) -> GameResult {
        GameController::run(&mut self.game, &mut self.strats)
    }
}

*/
