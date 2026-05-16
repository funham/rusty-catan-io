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
pub enum EngineCore {
    Active(ActiveEngine),
    Finished(FinishedEngine),
}

#[derive(Debug, Clone)]
pub struct ActiveEngine {
    pub game: GameState,
    pub init: Option<crate::gameplay::game::init::GameInitializationState>,
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
pub struct FinishedEngine {
    pub game: GameState,
    pub index: GameIndex,
    pub result: GameResult,
    pub stats: GameRunStats,
}

impl EngineCore {
    pub fn active(game: GameState) -> Self {
        let index = GameIndex::rebuild(&game);
        Self::Active(ActiveEngine {
            game,
            init: None,
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

    pub fn from_snapshot_parts(
        game: GameState,
        phase: GamePhase,
        pending: PendingDecisions,
        next_decision_id: u64,
        trade_sessions: TradeSessions,
        stats: GameRunStats,
        invalid_actions: u64,
        pending_discards: PendingDiscards,
        result: Option<GameResult>,
    ) -> Self {
        let index = GameIndex::rebuild(&game);
        match result {
            Some(result) => Self::Finished(FinishedEngine {
                game,
                index,
                result,
                stats,
            }),
            None => Self::Active(ActiveEngine {
                game,
                init: None,
                index,
                phase,
                pending,
                next_decision_id,
                trade_sessions,
                stats,
                invalid_actions,
                pending_discards,
            }),
        }
    }

    pub fn as_active(&self) -> Option<&ActiveEngine> {
        match self {
            Self::Active(active) => Some(active),
            Self::Finished(_) => None,
        }
    }

    pub fn active_mut(&mut self) -> Option<&mut ActiveEngine> {
        match self {
            Self::Active(active) => Some(active),
            Self::Finished(_) => None,
        }
    }

    pub fn state(&self) -> &GameState {
        match self {
            Self::Active(active) => &active.game,
            Self::Finished(finished) => &finished.game,
        }
    }

    pub fn index(&self) -> &GameIndex {
        match self {
            Self::Active(active) => &active.index,
            Self::Finished(finished) => &finished.index,
        }
    }

    pub fn stats(&self) -> GameRunStats {
        match self {
            Self::Active(active) => active.stats,
            Self::Finished(finished) => finished.stats,
        }
    }

    pub fn result(&self) -> Option<&GameResult> {
        match self {
            Self::Active(_) => None,
            Self::Finished(finished) => Some(&finished.result),
        }
    }

    pub(crate) fn take_active(&mut self) -> Option<ActiveEngine> {
        let placeholder = Self::Finished(FinishedEngine {
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
