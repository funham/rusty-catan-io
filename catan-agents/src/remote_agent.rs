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
        field::state::{BoardLayout, BoardState},
        game::{
            event::{
                GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
                PlayerNotification,
            },
            view::{
                OmniscientGameView, PlayerDecisionContext, PlayerNotificationContext,
                PrivatePlayerView, PublicBankResources, PublicGameView, PublicPlayerResources,
                PublicVpKnowledge,
            },
        },
        primitives::{
            PortKind, Tile,
            bank::DeckFullnessLevel,
            build::{Establishment, Road},
            dev_card::{DevCardData, UsableDevCardCollection},
            player::PlayerId,
            resource::{ResourceCollection, ResourceMap},
        },
    },
    topology::Path,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CliRole {
    Player { player_id: PlayerId },
    Spectator,
    PlayerObserver { player_id: PlayerId },
    Omniscient,
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
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionRequestFrame {
    InitStage(UiModel),
    InitAction(UiModel),
    PostDice(UiModel),
    PostDevCard(UiModel),
    Regular(UiModel),
    MoveRobbers(UiModel),
    ChoosePlayerToRob(UiModel),
    AnswerTrade(UiModel),
    DropHalf(UiModel),
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
pub struct UiModel {
    pub actor: Option<PlayerId>,
    pub public: UiPublicGame,
    pub private: Option<UiPrivatePlayer>,
    pub omniscient: Option<UiOmniscient>,
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
    pub ports: Vec<(Path, PortKind)>,
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
        Ok(Self { player_id, stream })
    }

    fn request(&mut self, request: DecisionRequestFrame) -> DecisionResponseFrame {
        write_frame(&mut self.stream, &HostToCli::DecisionRequest(request))
            .expect("failed to write CLI decision request");
        match read_frame::<CliToHost>(&mut self.stream).expect("failed to read CLI response") {
            CliToHost::DecisionResponse(response) => response,
            CliToHost::Error { message } => panic!("remote CLI error: {message}"),
            CliToHost::Ready => panic!("unexpected ready from remote CLI"),
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
        match self.request(DecisionRequestFrame::InitStage(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::InitStage(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        match self.request(DecisionRequestFrame::InitAction(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::InitAction(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        match self.request(DecisionRequestFrame::PostDice(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::PostDice(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn after_dev_card_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        match self.request(DecisionRequestFrame::PostDevCard(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::PostDevCard(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        match self.request(DecisionRequestFrame::Regular(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::Regular(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        match self.request(DecisionRequestFrame::MoveRobbers(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::MoveRobbers(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
    ) -> ChoosePlayerToRobAction {
        match self.request(DecisionRequestFrame::ChoosePlayerToRob(
            UiModel::from_decision(&context),
        )) {
            DecisionResponseFrame::ChoosePlayerToRob(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn answer_trade(&mut self, context: PlayerDecisionContext<'_>) -> TradeAnswer {
        match self.request(DecisionRequestFrame::AnswerTrade(UiModel::from_decision(
            &context,
        ))) {
            DecisionResponseFrame::AnswerTrade(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        match self.request(DecisionRequestFrame::DropHalf(UiModel::from_decision(
            &context,
        ))) {
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
}

impl RemoteCliObserver {
    pub fn new(kind: ObserverKind, mut stream: UnixStream) -> io::Result<Self> {
        let role = match kind {
            ObserverKind::Spectator => CliRole::Spectator,
            ObserverKind::Player(player_id) => CliRole::PlayerObserver { player_id },
            ObserverKind::Omniscient => CliRole::Omniscient,
        };
        write_frame(&mut stream, &HostToCli::Hello { role })?;
        expect_ready(&mut stream)?;
        Ok(Self { kind, stream })
    }
}

impl GameObserver for RemoteCliObserver {
    fn kind(&self) -> ObserverKind {
        self.kind
    }

    fn on_event(&mut self, event: &GameEvent, context: ObserverNotificationContext<'_>) {
        let model = UiModel::from_observer(context);
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Event {
                event: event.clone(),
                view: model,
            },
        );
    }
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
        }
    }

    pub fn from_player_notification(context: &PlayerNotificationContext<'_>) -> Self {
        Self {
            actor: Some(context.self_id),
            public: UiPublicGame::from_public(&context.public),
            private: Some(UiPrivatePlayer::from_private(&context.private)),
            omniscient: None,
        }
    }

    pub fn from_observer(context: ObserverNotificationContext<'_>) -> Self {
        match context {
            ObserverNotificationContext::Spectator { public } => Self {
                actor: None,
                public: UiPublicGame::from_public(&public),
                private: None,
                omniscient: None,
            },
            ObserverNotificationContext::Player { public, private } => Self {
                actor: Some(private.player_id),
                public: UiPublicGame::from_public(&public),
                private: Some(UiPrivatePlayer::from_private(&private)),
                omniscient: None,
            },
            ObserverNotificationContext::Omniscient { public, full } => Self {
                actor: None,
                public: UiPublicGame::from_public(&public),
                private: None,
                omniscient: Some(UiOmniscient::from_full(&full)),
            },
        }
    }
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
            field_radius: board.arrangement.field_radius,
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
    match read_frame::<CliToHost>(stream)? {
        CliToHost::Ready => Ok(()),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected ready, got {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{CliRole, HostToCli, UiBoard, UiModel, read_frame, write_frame};
    use catan_core::gameplay::game::init::GameInitializationState;

    #[test]
    fn frame_round_trip() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"hello").unwrap();
        let value: String = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(value, "hello");
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
}
