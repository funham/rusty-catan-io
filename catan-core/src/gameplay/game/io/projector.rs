use crate::gameplay::game::{
    event::{EventTransaction, EventVisibility, GameEvent},
    input::DecisionRequest,
    output::{GameEventRecord, GameOutput},
};

pub fn project_event(event: GameEvent) -> GameOutput {
    GameOutput::Event(GameEventRecord {
        visibility: EventVisibility::for_event(&event),
        event,
    })
}

pub fn project_transaction(transaction: &EventTransaction) -> Vec<GameOutput> {
    let mut outputs = Vec::new();
    for event in &transaction.events {
        outputs.push(project_event(event.clone()));
        match event {
            GameEvent::DecisionOpened(decision) => {
                outputs.push(GameOutput::DecisionOpened(
                    DecisionRequest::from_open_decision(decision),
                ));
            }
            GameEvent::DecisionClosed { decision_id } => {
                outputs.push(GameOutput::DecisionClosed {
                    decision_id: *decision_id,
                });
            }
            GameEvent::CommandRejected { token, reason, .. } => {
                outputs.push(GameOutput::CommandRejected {
                    token: *token,
                    reason: reason.clone(),
                });
            }
            _ => {}
        }
    }
    outputs
}

#[cfg(test)]
mod tests {
    use crate::gameplay::{
        game::{
            event::{EventCause, EventTransaction, EventVisibility, GameEvent},
            output::GameOutput,
        },
        primitives::{dev_card::DevCardKind, player::PlayerId},
    };

    use super::project_transaction;

    const P1: PlayerId = PlayerId::new(1);
    const P2: PlayerId = PlayerId::new(2);

    #[test]
    fn transaction_projection_preserves_event_and_visibility() {
        let mut transaction = EventTransaction::new(EventCause::Start);
        transaction.events.push(GameEvent::DevCardDrawn {
            player_id: P2,
            card: DevCardKind::VictoryPoint,
        });

        let outputs = project_transaction(&transaction);

        let [GameOutput::Event(record)] = outputs.as_slice() else {
            panic!("transaction should project to one event output");
        };
        assert!(matches!(
            record.event,
            GameEvent::DevCardDrawn { player_id: P2, .. }
        ));
        let mut expected_recipients = smallvec::SmallVec::new();
        expected_recipients.push(P2);
        assert_eq!(
            record.visibility,
            EventVisibility::PrivateTo(expected_recipients)
        );
    }

    #[test]
    fn transaction_projection_includes_compatibility_decision_output() {
        use crate::gameplay::game::decision::{
            DecisionId, DecisionKind, DecisionLifetime, OpenDecision,
        };

        let mut transaction = EventTransaction::new(EventCause::Start);
        transaction
            .events
            .push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(3),
                player_id: P1,
                kind: DecisionKind::InitialPlacement,
                lifetime: DecisionLifetime::OneShot,
            }));

        let outputs = project_transaction(&transaction);

        assert!(matches!(outputs.as_slice(), [
            GameOutput::Event(record),
            GameOutput::DecisionOpened(decision),
        ] if matches!(record.event, GameEvent::DecisionOpened(_)) && decision.id() == DecisionId(3)));
    }
}
