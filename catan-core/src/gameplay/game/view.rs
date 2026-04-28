use crate::{
    algorithm,
    gameplay::{
        field::state::{BoardLayout, BoardState},
        game::{index::GameIndex, query::GameQuery, state::GameState},
        primitives::{
            bank::{Bank, DeckFullnessLevel},
            build::BoardBuildData,
            dev_card::{DevCardData, UsableDevCardCollection},
            player::PlayerId,
            resource::{ResourceCollection, ResourceMap},
            turn::GameTurn,
        },
    },
    topology::Hex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountingMode {
    Human,
    Counting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerVisibility {
    pub id: PlayerId,
    pub counting: CountingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectatorVisibility {
    pub counting: CountingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityPolicy {
    Player(PlayerVisibility),
    Spectator(SpectatorVisibility),
    Omniscient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityConfig {
    pub player_mode: CountingMode,
    pub spectator_mode: CountingMode,
}

impl Default for VisibilityConfig {
    fn default() -> Self {
        Self {
            player_mode: CountingMode::Human,
            spectator_mode: CountingMode::Human,
        }
    }
}

impl VisibilityConfig {
    pub fn player_policy(&self, id: PlayerId) -> VisibilityPolicy {
        VisibilityPolicy::Player(PlayerVisibility {
            id,
            counting: self.player_mode,
        })
    }

    pub fn spectator_policy(&self) -> VisibilityPolicy {
        VisibilityPolicy::Spectator(SpectatorVisibility {
            counting: self.spectator_mode,
        })
    }
}

#[derive(Debug, Clone)]
pub enum PublicBankResources {
    Exact(ResourceCollection),
    Approx(ResourceMap<DeckFullnessLevel>),
}

#[derive(Debug, Clone)]
pub struct PublicBankView {
    pub resources: PublicBankResources,
    pub dev_card_count: u16,
}

#[derive(Debug, Clone)]
pub enum PublicPlayerResources {
    Exact(ResourceCollection),
    Total(u16),
}

#[derive(Debug, Clone)]
pub enum PublicVpKnowledge {
    Hidden,
    Exact(u16),
}

#[derive(Debug, Clone)]
pub struct PublicPlayerDevCards {
    pub queued: u16,
    pub active: u16,
    pub played: UsableDevCardCollection,
    pub victory_points: PublicVpKnowledge,
}

#[derive(Debug, Clone)]
pub struct PublicPlayerView {
    pub player_id: PlayerId,
    pub resources: PublicPlayerResources,
    pub dev_cards: PublicPlayerDevCards,
}

#[derive(Debug, Clone)]
pub struct PublicGameView<'a> {
    pub turn: &'a GameTurn,
    pub board: &'a BoardLayout,
    pub board_state: &'a BoardState,
    pub bank: PublicBankView,
    pub players: Vec<PublicPlayerView>,
    pub builds: &'a BoardBuildData,
    pub longest_road_owner: Option<PlayerId>,
    pub largest_army_owner: Option<PlayerId>,
}

#[derive(Debug, Clone, Copy)]
pub struct PrivatePlayerView<'a> {
    pub player_id: PlayerId,
    pub resources: &'a ResourceCollection,
    pub dev_cards: &'a DevCardData,
}

#[derive(Debug, Clone, Copy)]
pub struct OmniscientGameView<'a> {
    pub state: &'a GameState,
    pub index: &'a GameIndex,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchFactory<'a> {
    state: &'a GameState,
    policy: VisibilityPolicy,
    root_player: PlayerId,
}

#[derive(Debug, Clone)]
pub struct SearchSeed {
    pub root_player: PlayerId,
    pub policy: VisibilityPolicy,
    pub state: GameState,
}

#[derive(Debug, Clone)]
pub struct PlayerDecisionContext<'a> {
    pub actor: PlayerId,
    pub public: PublicGameView<'a>,
    pub private: PrivatePlayerView<'a>,
    pub search: Option<SearchFactory<'a>>,
}

#[derive(Debug, Clone)]
pub struct PlayerNotificationContext<'a> {
    pub self_id: PlayerId,
    pub public: PublicGameView<'a>,
    pub private: PrivatePlayerView<'a>,
}

pub struct ContextFactory<'a> {
    pub state: &'a GameState,
    pub index: &'a GameIndex,
    pub visibility: &'a VisibilityConfig,
}

impl<'a> PublicGameView<'a> {
    pub fn is_player_on_hex(&self, id: PlayerId, hex: Hex) -> bool {
        algorithm::is_player_on_hex(hex, self.builds.by_player(id))
    }

    pub fn players_on_hex(&self, hex: Hex) -> Vec<PlayerId> {
        algorithm::players_on_hex(hex, self.builds.players().iter())
            .into_iter()
            .collect()
    }
}

impl<'a> SearchFactory<'a> {
    pub fn new(state: &'a GameState, policy: VisibilityPolicy, root_player: PlayerId) -> Self {
        Self {
            state,
            policy,
            root_player,
        }
    }

    pub fn make_owned(&self) -> SearchSeed {
        SearchSeed {
            root_player: self.root_player,
            policy: self.policy,
            state: self.state.clone(),
        }
    }
}

impl<'a> ContextFactory<'a> {
    pub fn player_decision_context(
        &self,
        player_id: PlayerId,
        search: Option<SearchFactory<'a>>,
    ) -> PlayerDecisionContext<'a> {
        PlayerDecisionContext {
            actor: player_id,
            public: self.public_view(self.visibility.player_policy(player_id)),
            private: self.private_view(player_id),
            search,
        }
    }

    pub fn player_notification_context(
        &self,
        player_id: PlayerId,
    ) -> PlayerNotificationContext<'a> {
        PlayerNotificationContext {
            self_id: player_id,
            public: self.public_view(self.visibility.player_policy(player_id)),
            private: self.private_view(player_id),
        }
    }

    pub fn spectator_public_view(&self) -> PublicGameView<'a> {
        self.public_view(self.visibility.spectator_policy())
    }

    pub fn omniscient_view(&self) -> OmniscientGameView<'a> {
        OmniscientGameView {
            state: self.state,
            index: self.index,
        }
    }

    pub fn public_view(&self, policy: VisibilityPolicy) -> PublicGameView<'a> {
        let query = GameQuery::new(self.state, self.index);

        PublicGameView {
            turn: &self.state.turn,
            board: &self.state.board,
            board_state: &self.state.board_state,
            bank: self.project_bank(policy),
            players: self.project_players(policy),
            builds: &self.state.builds,
            longest_road_owner: query.longest_road_owner(),
            largest_army_owner: query.largest_army_owner(),
        }
    }

    pub fn private_view(&self, player_id: PlayerId) -> PrivatePlayerView<'a> {
        let p = self.state.players.get(player_id);
        PrivatePlayerView {
            player_id,
            resources: p.resources(),
            dev_cards: p.dev_cards(),
        }
    }

    fn project_bank(&self, policy: VisibilityPolicy) -> PublicBankView {
        PublicBankView {
            resources: project_bank_resources(&self.state.bank, policy),
            dev_card_count: self.state.bank.dev_cards.len() as u16,
        }
    }

    fn project_players(&self, policy: VisibilityPolicy) -> Vec<PublicPlayerView> {
        self.state
            .players
            .iter()
            .enumerate()
            .map(|(player_id, player)| {
                project_player(player_id, player.resources(), player.dev_cards(), policy)
            })
            .collect()
    }
}

fn project_bank_resources(bank: &Bank, policy: VisibilityPolicy) -> PublicBankResources {
    match policy {
        VisibilityPolicy::Player(PlayerVisibility {
            counting: CountingMode::Counting,
            ..
        })
        | VisibilityPolicy::Spectator(SpectatorVisibility {
            counting: CountingMode::Counting,
        })
        | VisibilityPolicy::Omniscient => PublicBankResources::Exact(bank.resources),
        VisibilityPolicy::Player(PlayerVisibility {
            counting: CountingMode::Human,
            ..
        })
        | VisibilityPolicy::Spectator(SpectatorVisibility {
            counting: CountingMode::Human,
        }) => PublicBankResources::Approx(bank.public_view().resources),
    }
}

fn project_player(
    player_id: PlayerId,
    resources: &ResourceCollection,
    dev_cards: &DevCardData,
    policy: VisibilityPolicy,
) -> PublicPlayerView {
    let resources = match policy {
        VisibilityPolicy::Player(PlayerVisibility {
            counting: CountingMode::Counting,
            ..
        })
        | VisibilityPolicy::Spectator(SpectatorVisibility {
            counting: CountingMode::Counting,
        })
        | VisibilityPolicy::Omniscient => PublicPlayerResources::Exact(*resources),
        VisibilityPolicy::Player(_) | VisibilityPolicy::Spectator(_) => {
            PublicPlayerResources::Total(resources.total())
        }
    };

    let victory_points = match policy {
        VisibilityPolicy::Omniscient => PublicVpKnowledge::Exact(dev_cards.victory_pts),
        VisibilityPolicy::Player(_) | VisibilityPolicy::Spectator(_) => PublicVpKnowledge::Hidden,
    };

    PublicPlayerView {
        player_id,
        resources,
        dev_cards: PublicPlayerDevCards {
            queued: dev_cards.queued.total(),
            active: dev_cards.active.total(),
            played: dev_cards.used,
            victory_points,
        },
    }
}
