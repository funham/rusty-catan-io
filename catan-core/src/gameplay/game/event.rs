use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        game::view::{GameSnapshot, GameView, PrivatePlayerData},
        primitives::{build::Build, dev_card::DevCardUsage, player::PlayerId},
    },
    math::dice::DiceVal,
};

pub struct SpectatorContext<'a> {
    pub view: &'a GameView,
}

pub struct PlayerContext<'a, 'b> {
    pub view: &'a GameView,
    pub player_data: &'b PrivatePlayerData,
}

pub struct AuthorizedContext<'a, 'b> {
    pub view: &'a GameView,
    pub snapshot: &'b GameSnapshot,
}

pub trait SpectatorObserver {
    fn on_event(&mut self, event: &GameEvent, context: &SpectatorContext);
}

pub trait PlayerObserver {
    fn player_id(&self) -> PlayerId;
    fn on_event(&mut self, event: &GameEvent, context: &PlayerContext) {
        let _ = (event, context);
    }
}

pub trait AuthorizedObserver {
    fn on_event(&mut self, event: &GameEvent, context: &AuthorizedContext);
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
