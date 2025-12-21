pub mod common;
pub mod gameplay;
pub mod math;
pub mod topology;

pub use gameplay::strategy;

use clap::Parser;

use crate::gameplay::game::{
    controller::{GameController, GameResult},
    init::GameInitializationState,
    state::GameState,
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
}

pub struct GameStarter {
    strats: Vec<Box<dyn strategy::Strategy>>,
    game: GameState,
}

impl GameStarter {
    pub fn new(args: Args) -> Self {
        let mut strats: Vec<Box<dyn strategy::Strategy>> = Vec::new();
        for _strat_name in args.strategies {
            strats.push(Box::new(
                strategy::lazy_ass_strategy::LazyAssStrategy::default(),
            ));
        }

        let game_init = GameInitializationState::new(args.field_size);
        let game = GameController::init(game_init);

        Self { strats, game }
    }

    pub fn run(mut self) -> GameResult {
        GameController::run(&mut self.game, &mut self.strats)
    }
}
