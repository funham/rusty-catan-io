use serde::{Deserialize, Serialize};

use crate::gameplay::primitives::player::PlayerId;

use super::{
    decision::DecisionId,
    event::{EventTransaction, EventVisibility, GameEvent},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEventRecord {
    pub tx_id: u64,
    pub event: GameEvent,
    pub visibility: EventVisibility,
}

impl GameEventRecord {
    pub fn from_transaction(transaction: &EventTransaction) -> Vec<Self> {
        transaction
            .events
            .iter()
            .cloned()
            .map(|event| Self {
                tx_id: transaction.tx_id,
                visibility: EventVisibility::for_event(&event),
                event,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameOutput {
    Event(GameEventRecord),
    DecisionOpened(super::decision::OpenDecision),
    DecisionClosed {
        decision_id: DecisionId,
    },
    CommandRejected {
        player_id: PlayerId,
        decision_id: Option<DecisionId>,
        reason: CommandRejectionReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandRejectionReason {
    WrongPlayer { expected: PlayerId },
    StaleDecision,
    WrongPhase,
    GameEnded,
    IllegalCommand(String),
}

#[cfg(test)]
mod tests {
    use crate::gameplay::{
        game::{
            event::{EventCause, EventTransaction, EventVisibility, GameEvent},
            output::GameEventRecord,
        },
        primitives::{dev_card::DevCardKind, player::PlayerId, resource::Resource},
    };

    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);
    const P2: PlayerId = PlayerId::new(2);

    #[test]
    fn event_visibility_marks_private_events() {
        let dev_card = EventVisibility::for_event(&GameEvent::DevCardDrawn {
            player_id: P1,
            card: DevCardKind::VictoryPoint,
        });
        let mut expected_dev_card = smallvec::SmallVec::new();
        expected_dev_card.push(P1);
        assert_eq!(dev_card, EventVisibility::PrivateTo(expected_dev_card));

        let stolen = EventVisibility::for_event(&GameEvent::ResourceStolen {
            player_id: P0,
            robbed_id: P2,
            resource: Resource::Brick,
        });
        let mut expected_stolen = smallvec::SmallVec::new();
        expected_stolen.push(P0);
        expected_stolen.push(P2);
        assert_eq!(stolen, EventVisibility::PrivateTo(expected_stolen));

        assert_eq!(
            EventVisibility::for_event(&GameEvent::GameStarted),
            EventVisibility::Public
        );
    }

    #[test]
    fn transaction_projects_events_with_tx_id_and_visibility() {
        let mut tx = EventTransaction::new(7, EventCause::Start);
        tx.events.push(GameEvent::GameStarted);

        let records = GameEventRecord::from_transaction(&tx);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tx_id, 7);
        assert!(matches!(records[0].event, GameEvent::GameStarted));
        assert_eq!(records[0].visibility, EventVisibility::Public);
    }
}
