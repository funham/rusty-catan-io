use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
};

use catan_core::{
    agent::{
        action::{
            ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
        agent::PlayerRuntime,
    },
    gameplay::{
        field::{
            PortPos,
            state::{BoardLayout, BoardState},
        },
        game::{
            event::{
                GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
                PlayerNotification,
            },
            legal::{self, BuildClass},
            state::GameState,
            view::{
                OmniscientGameView, PlayerDecisionContext, PlayerNotificationContext,
                PrivatePlayerView, PublicBankResources, PublicGameView, PublicPlayerResources,
                PublicVpKnowledge,
            },
        },
        primitives::{
            PortKind, Tile,
            bank::DeckFullnessLevel,
            build::{Build, Establishment, Road},
            dev_card::{DevCardData, DevCardUsage, UsableDevCardCollection},
            player::PlayerId,
            resource::{ResourceCollection, ResourceMap},
            trade::BankTrade,
        },
    },
    topology::Hex,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CliRole {
    Player { player_id: PlayerId },
    Spectator,
    PlayerObserver { player_id: PlayerId },
    Omniscient,
    SnapshotObserver,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostToCli {
    Hello { role: CliRole },
    DecisionRequest(DecisionRequestFrame),
    Event { event: GameEvent, view: UiModel },
    Shutdown { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CliToHost {
    Ready,
    DecisionResponse(DecisionResponseFrame),
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
    InitAction(DecisionRequestEnvelope),
    PostDice(DecisionRequestEnvelope),
    PostDevCard(DecisionRequestEnvelope),
    Regular(DecisionRequestEnvelope),
    MoveRobbers(DecisionRequestEnvelope),
    ChoosePlayerToRob(DecisionRequestEnvelope),
    AnswerTrade(DecisionRequestEnvelope),
    DropHalf(DecisionRequestEnvelope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionResponseFrame {
    InitStage(InitStageAction),
    InitAction(InitAction),
    PostDice(PostDiceAction),
    PostDevCard(PostDevCardAction),
    Regular(RegularAction),
    MoveRobbers(MoveRobbersAction),
    ChoosePlayerToRob(ChoosePlayerToRobAction),
    AnswerTrade(TradeAnswer),
    DropHalf(DropHalfAction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequestEnvelope {
    pub request_id: u64,
    pub view: UiModel,
    pub legal: LegalDecisionOptions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegalDecisionOptions {
    pub initial_placements: Vec<InitStageAction>,
    pub builds: LegalBuildOptions,
    pub regular_actions: Vec<RegularAction>,
    pub bank_trades: Vec<BankTrade>,
    pub dev_card_usages: Vec<DevCardUsage>,
    pub robber_hexes: Vec<Hex>,
    pub robber_pos: Option<Hex>,
    pub rob_targets: Vec<PlayerId>,
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
            Self::InitAction(_) => "init_action",
            Self::PostDice(_) => "post_dice",
            Self::PostDevCard(_) => "post_dev_card",
            Self::Regular(_) => "regular",
            Self::MoveRobbers(_) => "move_robbers",
            Self::ChoosePlayerToRob(_) => "choose_player_to_rob",
            Self::AnswerTrade(_) => "answer_trade",
            Self::DropHalf(_) => "drop_half",
        }
    }

    pub fn envelope(&self) -> &DecisionRequestEnvelope {
        match self {
            Self::InitStage(envelope)
            | Self::InitAction(envelope)
            | Self::PostDice(envelope)
            | Self::PostDevCard(envelope)
            | Self::Regular(envelope)
            | Self::MoveRobbers(envelope)
            | Self::ChoosePlayerToRob(envelope)
            | Self::AnswerTrade(envelope)
            | Self::DropHalf(envelope) => envelope,
        }
    }
}

impl LegalDecisionOptions {
    pub fn from_context(context: &PlayerDecisionContext<'_>, robber_pos: Option<Hex>) -> Self {
        let initial_placements = legal::legal_initial_placements(context)
            .into_iter()
            .map(|(establishment, road)| InitStageAction {
                establishment_position: establishment.pos,
                road,
            })
            .collect();

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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiModel {
    pub actor: Option<PlayerId>,
    pub public: UiPublicGame,
    pub private: Option<UiPrivatePlayer>,
    pub omniscient: Option<UiOmniscient>,
    pub snapshot_state: Option<GameState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPublicGame {
    pub board: UiBoard,
    pub board_state: BoardState,
    pub bank: UiPublicBank,
    pub players: Vec<UiPublicPlayer>,
    pub builds: Vec<UiPlayerBuilds>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPlayerBuilds {
    pub player_id: PlayerId,
    pub establishments: Vec<Establishment>,
    pub roads: Vec<Road>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiBoard {
    pub n_players: usize,
    pub field_radius: u8,
    pub tiles: Vec<Tile>,
    pub ports: Vec<(PortPos, PortKind)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPublicBank {
    pub resources: UiPublicBankResources,
    pub dev_card_count: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiPublicBankResources {
    Exact(ResourceCollection),
    Approx(ResourceMap<DeckFullnessLevel>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPublicPlayer {
    pub player_id: PlayerId,
    pub resources: UiPublicPlayerResources,
    pub queued_dev_cards: u16,
    pub active_dev_cards: u16,
    pub played_dev_cards: UsableDevCardCollection,
    pub victory_points: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiPublicPlayerResources {
    Exact(ResourceCollection),
    Total(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPrivatePlayer {
    pub player_id: PlayerId,
    pub resources: ResourceCollection,
    pub dev_cards: DevCardData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiOmniscient {
    pub players: Vec<UiPrivatePlayer>,
    pub bank_resources: ResourceCollection,
}

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(io::Error::other)?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds maximum length",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds maximum length",
        ));
    }
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

pub struct RemoteCliAgent {
    player_id: PlayerId,
    stream: UnixStream,
    next_request_id: u64,
}

impl RemoteCliAgent {
    pub fn new(player_id: PlayerId, mut stream: UnixStream) -> io::Result<Self> {
        write_frame(
            &mut stream,
            &HostToCli::Hello {
                role: CliRole::Player { player_id },
            },
        )?;
        expect_ready(&mut stream)?;
        Ok(Self {
            player_id,
            stream,
            next_request_id: 0,
        })
    }

    fn request(&mut self, request: DecisionRequestFrame) -> DecisionResponseFrame {
        let request_id = request.request_id();
        let kind = request.kind();
        log::trace!(
            target: "catan_agents::remote_agent",
            "sending CLI decision request id={request_id} kind={kind}"
        );
        write_frame(&mut self.stream, &HostToCli::DecisionRequest(request))
            .expect("failed to write CLI decision request");
        loop {
            match read_frame::<CliToHost>(&mut self.stream).expect("failed to read CLI response") {
                CliToHost::DecisionResponse(response) => {
                    log::trace!(
                        target: "catan_agents::remote_agent",
                        "received CLI decision response id={request_id} kind={kind}"
                    );
                    return response;
                }
                CliToHost::Log {
                    level,
                    target,
                    message,
                } => log_remote_cli_message(level, &target, &message),
                CliToHost::Error { message } => panic!("remote CLI error: {message}"),
                CliToHost::Ready => panic!("unexpected ready from remote CLI"),
            }
        }
    }

    fn envelope(
        &mut self,
        context: &PlayerDecisionContext<'_>,
        robber_pos: Option<Hex>,
    ) -> DecisionRequestEnvelope {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        DecisionRequestEnvelope {
            request_id,
            view: UiModel::from_decision(context),
            legal: LegalDecisionOptions::from_context(context, robber_pos),
        }
    }
}

impl PlayerNotification for RemoteCliAgent {
    fn on_event(&mut self, event: &GameEvent, context: PlayerNotificationContext<'_>) {
        let model = UiModel::from_player_notification(&context);
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Event {
                event: event.clone(),
                view: model,
            },
        );
    }
}

impl PlayerRuntime for RemoteCliAgent {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::InitStage(envelope)) {
            DecisionResponseFrame::InitStage(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::InitAction(envelope)) {
            DecisionResponseFrame::InitAction(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::PostDice(envelope)) {
            DecisionResponseFrame::PostDice(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn after_dev_card_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::PostDevCard(envelope)) {
            DecisionResponseFrame::PostDevCard(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::Regular(envelope)) {
            DecisionResponseFrame::Regular(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::MoveRobbers(envelope)) {
            DecisionResponseFrame::MoveRobbers(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        let envelope = self.envelope(&context, Some(robber_pos));
        match self.request(DecisionRequestFrame::ChoosePlayerToRob(envelope)) {
            DecisionResponseFrame::ChoosePlayerToRob(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn answer_trade(&mut self, context: PlayerDecisionContext<'_>) -> TradeAnswer {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::AnswerTrade(envelope)) {
            DecisionResponseFrame::AnswerTrade(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::DropHalf(envelope)) {
            DecisionResponseFrame::DropHalf(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }
}

impl Drop for RemoteCliAgent {
    fn drop(&mut self) {
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Shutdown {
                reason: "host dropped remote CLI agent".to_owned(),
            },
        );
    }
}

pub struct RemoteCliObserver {
    kind: ObserverKind,
    stream: UnixStream,
    include_snapshot_state: bool,
}

impl RemoteCliObserver {
    pub fn new(kind: ObserverKind, stream: UnixStream) -> io::Result<Self> {
        let role = match kind {
            ObserverKind::Spectator => CliRole::Spectator,
            ObserverKind::Player(player_id) => CliRole::PlayerObserver { player_id },
            ObserverKind::Omniscient => CliRole::Omniscient,
        };
        Self::new_with_role(kind, role, stream)
    }

    pub fn new_snapshot(stream: UnixStream) -> io::Result<Self> {
        Self::new_with_role(ObserverKind::Omniscient, CliRole::SnapshotObserver, stream)
    }

    fn new_with_role(
        kind: ObserverKind,
        role: CliRole,
        mut stream: UnixStream,
    ) -> io::Result<Self> {
        let include_snapshot_state = matches!(role, CliRole::SnapshotObserver);
        write_frame(&mut stream, &HostToCli::Hello { role })?;
        expect_ready(&mut stream)?;
        Ok(Self {
            kind,
            stream,
            include_snapshot_state,
        })
    }
}

impl GameObserver for RemoteCliObserver {
    fn kind(&self) -> ObserverKind {
        self.kind
    }

    fn on_event(&mut self, event: &GameEvent, context: ObserverNotificationContext<'_>) {
        let model = UiModel::from_observer(context, self.include_snapshot_state);
        let summary = ui_model_summary(&model);
        let started = std::time::Instant::now();
        let frame_len = observer_event_frame_len(event, &model)
            .map(|len| len.to_string())
            .unwrap_or_else(|err| format!("serialize_error:{err}"));
        log::trace!(
            target: "catan_agents::remote_observer_flow",
            "send start event={:?} snapshot={} frame_len={} {}",
            event,
            self.include_snapshot_state,
            frame_len,
            summary
        );
        if let Err(err) = write_frame(
            &mut self.stream,
            &HostToCli::Event {
                event: event.clone(),
                view: model,
            },
        ) {
            log::warn!(target: "catan_agents::remote_observer_flow", "send failed event={:?} elapsed_ms={} err={err}", event, started.elapsed().as_millis());
        } else {
            log::trace!(
                target: "catan_agents::remote_observer_flow",
                "send done event={:?} elapsed_ms={}",
                event,
                started.elapsed().as_millis()
            );
        }
    }
}

fn observer_event_frame_len(
    event: &GameEvent,
    model: &UiModel,
) -> Result<usize, serde_json::Error> {
    serde_json::to_vec(&HostToCli::Event {
        event: event.clone(),
        view: model.clone(),
    })
    .map(|payload| payload.len())
}

impl Drop for RemoteCliObserver {
    fn drop(&mut self) {
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Shutdown {
                reason: "host dropped remote CLI observer".to_owned(),
            },
        );
    }
}

impl UiModel {
    pub fn from_decision(context: &PlayerDecisionContext<'_>) -> Self {
        Self {
            actor: Some(context.actor),
            public: UiPublicGame::from_public(&context.public),
            private: Some(UiPrivatePlayer::from_private(&context.private)),
            omniscient: None,
            snapshot_state: None,
        }
    }

    pub fn from_player_notification(context: &PlayerNotificationContext<'_>) -> Self {
        Self {
            actor: Some(context.self_id),
            public: UiPublicGame::from_public(&context.public),
            private: Some(UiPrivatePlayer::from_private(&context.private)),
            omniscient: None,
            snapshot_state: None,
        }
    }

    pub fn from_observer(
        context: ObserverNotificationContext<'_>,
        include_snapshot_state: bool,
    ) -> Self {
        match context {
            ObserverNotificationContext::Spectator { public } => Self {
                actor: None,
                public: UiPublicGame::from_public(&public),
                private: None,
                omniscient: None,
                snapshot_state: None,
            },
            ObserverNotificationContext::Player { public, private } => Self {
                actor: Some(private.player_id),
                public: UiPublicGame::from_public(&public),
                private: Some(UiPrivatePlayer::from_private(&private)),
                omniscient: None,
                snapshot_state: None,
            },
            ObserverNotificationContext::Omniscient { public, full } => Self {
                actor: None,
                public: UiPublicGame::from_public(&public),
                private: None,
                omniscient: Some(UiOmniscient::from_full(&full)),
                snapshot_state: include_snapshot_state.then(|| full.state.clone()),
            },
        }
    }
}

pub fn ui_model_summary(model: &UiModel) -> String {
    let settlements: usize = model
        .public
        .builds
        .iter()
        .map(|builds| builds.establishments.len())
        .sum();
    let roads: usize = model
        .public
        .builds
        .iter()
        .map(|builds| builds.roads.len())
        .sum();
    let resources = model
        .snapshot_state
        .as_ref()
        .map(|state| {
            state
                .players
                .iter()
                .enumerate()
                .map(|(player_id, player)| format!("p{player_id}:{}", player.resources().total()))
                .collect::<Vec<_>>()
                .join(",")
        })
        .or_else(|| {
            model.omniscient.as_ref().map(|omniscient| {
                omniscient
                    .players
                    .iter()
                    .map(|player| format!("p{}:{}", player.player_id, player.resources.total()))
                    .collect::<Vec<_>>()
                    .join(",")
            })
        })
        .unwrap_or_else(|| "-".to_owned());
    format!("builds S:{settlements} R:{roads}; resources [{resources}]")
}

impl UiPublicGame {
    fn from_public(public: &PublicGameView<'_>) -> Self {
        Self {
            board: UiBoard::from_board(public.board),
            board_state: *public.board_state,
            bank: UiPublicBank::from_public(&public.bank),
            players: public
                .players
                .iter()
                .map(|player| UiPublicPlayer {
                    player_id: player.player_id,
                    resources: match player.resources {
                        PublicPlayerResources::Exact(resources) => {
                            UiPublicPlayerResources::Exact(resources)
                        }
                        PublicPlayerResources::Total(total) => {
                            UiPublicPlayerResources::Total(total)
                        }
                    },
                    queued_dev_cards: player.dev_cards.queued,
                    active_dev_cards: player.dev_cards.active,
                    played_dev_cards: player.dev_cards.played,
                    victory_points: match player.dev_cards.victory_points {
                        PublicVpKnowledge::Hidden => None,
                        PublicVpKnowledge::Exact(points) => Some(points),
                    },
                })
                .collect(),
            builds: public
                .builds
                .players_indexed()
                .map(|(player_id, builds)| UiPlayerBuilds {
                    player_id,
                    establishments: builds.establishments.iter().copied().collect(),
                    roads: builds.roads.iter().collect(),
                })
                .collect(),
            longest_road_owner: public.longest_road_owner,
            largest_army_owner: public.largest_army_owner,
        }
    }
}

impl UiBoard {
    fn from_board(board: &BoardLayout) -> Self {
        Self {
            n_players: board.n_players,
            field_radius: board.arrangement.radius(),
            tiles: board.arrangement.iter().collect(),
            ports: board
                .arrangement
                .ports()
                .iter()
                .map(|(path, port)| (*path, *port))
                .collect(),
        }
    }
}

impl UiPublicBank {
    fn from_public(bank: &catan_core::gameplay::game::view::PublicBankView) -> Self {
        Self {
            resources: match &bank.resources {
                PublicBankResources::Exact(resources) => UiPublicBankResources::Exact(*resources),
                PublicBankResources::Approx(resources) => UiPublicBankResources::Approx(*resources),
            },
            dev_card_count: bank.dev_card_count,
        }
    }
}

impl UiPrivatePlayer {
    fn from_private(private: &PrivatePlayerView<'_>) -> Self {
        Self {
            player_id: private.player_id,
            resources: *private.resources,
            dev_cards: private.dev_cards.clone(),
        }
    }
}

impl UiOmniscient {
    fn from_full(full: &OmniscientGameView<'_>) -> Self {
        Self {
            players: full
                .state
                .players
                .iter()
                .enumerate()
                .map(|(player_id, player)| UiPrivatePlayer {
                    player_id,
                    resources: *player.resources(),
                    dev_cards: player.dev_cards().clone(),
                })
                .collect(),
            bank_resources: full.state.bank.resources,
        }
    }
}

fn expect_ready(stream: &mut UnixStream) -> io::Result<()> {
    loop {
        match read_frame::<CliToHost>(stream)? {
            CliToHost::Ready => return Ok(()),
            CliToHost::Log {
                level,
                target,
                message,
            } => log_remote_cli_message(level, &target, &message),
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected ready, got {other:?}"),
                ));
            }
        }
    }
}

fn log_remote_cli_message(level: RemoteLogLevel, child_target: &str, message: &str) {
    let level = log::Level::from(level);
    for line in message.lines().filter(|line| !line.trim().is_empty()) {
        log::log!(target: "catan_cli_child", level, "[{child_target}] {line}");
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
    use super::{
        CliRole, HostToCli, LegalDecisionOptions, RemoteCliObserver, UiBoard, UiModel, read_frame,
        write_frame,
    };
    use catan_core::gameplay::{
        game::{
            event::{GameEvent, GameObserver, ObserverNotificationContext},
            index::GameIndex,
            init::GameInitializationState,
            view::{ContextFactory, SearchFactory, VisibilityConfig},
        },
        primitives::{
            dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
            resource::{Resource, ResourceCollection},
        },
    };

    #[test]
    fn frame_round_trip() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"hello").unwrap();
        let value: String = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(value, "hello");
    }

    #[test]
    fn log_frame_round_trip() {
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &super::CliToHost::Log {
                level: super::RemoteLogLevel::Trace,
                target: "child_target".to_owned(),
                message: "child log".to_owned(),
            },
        )
        .unwrap();
        let value: super::CliToHost = read_frame(&mut bytes.as_slice()).unwrap();
        assert!(matches!(
            value,
            super::CliToHost::Log {
                level: super::RemoteLogLevel::Trace,
                target,
                message,
            } if target == "child_target" && message == "child log"
        ));
    }

    #[test]
    fn rejects_large_frame() {
        let bytes = ((16 * 1024 * 1024 + 1) as u32).to_be_bytes().to_vec();
        let err = read_frame::<String>(&mut bytes.as_slice()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn ui_board_serializes_to_json() {
        let state = GameInitializationState::default().finish();
        let board = UiBoard::from_board(&state.board);
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = catan_core::gameplay::game::view::VisibilityConfig::default();
        let factory = catan_core::gameplay::game::view::ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let model = UiModel::from_decision(&factory.player_decision_context(0, None));
        let msg = HostToCli::Hello {
            role: CliRole::Spectator,
        };

        serde_json::to_vec(&board).unwrap();
        serde_json::to_vec(&model).unwrap();
        serde_json::to_vec(&msg).unwrap();
    }

    #[test]
    fn legal_options_attach_regular_trades_and_dev_card_usages() {
        let mut state = GameInitializationState::default().finish();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 4,
                    wheat: 1,
                    sheep: 1,
                    ore: 1,
                    ..ResourceCollection::ZERO
                },
                0,
            )
            .expect("bank should fund test player");
        state
            .players
            .get_mut(0)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::YearOfPlenty));
        state.players.get_mut(0).dev_cards_reset_queue();

        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);
        let legal = LegalDecisionOptions::from_context(&context, None);

        assert!(!legal.initial_placements.is_empty());
        assert!(!legal.regular_actions.is_empty());
        assert!(
            legal
                .bank_trades
                .iter()
                .any(|trade| trade.give == Resource::Brick)
        );
        assert!(
            legal
                .dev_card_usages
                .iter()
                .any(|usage| { matches!(usage, DevCardUsage::YearOfPlenty([_, _])) })
        );
    }

    #[test]
    fn legal_options_attach_explicit_robber_context() {
        let mut init = GameInitializationState::default();
        let mut victim_hex = None;
        for player_id in 0..2 {
            let (settlement, road) = init
                .builds
                .query()
                .possible_initial_placements(&init.board, player_id)
                .into_iter()
                .next()
                .expect("default board should have initial placements");
            if player_id == 1 {
                let board_hexes = init.board.arrangement.hex_iter().collect::<Vec<_>>();
                victim_hex =
                    settlement.pos.as_set().into_iter().find(|hex| {
                        *hex != init.board_state.robber_pos && board_hexes.contains(hex)
                    });
            }
            init.builds
                .try_init_place(player_id, road, settlement)
                .expect("generated initial placement should be valid");
        }
        let mut state = init.finish();
        state
            .transfer_from_bank(Resource::Brick.into(), 1)
            .expect("bank should fund victim");
        let victim_hex = victim_hex.expect("victim should touch a non-robber hex");

        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);
        let legal = LegalDecisionOptions::from_context(&context, Some(victim_hex));

        assert_eq!(legal.robber_pos, Some(victim_hex));
        assert!(legal.rob_targets.contains(&1));
        assert!(!legal.robber_hexes.contains(&state.board_state.robber_pos));
    }

    #[test]
    fn snapshot_observer_model_includes_exact_state_only_when_requested() {
        let state = GameInitializationState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };

        let normal = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            false,
        );
        let snapshot = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );

        assert!(normal.omniscient.is_some());
        assert!(normal.snapshot_state.is_none());
        assert!(snapshot.snapshot_state.is_some());
        assert_eq!(
            snapshot.snapshot_state.unwrap().bank.dev_cards,
            state.bank.dev_cards
        );
    }

    #[test]
    fn snapshot_observer_event_frame_writes_with_exact_state() {
        let state = GameInitializationState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let model = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );
        let mut bytes = Vec::new();

        write_frame(
            &mut bytes,
            &HostToCli::Event {
                event: catan_core::gameplay::game::event::GameEvent::GameStarted,
                view: model,
            },
        )
        .unwrap();

        assert!(!bytes.is_empty());
    }

    #[test]
    fn snapshot_observer_event_frame_writes_with_built_exact_state() {
        let mut init = GameInitializationState::default();
        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .next()
            .expect("default board should have an initial placement");
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");

        let state = init.finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let model = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );
        let mut bytes = Vec::new();

        write_frame(
            &mut bytes,
            &HostToCli::Event {
                event: GameEvent::InitialPlacementBuilt {
                    player_id: 0,
                    settlement: settlement.pos,
                    road,
                },
                view: model,
            },
        )
        .unwrap();

        assert!(!bytes.is_empty());
    }

    #[test]
    fn remote_observer_streams_multiple_event_frames_without_responses() {
        let (host, mut child) = std::os::unix::net::UnixStream::pair().unwrap();
        child
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let writer = std::thread::spawn(move || {
            let mut observer = RemoteCliObserver {
                kind: catan_core::gameplay::game::event::ObserverKind::Omniscient,
                stream: host,
                include_snapshot_state: true,
            };

            let mut state = GameInitializationState::default().finish();
            let first_index = GameIndex::rebuild(&state);
            let visibility = VisibilityConfig::default();
            let first_factory = ContextFactory {
                state: &state,
                index: &first_index,
                visibility: &visibility,
            };
            observer.on_event(
                &GameEvent::GameStarted,
                ObserverNotificationContext::Omniscient {
                    public: first_factory.spectator_public_view(),
                    full: first_factory.omniscient_view(),
                },
            );

            state
                .transfer_from_bank(Resource::Brick.into(), 0)
                .expect("bank should fund player");
            let second_index = GameIndex::rebuild(&state);
            let second_factory = ContextFactory {
                state: &state,
                index: &second_index,
                visibility: &visibility,
            };
            observer.on_event(
                &GameEvent::ResourcesDistributed,
                ObserverNotificationContext::Omniscient {
                    public: second_factory.spectator_public_view(),
                    full: second_factory.omniscient_view(),
                },
            );
        });

        let first = read_frame::<HostToCli>(&mut child).unwrap();
        let second = read_frame::<HostToCli>(&mut child).unwrap();
        assert!(matches!(
            first,
            HostToCli::Event {
                event: GameEvent::GameStarted,
                ..
            }
        ));
        assert!(matches!(
            second,
            HostToCli::Event {
                event: GameEvent::ResourcesDistributed,
                ..
            }
        ));
        if let HostToCli::Event { view, .. } = second {
            assert_eq!(
                view.snapshot_state
                    .expect("snapshot role should include exact state")
                    .players
                    .get(0)
                    .resources()
                    .total(),
                1
            );
        }
        writer.join().unwrap();
    }
}
