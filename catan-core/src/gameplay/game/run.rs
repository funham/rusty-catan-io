use serde::{Deserialize, Serialize};

use crate::gameplay::{primitives::player::PlayerId, random::GameRandom};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameResult {
    Win(PlayerId),
    Interrupted { reason: String },
    LimitReached { turns: u64 },
}

#[derive(Debug)]
pub struct RunOptions {
    pub max_turns: Option<u64>,
    pub max_invalid_actions: Option<u64>,
    pub random: GameRandom,
}

pub const DEFAULT_MAX_TURNS: u64 = 500;
pub const DEFAULT_MAX_INVALID_ACTIONS: u64 = 10;

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_turns: Some(DEFAULT_MAX_TURNS),
            max_invalid_actions: Some(DEFAULT_MAX_INVALID_ACTIONS),
            random: GameRandom::default(),
        }
    }
}
