use catan_core::{
    gameplay::game::command::{
        ChooseRobbedPlayerCommand, DiscardHalfCommand, InitCommand, InitialPlacementCommand,
        MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand, TradeAnswer,
    },
    gameplay::{
        game::{
            decision::DecisionId,
            event::{GameEvent, ObserverKind},
            input::{PlayerCommand, TradeCommand, TradeResponseCommand},
            output::GameOutput,
        },
        primitives::player::PlayerId,
    },
};
use catan_runtime::run_stats::GameSummary;
use serde::{Deserialize, Serialize};

pub use catan_core::gameplay::game::projection::{
    DecisionRequestEnvelope, GameProjection, LegalBuildOptions, LegalDecisionOptions,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteRole {
    Player { player_id: PlayerId },
    Spectator,
    PlayerObserver { player_id: PlayerId },
    Omniscient,
    SnapshotObserver,
}

impl RemoteRole {
    pub fn is_observer(&self) -> bool {
        self.observer_kind().is_some()
    }

    pub fn observer_kind(&self) -> Option<ObserverKind> {
        match self {
            Self::Spectator => Some(ObserverKind::Spectator),
            Self::PlayerObserver { player_id } => Some(ObserverKind::Player(*player_id)),
            Self::Omniscient | Self::SnapshotObserver => Some(ObserverKind::Omniscient),
            Self::Player { .. } => None,
        }
    }

    pub fn includes_exact_snapshot_state(&self) -> bool {
        matches!(self, Self::SnapshotObserver)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Player { .. } => "player",
            Self::Spectator => "spectator",
            Self::PlayerObserver { .. } => "player-observer",
            Self::Omniscient => "omniscient",
            Self::SnapshotObserver => "snapshot-observer",
        }
    }

    pub fn socket_abbrev(&self) -> &'static str {
        match self {
            Self::Player { .. } => "p",
            Self::Spectator => "spec",
            Self::PlayerObserver { .. } => "pobs",
            Self::Omniscient => "omni",
            Self::SnapshotObserver => "snap",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostMessage {
    Hello {
        role: RemoteRole,
    },
    DecisionRequest(DecisionRequestFrame),
    Output {
        output: GameOutput,
        view: Box<GameProjection>,
        legal: LegalDecisionOptions,
    },
    Event {
        event: GameEvent,
        view: GameProjection,
    },
    GameSummary {
        summary: GameSummary,
    },
    SnapshotSaved {
        path: String,
    },
    SnapshotFailed {
        reason: String,
    },
    Shutdown {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Ready,
    DecisionResponse(DecisionResponseFrame),
    SubmitCommand {
        player_id: PlayerId,
        decision_id: DecisionId,
        command: PlayerCommand,
    },
    SaveSnapshot,
    Error {
        message: String,
    },
    Log {
        level: RemoteLogLevel,
        target: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionRequestFrame {
    InitStage(DecisionRequestEnvelope),
    InitCommand(DecisionRequestEnvelope),
    PostDice(DecisionRequestEnvelope),
    PostDevCard(DecisionRequestEnvelope),
    Regular(DecisionRequestEnvelope),
    MoveRobber(DecisionRequestEnvelope),
    ChoosePlayerToRob(DecisionRequestEnvelope),
    AnswerTrade(DecisionRequestEnvelope),
    TradeResponse(DecisionRequestEnvelope),
    TradeOwnerAction(DecisionRequestEnvelope),
    DiscardHalf(DecisionRequestEnvelope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionResponseFrame {
    InitStage(InitialPlacementCommand),
    InitCommand(InitCommand),
    PostDice(PostDiceCommand),
    PostDevCard(PostDevCardCommand),
    Regular(RegularCommand),
    MoveRobber(MoveRobberCommand),
    ChoosePlayerToRob(ChooseRobbedPlayerCommand),
    AnswerTrade(TradeAnswer),
    TradeResponse(TradeResponseCommand),
    TradeOwnerAction(TradeCommand),
    DiscardHalf(DiscardHalfCommand),
}

impl DecisionRequestFrame {
    pub fn request_id(&self) -> u64 {
        self.envelope().request_id
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::InitStage(_) => "init_stage",
            Self::InitCommand(_) => "init_action",
            Self::PostDice(_) => "post_dice",
            Self::PostDevCard(_) => "post_dev_card",
            Self::Regular(_) => "regular",
            Self::MoveRobber(_) => "move_robber",
            Self::ChoosePlayerToRob(_) => "choose_player_to_rob",
            Self::AnswerTrade(_) => "answer_trade",
            Self::TradeResponse(_) => "trade_response",
            Self::TradeOwnerAction(_) => "trade_owner_action",
            Self::DiscardHalf(_) => "discard_half",
        }
    }

    pub fn envelope(&self) -> &DecisionRequestEnvelope {
        match self {
            Self::InitStage(envelope)
            | Self::InitCommand(envelope)
            | Self::PostDice(envelope)
            | Self::PostDevCard(envelope)
            | Self::Regular(envelope)
            | Self::MoveRobber(envelope)
            | Self::ChoosePlayerToRob(envelope)
            | Self::AnswerTrade(envelope)
            | Self::TradeResponse(envelope)
            | Self::TradeOwnerAction(envelope)
            | Self::DiscardHalf(envelope) => envelope,
        }
    }
}

impl From<RemoteLogLevel> for log::Level {
    fn from(value: RemoteLogLevel) -> Self {
        match value {
            RemoteLogLevel::Error => Self::Error,
            RemoteLogLevel::Warn => Self::Warn,
            RemoteLogLevel::Info => Self::Info,
            RemoteLogLevel::Debug => Self::Debug,
            RemoteLogLevel::Trace => Self::Trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_summary_message_round_trips() {
        let message = HostMessage::GameSummary {
            summary: GameSummary::default(),
        };
        let encoded = serde_json::to_string(&message).unwrap();
        let decoded: HostMessage = serde_json::from_str(&encoded).unwrap();
        assert!(matches!(decoded, HostMessage::GameSummary { .. }));
    }
}
