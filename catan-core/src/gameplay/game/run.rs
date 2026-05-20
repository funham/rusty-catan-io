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

// TODO: remove out of core completely, it belongs to it's domain in runtime or catan_bench
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRunStats {
    pub game_started: u64,
    pub turns_started: u64,
    pub turns_ended: u64,
    pub decision_requests: u64,
    pub regular_actions: u64,
    pub builds: u64,
    pub bank_trades: u64,
    pub dev_cards_bought: u64,
    pub dev_cards_used: u64,
    pub dice_rolls: u64,
    pub resources_distributed: u64,
    pub player_discards: u64,
    pub robber_moves: u64,
    pub action_rejections: u64,
    pub games_ended: u64,
    pub games_interrupted: u64,
}
