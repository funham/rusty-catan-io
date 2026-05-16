use smallvec::SmallVec;

use crate::gameplay::{
    game::{
        decision::PendingDecisions,
        index::GameIndex,
        phase::GamePhase,
        run::{GameResult, GameRunStats},
        state::GameState,
        trade::TradeSession,
    },
    primitives::player::PlayerId,
};

pub type TradeSessions = SmallVec<[TradeSession; 16]>;
pub type PendingDiscards = SmallVec<[PlayerId; 8]>;

#[derive(Debug, Clone)]
pub enum EngineLifecycle {
    Active(ActiveGame),
    Finished(FinishedGame),
}

#[derive(Debug, Clone)]
pub struct ActiveGame {
    pub game: GameState,
    pub index: GameIndex,
    pub phase: GamePhase,
    pub pending: PendingDecisions,
    pub next_decision_id: u64,
    pub trade_sessions: TradeSessions,
    pub stats: GameRunStats,
    pub invalid_actions: u64,
    pub pending_discards: PendingDiscards,
}

#[derive(Debug, Clone)]
pub struct FinishedGame {
    pub game: GameState,
    pub index: GameIndex,
    pub result: GameResult,
    pub stats: GameRunStats,
}

impl EngineLifecycle {
    pub fn active(game: GameState) -> Self {
        let index = GameIndex::rebuild(&game);
        Self::Active(ActiveGame {
            game,
            index,
            phase: GamePhase::NotStarted,
            pending: PendingDecisions::default(),
            next_decision_id: 0,
            trade_sessions: SmallVec::new(),
            stats: GameRunStats::default(),
            invalid_actions: 0,
            pending_discards: SmallVec::new(),
        })
    }

    pub fn as_active(&self) -> Option<&ActiveGame> {
        match self {
            Self::Active(active) => Some(active),
            Self::Finished(_) => None,
        }
    }

    pub fn active_mut(&mut self) -> Option<&mut ActiveGame> {
        match self {
            Self::Active(active) => Some(active),
            Self::Finished(_) => None,
        }
    }

    pub(crate) fn take_active(&mut self) -> Option<ActiveGame> {
        let placeholder = Self::Finished(FinishedGame {
            game: match self {
                Self::Active(active) => active.game.clone(),
                Self::Finished(finished) => finished.game.clone(),
            },
            index: match self {
                Self::Active(active) => active.index.clone(),
                Self::Finished(finished) => finished.index.clone(),
            },
            result: GameResult::Interrupted {
                reason: "lifecycle transition in progress".to_owned(),
            },
            stats: match self {
                Self::Active(active) => active.stats,
                Self::Finished(finished) => finished.stats,
            },
        });
        match std::mem::replace(self, placeholder) {
            Self::Active(active) => Some(active),
            Self::Finished(finished) => {
                *self = Self::Finished(finished);
                None
            }
        }
    }
}
