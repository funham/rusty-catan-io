use catan_core::gameplay::{
    field::{
        PortPos,
        state::{BoardLayout, BoardState},
    },
    game::{
        event::ObserverNotificationContext,
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
        build::{Establishment, Road},
        dev_card::{DevCardData, UsableDevCardCollection},
        player::PlayerId,
        resource::{ResourceCollection, ResourceMap},
    },
};
use serde::{Deserialize, Serialize};

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
    pub fn from_board(board: &BoardLayout) -> Self {
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
