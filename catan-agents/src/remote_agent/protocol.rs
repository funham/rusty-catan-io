use std::{
    io::{self, Read, Write},
    marker::PhantomData,
};

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
            legal::{self, BuildClass},
            output::GameOutput,
            view::PlayerDecisionContext,
        },
        primitives::{build::Build, dev_card::DevCardUsage, player::PlayerId, trade::BankTrade},
    },
    topology::Hex,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::model::UiModel;

pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CliRole {
    Player { player_id: PlayerId },
    Spectator,
    PlayerObserver { player_id: PlayerId },
    Omniscient,
    SnapshotObserver,
}

impl CliRole {
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
pub enum HostToCli {
    Hello {
        role: CliRole,
    },
    DecisionRequest(DecisionRequestFrame),
    Output {
        output: GameOutput,
        view: Box<UiModel>,
        legal: LegalDecisionOptions,
    },
    Event {
        event: GameEvent,
        view: UiModel,
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
pub enum CliToHost {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequestEnvelope {
    pub request_id: u64,
    pub view: UiModel,
    pub legal: LegalDecisionOptions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegalDecisionOptions {
    pub initial_placements: Vec<InitialPlacementCommand>,
    pub builds: LegalBuildOptions,
    pub regular_actions: Vec<RegularCommand>,
    pub bank_trades: Vec<BankTrade>,
    pub dev_card_usages: Vec<DevCardUsage>,
    pub robber_hexes: Vec<Hex>,
    pub robber_pos: Option<Hex>,
    pub rob_targets: Vec<PlayerId>,
    #[serde(default)]
    pub dev_card_used_this_turn: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegalBuildOptions {
    pub roads: Vec<Build>,
    pub settlements: Vec<Build>,
    pub cities: Vec<Build>,
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

impl LegalDecisionOptions {
    pub fn from_context(context: &PlayerDecisionContext<'_>, robber_pos: Option<Hex>) -> Self {
        let initial_placements = legal::legal_initial_placements(context);

        let robber_hexes = context
            .public
            .board
            .arrangement
            .hex_iter()
            .filter(|hex| *hex != context.public.board_state.robber_pos)
            .collect();

        let rob_targets = robber_pos
            .map(|pos| legal::legal_rob_targets(context, pos))
            .unwrap_or_default();

        Self {
            initial_placements,
            builds: LegalBuildOptions {
                roads: legal::legal_builds(context, BuildClass::Road),
                settlements: legal::legal_builds(context, BuildClass::Settlement),
                cities: legal::legal_builds(context, BuildClass::City),
            },
            regular_actions: legal::legal_regular_actions(context),
            bank_trades: legal::legal_bank_trades(context),
            dev_card_usages: legal::legal_dev_card_usages(context),
            robber_hexes,
            robber_pos,
            rob_targets,
            dev_card_used_this_turn: false,
        }
    }
}

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(frame_too_large());
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_FRAME_LEN {
        return Err(frame_too_large());
    }
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

pub struct NonblockingFrameReader<T> {
    buffer: Vec<u8>,
    _marker: PhantomData<T>,
}

impl<T> Default for NonblockingFrameReader<T> {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            _marker: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> NonblockingFrameReader<T> {
    pub fn poll(&mut self, reader: &mut impl Read) -> io::Result<Option<T>> {
        let mut chunk = [0; 8192];
        let mut eof = false;
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }

        if self.buffer.len() < 4 {
            if eof {
                return Err(unexpected_eof());
            }
            return Ok(None);
        }
        let len = u32::from_be_bytes(
            self.buffer[..4]
                .try_into()
                .expect("slice length checked above"),
        ) as usize;
        if len > MAX_FRAME_LEN {
            return Err(frame_too_large());
        }
        let frame_len = 4 + len;
        if self.buffer.len() < frame_len {
            if eof {
                return Err(unexpected_eof());
            }
            return Ok(None);
        }

        let payload = self.buffer[4..frame_len].to_vec();
        self.buffer.drain(..frame_len);
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(io::Error::other)
    }
}

fn frame_too_large() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "frame exceeds maximum length")
}

fn unexpected_eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "remote closed frame stream")
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
