pub mod dummy;
pub mod json_dumper;

use crate::{gameplay::game::state::GameState, math::dice::DiceVal};

pub enum Context {
    Dice(DiceVal),
    Animation,
}

pub trait Dumper {
    /// Type of dumped data e.g. JSON
    type Output;

    /// (re-)set game state info
    fn state(&mut self, state: &GameState) -> &mut Self;

    /// add context information e.g. action
    fn context(&mut self, context: Context) -> &mut Self;

    /// remove all previously added info
    fn flush(&mut self) -> &mut Self;

    /// dump added state and context AND flush
    fn dump(&mut self) -> Self::Output;
}

pub trait Sender {
    /// must match Dumper::Output
    type Input;

    /// send data to an interface (e.g. pring to the console or send via socket)
    fn send(&mut self, input: &Self::Input);
}
