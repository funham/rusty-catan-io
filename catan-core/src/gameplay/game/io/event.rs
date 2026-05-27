use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{
    constants::capacities::{
        EVENT_BATCH_INLINE, EVENT_RECIPIENTS_INLINE, RESOURCE_DISTRIBUTION_INLINE,
    },
    gameplay::{
        game::{
            decision::{DecisionId, OpenDecision},
            input::DecisionToken,
            output::CommandRejectionReason,
            run::GameResult,
            trade::{TradeOfferId, TradeResponseState, TradeSessionId},
            view::{
                OmniscientGameView, PlayerNotificationContext, PrivatePlayerView, PublicGameView,
            },
        },
        primitives::{
            build::{Build, Road},
            dev_card::{DevCardKind, DevCardUsage},
            player::PlayerId,
            resource::{Resource, ResourceSet},
            trade::{BankTrade, PlayerTrade},
        },
    },
    math::dice::DiceRoll,
    topology::{Hex, Intersection},
};

pub type EventBatch = SmallVec<[GameEvent; EVENT_BATCH_INLINE]>;
pub type ResourceDistribution = SmallVec<[(PlayerId, ResourceSet); RESOURCE_DISTRIBUTION_INLINE]>;
pub type EventRecipients = SmallVec<[PlayerId; EVENT_RECIPIENTS_INLINE]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCause {
    Start,
    PlayerCommand(DecisionToken),
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTransaction {
    pub cause: EventCause,
    pub events: EventBatch,
}

impl EventTransaction {
    pub fn new(cause: EventCause) -> Self {
        Self {
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
        token: DecisionToken,
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
    InitialResourcesGranted {
        player_id: PlayerId,
        resources: ResourceSet,
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
    BankTradeCompleted {
        player_id: PlayerId,
        trade: BankTrade,
    },
    TradeOpened {
        session_id: TradeSessionId,
        proposer_id: PlayerId,
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
        offer_proposer_id: PlayerId,
        trade: PlayerTrade,
    },
    TradeCancelled {
        session_id: TradeSessionId,
        proposer_id: PlayerId,
    },
    PlayerDiscarded {
        player_id: PlayerId,
        resources: ResourceSet,
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
    GameEnded {
        result: GameResult,
    },
}
