use crate::gameplay::{
    field::{
        PortPos,
        state::{BoardLayout, BoardState},
    },
    game::{
        command::{InitialPlacementCommand, RegularCommand},
        event::ObserverNotificationContext,
        legal::{self, BuildClass},
        state::TableState,
        trade::{TradeOfferId, TradeResponseState, TradeSessionId},
        view::{
            OmniscientGameView, PlayerDecisionContext, PlayerNotificationContext,
            PrivatePlayerView, PublicBankDevCards, PublicBankResources, PublicGameView,
            PublicPlayerResources, PublicVpKnowledge,
        },
    },
    primitives::{
        PortKind, Tile,
        bank::DeckFullnessLevel,
        build::{Build, Establishment, Road},
        dev_card::{DevCardData, DevCardUsage, UsableDevCardSet},
        player::PlayerId,
        resource::{ResourceMap, ResourceSet},
        trade::{BankTrade, PlayerTrade},
    },
};
use crate::topology::Hex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameProjection {
    pub actor: Option<PlayerId>,
    pub public: PublicGameProjection,
    pub private: Option<PrivatePlayerProjection>,
    pub omniscient: Option<OmniscientProjection>,
    pub snapshot_state: Option<TableState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequestEnvelope {
    pub request_id: u64,
    pub view: GameProjection,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicGameProjection {
    pub board: BoardProjection,
    pub board_state: BoardState,
    pub bank: PublicBankProjection,
    pub players: Vec<PublicPlayerProjection>,
    pub builds: Vec<PlayerBuildProjection>,
    pub trade_sessions: Vec<TradeSessionProjection>,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSessionProjection {
    pub id: TradeSessionId,
    pub proposer: PlayerId,
    pub offers: Vec<TradeOfferProjection>,
    pub responses: Vec<Option<TradeResponseState>>,
    pub version: u64,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOfferProjection {
    pub id: TradeOfferId,
    pub proposer: PlayerId,
    pub trade: PlayerTrade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerBuildProjection {
    pub player_id: PlayerId,
    pub establishments: Vec<Establishment>,
    pub roads: Vec<Road>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardProjection {
    pub n_players: usize,
    pub field_radius: u8,
    pub tiles: Vec<Tile>,
    pub ports: Vec<(PortPos, PortKind)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicBankProjection {
    pub resources: PublicBankResourcesProjection,
    pub dev_cards: PublicBankDevCardsProjection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PublicBankResourcesProjection {
    Exact(ResourceSet),
    Approx(ResourceMap<DeckFullnessLevel>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PublicBankDevCardsProjection {
    Exact(u16),
    Approx(DeckFullnessLevel),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlayerProjection {
    pub player_id: PlayerId,
    pub resources: PublicPlayerResourcesProjection,
    pub queued_dev_cards: u16,
    pub active_dev_cards: u16,
    pub played_dev_cards: UsableDevCardSet,
    pub victory_points: Option<u16>,
    pub longest_road_length: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PublicPlayerResourcesProjection {
    Exact(ResourceSet),
    Total(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivatePlayerProjection {
    pub player_id: PlayerId,
    pub resources: ResourceSet,
    pub dev_cards: DevCardData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniscientProjection {
    pub players: Vec<PrivatePlayerProjection>,
    pub bank_resources: ResourceSet,
}

impl GameProjection {
    pub fn from_decision(context: &PlayerDecisionContext<'_>) -> Self {
        Self {
            actor: Some(context.actor),
            public: PublicGameProjection::from_public(&context.public),
            private: Some(PrivatePlayerProjection::from_private(&context.private)),
            omniscient: None,
            snapshot_state: None,
        }
    }

    pub fn from_player_notification(context: &PlayerNotificationContext<'_>) -> Self {
        Self {
            actor: Some(context.self_id),
            public: PublicGameProjection::from_public(&context.public),
            private: Some(PrivatePlayerProjection::from_private(&context.private)),
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
                public: PublicGameProjection::from_public(&public),
                private: None,
                omniscient: None,
                snapshot_state: None,
            },
            ObserverNotificationContext::Player { public, private } => Self {
                actor: Some(private.player_id),
                public: PublicGameProjection::from_public(&public),
                private: Some(PrivatePlayerProjection::from_private(&private)),
                omniscient: None,
                snapshot_state: None,
            },
            ObserverNotificationContext::Omniscient { public, full } => Self {
                actor: None,
                public: PublicGameProjection::from_public(&public),
                private: None,
                omniscient: Some(OmniscientProjection::from_full(&full)),
                snapshot_state: include_snapshot_state.then(|| full.state.clone()),
            },
        }
    }
}

pub fn game_projection_summary(model: &GameProjection) -> String {
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

impl PublicGameProjection {
    fn from_public(public: &PublicGameView<'_>) -> Self {
        Self {
            board: BoardProjection::from_board(public.board),
            board_state: *public.board_state,
            bank: PublicBankProjection::from_public(&public.bank),
            players: public
                .players
                .iter()
                .map(|player| PublicPlayerProjection {
                    player_id: player.player_id,
                    resources: match player.resources {
                        PublicPlayerResources::Exact(resources) => {
                            PublicPlayerResourcesProjection::Exact(resources)
                        }
                        PublicPlayerResources::Total(total) => {
                            PublicPlayerResourcesProjection::Total(total)
                        }
                    },
                    queued_dev_cards: player.dev_cards.queued,
                    active_dev_cards: player.dev_cards.active,
                    played_dev_cards: player.dev_cards.played,
                    victory_points: match player.dev_cards.victory_points {
                        PublicVpKnowledge::Hidden => None,
                        PublicVpKnowledge::Exact(points) => Some(points),
                    },
                    longest_road_length: player.longest_road_length,
                })
                .collect(),
            builds: public
                .builds
                .players_indexed()
                .map(|(player_id, builds)| PlayerBuildProjection {
                    player_id,
                    establishments: builds.establishments.iter().copied().collect(),
                    roads: builds.roads.iter().collect(),
                })
                .collect(),
            trade_sessions: public
                .trade_sessions
                .iter()
                .filter(|session| session.open)
                .map(|session| TradeSessionProjection {
                    id: session.id,
                    proposer: session.proposer,
                    offers: session
                        .offers
                        .iter()
                        .map(|offer| TradeOfferProjection {
                            id: offer.id,
                            proposer: offer.proposer,
                            trade: offer.trade,
                        })
                        .collect(),
                    responses: session.responses.clone(),
                    version: session.version,
                    open: session.open,
                })
                .collect(),
            longest_road_owner: public.longest_road_owner,
            largest_army_owner: public.largest_army_owner,
        }
    }
}

impl BoardProjection {
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

impl PublicBankProjection {
    fn from_public(bank: &crate::gameplay::game::view::PublicBankView) -> Self {
        Self {
            resources: match &bank.resources {
                PublicBankResources::Exact(resources) => {
                    PublicBankResourcesProjection::Exact(*resources)
                }
                PublicBankResources::Approx(resources) => {
                    PublicBankResourcesProjection::Approx(*resources)
                }
            },
            dev_cards: match bank.dev_cards {
                PublicBankDevCards::Exact(count) => PublicBankDevCardsProjection::Exact(count),
                PublicBankDevCards::Approx(level) => PublicBankDevCardsProjection::Approx(level),
            },
        }
    }
}

impl PrivatePlayerProjection {
    fn from_private(private: &PrivatePlayerView<'_>) -> Self {
        Self {
            player_id: private.player_id,
            resources: *private.resources,
            dev_cards: private.dev_cards.clone(),
        }
    }
}

impl OmniscientProjection {
    fn from_full(full: &OmniscientGameView<'_>) -> Self {
        Self {
            players: full
                .state
                .players
                .iter()
                .enumerate()
                .map(|(player_id, player)| PrivatePlayerProjection {
                    player_id: PlayerId::try_from(player_id)
                        .expect("player count should fit in u8"),
                    resources: *player.resources(),
                    dev_cards: player.dev_cards().clone(),
                })
                .collect(),
            bank_resources: full.state.bank.resources,
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

#[cfg(test)]
mod tests {
    use crate::gameplay::{
        game::{
            index::GameIndex,
            projection::{
                BoardProjection, GameProjection, LegalDecisionOptions,
                PublicBankResourcesProjection,
            },
            state::SetupGameState,
            trade::TradeSession,
            view::{ContextFactory, SearchFactory, VisibilityConfig},
        },
        primitives::{
            dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceSet},
            trade::PlayerTrade,
        },
    };

    const P0: PlayerId = PlayerId::new(0);

    #[test]
    fn projection_serializes_owned_public_board() {
        let state = SetupGameState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };

        let board = BoardProjection::from_board(&state.board);
        let projection = GameProjection::from_decision(&factory.player_decision_context(P0, None));

        serde_json::to_vec(&board).unwrap();
        serde_json::to_vec(&projection).unwrap();
    }

    #[test]
    fn projection_projects_open_trade_sessions() {
        let state = SetupGameState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let sessions = vec![TradeSession::new(
            crate::gameplay::game::trade::TradeSessionId(7),
            P0,
            PlayerTrade {
                give: ResourceSet::from(Resource::Brick),
                take: ResourceSet::from(Resource::Ore),
            },
            state.players.count(),
        )];
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &sessions,
        };

        let projection = GameProjection::from_decision(&factory.player_decision_context(P0, None));

        assert_eq!(projection.public.trade_sessions.len(), 1);
        assert_eq!(projection.public.trade_sessions[0].id.0, 7);
        assert_eq!(
            projection.public.trade_sessions[0].offers[0]
                .trade
                .give
                .brick,
            1
        );
        assert_eq!(
            projection.public.trade_sessions[0].offers[0].trade.take.ore,
            1
        );
    }

    #[test]
    fn legal_options_attach_trades_and_dev_card_usages() {
        let mut state = SetupGameState::default().finish();
        state
            .transfer_from_bank(
                ResourceSet {
                    brick: 4,
                    wheat: 1,
                    sheep: 1,
                    ore: 1,
                    ..ResourceSet::EMPTY
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
            trade_sessions: &[],
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(P0), P0));
        let context = factory.player_decision_context(P0, search);

        let legal = LegalDecisionOptions::from_context(&context, None);

        assert!(!legal.initial_placements.is_empty());
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
                .any(|usage| matches!(usage, DevCardUsage::YearOfPlenty([_, _])))
        );
    }

    #[test]
    fn spectator_projection_uses_public_bank_visibility() {
        let state = SetupGameState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };

        let projection = GameProjection::from_observer(
            crate::gameplay::game::event::ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        );

        assert!(matches!(
            projection.public.bank.resources,
            PublicBankResourcesProjection::Approx(_)
        ));
        assert!(projection.snapshot_state.is_none());
    }
}
