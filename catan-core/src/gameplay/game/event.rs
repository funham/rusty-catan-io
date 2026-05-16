use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{
    gameplay::game::action::RegularAction,
    gameplay::{
        game::view::{
            OmniscientGameView, PlayerNotificationContext, PrivatePlayerView, PublicGameView,
        },
        game::{
            decision::{DecisionId, OpenDecision},
            output::CommandRejectionReason,
            run::GameResult,
            trade::{TradeOfferId, TradeResponseState, TradeScope, TradeSessionId},
        },
        primitives::{
            build::{Build, Road},
            dev_card::{DevCardKind, DevCardUsage},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::PlayerTrade,
        },
    },
    math::dice::DiceRoll,
    topology::{Hex, Intersection},
};

pub type EventBatch = SmallVec<[GameEvent; 32]>;
pub type ResourceDistribution = SmallVec<[(PlayerId, ResourceCollection); 8]>;
pub type EventRecipients = SmallVec<[PlayerId; 2]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCause {
    Start,
    PlayerCommand {
        player_id: PlayerId,
        decision_id: DecisionId,
    },
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTransaction {
    pub tx_id: u64,
    pub cause: EventCause,
    pub events: EventBatch,
}

impl EventTransaction {
    pub fn new(tx_id: u64, cause: EventCause) -> Self {
        Self {
            tx_id,
            cause,
            events: EventBatch::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventVisibility {
    Public,
    PrivateTo(EventRecipients),
    OmniscientOnly,
}

impl EventVisibility {
    pub fn for_event(event: &GameEvent) -> Self {
        match event {
            GameEvent::DevCardDrawn { player_id, .. } => {
                let mut recipients = EventRecipients::new();
                recipients.push(*player_id);
                Self::PrivateTo(recipients)
            }
            GameEvent::ResourceStolen {
                player_id,
                robbed_id,
                ..
            } => {
                let mut recipients = EventRecipients::new();
                recipients.push(*player_id);
                recipients.push(*robbed_id);
                Self::PrivateTo(recipients)
            }
            _ => Self::Public,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEndPlayerStats {
    pub player_id: PlayerId,
    pub total_vp: u16,
    pub build_and_dev_card_vp: u16,
    pub award_vp: u16,
    pub settlements: u16,
    pub cities: u16,
    pub roads: u16,
    pub longest_road_length: u16,
    pub knights_used: u16,
    pub has_longest_road: bool,
    pub has_largest_army: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverKind {
    Spectator,
    Player(PlayerId),
    Omniscient,
}

pub enum ObserverNotificationContext<'a> {
    Spectator {
        public: PublicGameView<'a>,
    },
    Player {
        public: PublicGameView<'a>,
        private: PrivatePlayerView<'a>,
    },
    Omniscient {
        public: PublicGameView<'a>,
        full: OmniscientGameView<'a>,
    },
}

pub trait GameObserver {
    fn kind(&self) -> ObserverKind;
    fn on_event(&mut self, event: &GameEvent, context: ObserverNotificationContext<'_>);
}

pub trait PlayerNotification {
    fn on_event(&mut self, event: &GameEvent, context: PlayerNotificationContext<'_>) {
        let _ = (event, context);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    GameStarted,
    DecisionOpened(OpenDecision),
    DecisionClosed {
        decision_id: DecisionId,
    },
    CommandRejected {
        player_id: PlayerId,
        decision_id: Option<DecisionId>,
        reason: CommandRejectionReason,
        counts_toward_limit: bool,
    },
    TurnStarted {
        player_id: PlayerId,
        turn_no: u64,
    },
    TurnEnded {
        player_id: PlayerId,
        turn_no: u64,
    },
    InitialPlacementBuilt {
        player_id: PlayerId,
        settlement: Intersection,
        road: Road,
    },
    DiceRolled {
        player_id: PlayerId,
        value: DiceRoll,
    },
    ResourcesDistributed {
        by_player: ResourceDistribution,
    },
    DevCardBought {
        player_id: PlayerId,
    },
    DevCardDrawn {
        player_id: PlayerId,
        card: DevCardKind,
    },
    DevCardUsed {
        player_id: PlayerId,
        usage: DevCardUsage,
    },
    Built {
        player_id: PlayerId,
        build: Build,
    },
    Traded {
        player_id: PlayerId,
    },
    TradeOpened {
        session_id: TradeSessionId,
        proposer_id: PlayerId,
        scope: TradeScope,
        offer_id: TradeOfferId,
        offer: PlayerTrade,
    },
    TradeOfferAdded {
        session_id: TradeSessionId,
        player_id: PlayerId,
        offer_id: TradeOfferId,
        offer: PlayerTrade,
    },
    TradeResponseUpdated {
        session_id: TradeSessionId,
        player_id: PlayerId,
        response: TradeResponseState,
    },
    TradeCompleted {
        session_id: TradeSessionId,
        proposer_id: PlayerId,
        peer_id: PlayerId,
        offer_id: TradeOfferId,
    },
    TradeCancelled {
        session_id: TradeSessionId,
        proposer_id: PlayerId,
    },
    PlayerDiscarded {
        player_id: PlayerId,
        resources: ResourceCollection,
    },
    RobberMoved {
        player_id: PlayerId,
        hex: Hex,
        robbed_id: Option<PlayerId>,
    },
    ResourceStolen {
        player_id: PlayerId,
        robbed_id: PlayerId,
        resource: Resource,
    },
    ActionRejected {
        player_id: PlayerId,
        action: RegularAction,
        reason: String,
    },
    GameEnded {
        winner_id: PlayerId,
        turn_no: u64,
        stats: Vec<GameEndPlayerStats>,
    },
    GameInterrupted {
        reason: String,
    },
    GameFinished {
        result: GameResult,
    },
}
