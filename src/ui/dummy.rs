use super::*;

pub struct Dummy;

impl Dumper for Dummy {
    type Output = ();

    fn state(&mut self, _state: &crate::gameplay::game::state::GameState) -> &mut Self {
        self
    }

    fn context(&mut self, _context: Context) -> &mut Self {
        self
    }

    fn flush(&mut self) -> &mut Self {
        self
    }

    fn dump(&mut self) -> Self::Output {
        ()
    }
}

impl Sender for Dummy {
    type Input = ();

    fn send(&mut self, _input: &Self::Input) {}
}
