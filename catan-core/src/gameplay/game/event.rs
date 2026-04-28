use serde::{Deserialize, Serialize};

use crate::{
    agent::action::RegularAction,
    gameplay::{
        game::view::{
            OmniscientGameView, PlayerNotificationContext, PrivatePlayerView, PublicGameView,
        },
        primitives::{
            build::{Build, Road},
            dev_card::DevCardUsage,
            player::PlayerId,
            resource::ResourceCollection,
        },
    },
    math::dice::DiceVal,
    topology::{Hex, Intersection},
};

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
        value: DiceVal,
    },
    ResourcesDistributed,
    DevCardBought {
        player_id: PlayerId,
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
    PlayerDiscarded {
        player_id: PlayerId,
        resources: ResourceCollection,
    },
    RobberMoved {
        player_id: PlayerId,
        hex: Hex,
        robbed_id: Option<PlayerId>,
    },
    ActionRejected {
        player_id: PlayerId,
        action: RegularAction,
        reason: String,
    },
    GameEnded {
        winner_id: PlayerId,
    },
    GameInterrupted {
        reason: String,
    },
}
