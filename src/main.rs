use clap::Parser;
use rusty_catan_io::Args;
use rusty_catan_io::GameStarter;
use rusty_catan_io::gameplay::game::controller::GameResult;

fn main() {
    let args = Args::parse();
    println!("Chosen strategies: {:?}", args.strategies);
    let starter = GameStarter::new(args);

    match starter.run() {
        GameResult::Win(id) => println!("Congrats! player #{} won!", id + 1),
        GameResult::Interrupted => println!("The game was interrupted"),
    }
}
