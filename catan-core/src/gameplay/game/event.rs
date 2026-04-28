use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        game::view::{OmniscientGameView, PlayerNotificationContext, PublicGameView},
        primitives::{build::Build, dev_card::DevCardUsage, player::PlayerId},
    },
    math::dice::DiceVal,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverKind {
    Spectator,
    Omniscient,
}

pub enum ObserverNotificationContext<'a> {
    Spectator { public: PublicGameView<'a> },
    Omniscient {
        public: PublicGameView<'a>,
        full: OmniscientGameView<'a>,
    },
}

pub trait GameObserver {
    fn kind(&self) -> ObserverKind;
    fn on_event(&mut self, event: &GameEvent, context: ObserverNotificationContext<'_>);
}

pub trait PlayerNotification {
    fn on_event(&mut self, event: &GameEvent, context: PlayerNotificationContext<'_>) {
        let _ = (event, context);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    DiceRolled(DiceVal),
    DevCardBought,
    DevCardUsed(DevCardUsage),
    Built(Build),
    Traded,
    GameEnded { winner_id: PlayerId },
}
