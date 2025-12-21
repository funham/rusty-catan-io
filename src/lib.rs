pub mod common;
pub mod gameplay;
pub mod math;
pub mod topology;

pub use gameplay::strategy;

use clap::Parser;

use crate::gameplay::{
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
    strats: Vec<Box<dyn strategy::Strategy>>,
    game: GameState,
}

impl GameStarter {
    pub fn new(args: Args) -> Self {
        let n_players = args.strategies.len();
        let mut strats: Vec<Box<dyn strategy::Strategy>> = Vec::new();
        for _strat_name in args.strategies {
            log::warn!("todo: implement strategy table");
            strats.push(Box::new(
                strategy::lazy_ass_strategy::LazyAssStrategy::default(),
            ));
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
