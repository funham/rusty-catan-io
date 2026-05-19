use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::gameplay::{
    game::{
        decision::{DecisionAllocator, PendingDecisions},
        index::GameIndex,
        init::GameInitializationState,
        run::{GameResult, GameRunStats},
        state::{GameState, TableState},
        trade::TradeSession,
    },
    primitives::{
        player::PlayerId,
        turn::{BackAndForthCycle, GameTurn, RegularCycle},
    },
};

#[cfg(test)]
use crate::gameplay::game::phase::GamePhase;

pub type TradeSessions = SmallVec<[TradeSession; 16]>;
pub type PendingDiscards = SmallVec<[PlayerId; 8]>;

#[cfg(test)]
pub type EngineCore = EngineState;

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
    #[cfg(test)]
    pub phase: GamePhase,
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
            #[cfg(test)]
            phase: GamePhase::Turn(crate::gameplay::game::phase::TurnPhase::InitCommand),
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
    pub fn unstarted(init: GameInitializationState) -> Self {
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

    pub fn from_game_for_tests(game: GameState) -> Self {
        let index = GameIndex::rebuild(&game);
        Self::Playing(PlayingEngine {
            game,
            #[cfg(test)]
            phase: GamePhase::NotStarted,
            index,
            pending: PendingDecisions::default(),
            next_decision_id: 0,
            trade_sessions: SmallVec::new(),
            stats: GameRunStats::default(),
            invalid_actions: 0,
            pending_discards: SmallVec::new(),
        })
    }

    pub fn as_setup(&self) -> Option<&SetupEngine> {
        match self {
            Self::Setup(setup) => Some(setup),
            _ => None,
        }
    }

    pub fn setup_mut(&mut self) -> Option<&mut SetupEngine> {
        match self {
            Self::Setup(setup) => Some(setup),
            _ => None,
        }
    }

    pub fn as_playing(&self) -> Option<&PlayingEngine> {
        match self {
            Self::Playing(playing) => Some(playing),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn active(game: GameState) -> Self {
        Self::from_game_for_tests(game)
    }

    #[cfg(test)]
    pub fn as_active(&self) -> Option<&PlayingEngine> {
        self.as_playing()
    }

    #[cfg(test)]
    pub fn active_mut(&mut self) -> Option<&mut PlayingEngine> {
        self.playing_mut()
    }

    #[cfg(test)]
    pub fn force_playing_for_tests(&mut self) {
        match std::mem::replace(self, Self::interrupted_placeholder()) {
            Self::Unstarted(unstarted) => {
                *self = Self::Playing(unstarted.into_setup().into_playing());
            }
            Self::Setup(setup) => {
                *self = Self::Playing(setup.into_playing());
            }
            other => {
                *self = other;
            }
        }
    }

    pub fn playing_mut(&mut self) -> Option<&mut PlayingEngine> {
        match self {
            Self::Playing(playing) => Some(playing),
            _ => None,
        }
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

    pub(crate) fn take_setup(&mut self) -> Option<SetupEngine> {
        match std::mem::replace(self, Self::interrupted_placeholder()) {
            Self::Setup(setup) => Some(setup),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(crate) fn take_playing(&mut self) -> Option<PlayingEngine> {
        match std::mem::replace(self, Self::interrupted_placeholder()) {
            Self::Playing(playing) => Some(playing),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(crate) fn interrupted_placeholder() -> Self {
        let init = GameInitializationState::default().finish();
        let index = GameIndex::rebuild(&init);
        Self::Finished(FinishedEngine {
            game: init,
            index,
            result: GameResult::Interrupted {
                reason: "engine state transition in progress".to_owned(),
            },
            stats: GameRunStats::default(),
        })
    }
}
