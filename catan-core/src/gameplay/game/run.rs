use serde::{Deserialize, Serialize};

use crate::gameplay::{game::event::GameEvent, primitives::player::PlayerId, random::GameRandom};

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

impl GameRunStats {
    pub(crate) fn record_event(&mut self, event: &GameEvent) {
        match event {
            GameEvent::GameStarted => self.game_started += 1,
            GameEvent::DecisionOpened(decision)
                if !matches!(
                    decision.kind,
                    crate::gameplay::game::decision::DecisionKind::InitPlacement
                ) =>
            {
                self.decision_requests += 1
            }
            GameEvent::DecisionOpened(_) => {}
            GameEvent::DecisionClosed { .. } => {}
            GameEvent::CommandRejected {
                counts_toward_limit: true,
                ..
            } => self.action_rejections += 1,
            GameEvent::CommandRejected { .. } => {}
            GameEvent::TurnStarted { .. } => self.turns_started += 1,
            GameEvent::TurnEnded { .. } => self.turns_ended += 1,
            GameEvent::InitialPlacementBuilt { .. } => {}
            GameEvent::DiceRolled { .. } => self.dice_rolls += 1,
            GameEvent::ResourcesDistributed { .. } => self.resources_distributed += 1,
            GameEvent::DevCardBought { .. } => self.dev_cards_bought += 1,
            GameEvent::DevCardDrawn { .. } => {}
            GameEvent::DevCardUsed { .. } => self.dev_cards_used += 1,
            GameEvent::Built { .. } => self.builds += 1,
            GameEvent::Traded { .. } => self.bank_trades += 1,
            GameEvent::TradeOpened { .. }
            | GameEvent::TradeOfferAdded { .. }
            | GameEvent::TradeResponseUpdated { .. }
            | GameEvent::TradeCompleted { .. }
            | GameEvent::TradeCancelled { .. } => {}
            GameEvent::PlayerDiscarded { .. } => self.player_discards += 1,
            GameEvent::RobberMoved { .. } => self.robber_moves += 1,
            GameEvent::ResourceStolen { .. } => {}
            GameEvent::ActionRejected { .. } => self.action_rejections += 1,
            GameEvent::GameEnded { .. } => self.games_ended += 1,
            GameEvent::GameInterrupted { .. } => self.games_interrupted += 1,
            GameEvent::GameFinished { result } => match result {
                GameResult::Win(_) => self.games_ended += 1,
                GameResult::Interrupted { .. } | GameResult::LimitReached { .. } => {
                    self.games_interrupted += 1
                }
            },
        }
    }
}
