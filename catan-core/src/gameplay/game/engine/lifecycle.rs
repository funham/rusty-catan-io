use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::gameplay::{
    game::{
        decision::{DecisionAllocator, PendingDecisions},
        index::GameIndex,
        run::{GameResult, GameRunStats},
        state::{GameState, SetupGameState, TableState},
        trade::TradeSession,
    },
    primitives::{
        player::PlayerId,
        turn::{BackAndForthCycle, GameTurn, RegularCycle},
    },
};

pub type TradeSessions = SmallVec<[TradeSession; 16]>;
pub type PendingDiscards = SmallVec<[PlayerId; 8]>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineState {
    Unstarted(UnstartedEngine),
    Setup(SetupEngine),
    Playing(PlayingEngine),
    Finished(FinishedEngine),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnstartedEngine {
    pub table: TableState,
    pub setup_turn: GameTurn<BackAndForthCycle>,
    #[serde(skip)]
    pub index: GameIndex,
    pub next_decision_id: u64,
    pub stats: GameRunStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupEngine {
    pub table: TableState,
    pub setup_turn: GameTurn<BackAndForthCycle>,
    #[serde(skip)]
    pub index: GameIndex,
    pub pending: PendingDecisions,
    pub next_decision_id: u64,
    pub stats: GameRunStats,
    pub invalid_actions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayingEngine {
    pub game: GameState,
    #[serde(skip)]
    pub index: GameIndex,
    pub pending: PendingDecisions,
    pub next_decision_id: u64,
    pub trade_sessions: TradeSessions,
    pub stats: GameRunStats,
    pub invalid_actions: u64,
    pub pending_discards: PendingDiscards,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishedEngine {
    pub game: GameState,
    #[serde(skip)]
    pub index: GameIndex,
    pub result: GameResult,
    pub stats: GameRunStats,
}

impl UnstartedEngine {
    pub(crate) fn decisions(&self) -> DecisionAllocator {
        DecisionAllocator::new(self.next_decision_id)
    }

    pub fn into_setup(self) -> SetupEngine {
        SetupEngine {
            table: self.table,
            setup_turn: self.setup_turn,
            index: self.index,
            pending: PendingDecisions::default(),
            next_decision_id: self.next_decision_id,
            stats: self.stats,
            invalid_actions: 0,
        }
    }
}

impl SetupEngine {
    pub(crate) fn decisions(&self) -> DecisionAllocator {
        DecisionAllocator::new(self.next_decision_id)
    }

    pub fn into_playing(self) -> PlayingEngine {
        let game = GameState {
            table: self.table,
            turn: self.setup_turn.into_regular(),
        };
        let index = GameIndex::rebuild(&game);
        PlayingEngine {
            game,
            index,
            pending: self.pending,
            next_decision_id: self.next_decision_id,
            trade_sessions: SmallVec::new(),
            stats: self.stats,
            invalid_actions: self.invalid_actions,
            pending_discards: SmallVec::new(),
        }
    }
}

impl PlayingEngine {
    pub(crate) fn decisions(&self) -> DecisionAllocator {
        DecisionAllocator::new(self.next_decision_id)
    }

    pub fn turn(&self) -> &GameTurn<RegularCycle> {
        &self.game.turn
    }
}

impl EngineState {
    pub fn unstarted(init: SetupGameState) -> Self {
        let (table, setup_turn) = init.into_setup_parts();
        let index = GameIndex::rebuild_table(&table);
        Self::Unstarted(UnstartedEngine {
            table,
            setup_turn,
            index,
            next_decision_id: 0,
            stats: GameRunStats::default(),
        })
    }

    pub fn playing(game: GameState) -> Self {
        let index = GameIndex::rebuild(&game);
        Self::Playing(PlayingEngine {
            game,
            index,
            pending: PendingDecisions::default(),
            next_decision_id: 0,
            trade_sessions: SmallVec::new(),
            stats: GameRunStats::default(),
            invalid_actions: 0,
            pending_discards: SmallVec::new(),
        })
    }

    pub fn table(&self) -> &TableState {
        match self {
            Self::Unstarted(unstarted) => &unstarted.table,
            Self::Setup(setup) => &setup.table,
            Self::Playing(playing) => &playing.game.table,
            Self::Finished(finished) => &finished.game.table,
        }
    }

    pub fn game(&self) -> Option<&GameState> {
        match self {
            Self::Playing(playing) => Some(&playing.game),
            Self::Finished(finished) => Some(&finished.game),
            Self::Unstarted(_) | Self::Setup(_) => None,
        }
    }

    pub fn index(&self) -> &GameIndex {
        match self {
            Self::Unstarted(unstarted) => &unstarted.index,
            Self::Setup(setup) => &setup.index,
            Self::Playing(playing) => &playing.index,
            Self::Finished(finished) => &finished.index,
        }
    }

    pub fn stats(&self) -> GameRunStats {
        match self {
            Self::Unstarted(unstarted) => unstarted.stats,
            Self::Setup(setup) => setup.stats,
            Self::Playing(playing) => playing.stats,
            Self::Finished(finished) => finished.stats,
        }
    }

    pub fn result(&self) -> Option<&GameResult> {
        match self {
            Self::Finished(finished) => Some(&finished.result),
            _ => None,
        }
    }

    pub fn rebuild_indexes(&mut self) {
        match self {
            Self::Unstarted(unstarted) => {
                unstarted.index = GameIndex::rebuild_table(&unstarted.table);
            }
            Self::Setup(setup) => {
                setup.index = GameIndex::rebuild_table(&setup.table);
            }
            Self::Playing(playing) => {
                playing.index = GameIndex::rebuild(&playing.game);
            }
            Self::Finished(finished) => {
                finished.index = GameIndex::rebuild(&finished.game);
            }
        }
    }
}
