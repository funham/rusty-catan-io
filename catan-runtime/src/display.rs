use catan_core::{GameEvent, GameObserver};

pub trait RuntimeDisplay: GameObserver {}

impl<T: GameObserver> RuntimeDisplay for T {}

#[derive(Default)]
pub struct DisplayFanout {
    displays: Vec<Box<dyn RuntimeDisplay>>,
}

impl DisplayFanout {
    pub fn new(displays: Vec<Box<dyn RuntimeDisplay>>) -> Self {
        Self { displays }
    }
}

impl GameObserver for DisplayFanout {
    fn on_event(&mut self, event: &GameEvent) {
        for display in &mut self.displays {
            display.on_event(event);
        }
    }
}
