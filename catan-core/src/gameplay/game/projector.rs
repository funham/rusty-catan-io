use crate::gameplay::game::{
    event::{EventTransaction, EventVisibility, GameEvent},
    output::{GameEventRecord, GameOutput},
};

pub fn project_event(tx_id: u64, event: GameEvent) -> GameOutput {
    GameOutput::Event(GameEventRecord {
        tx_id,
        visibility: EventVisibility::for_event(&event),
        event,
    })
}

pub fn project_transaction(transaction: &EventTransaction) -> Vec<GameOutput> {
    let mut outputs = Vec::new();
    for event in &transaction.events {
        outputs.push(project_event(transaction.tx_id, event.clone()));
        match event {
            GameEvent::DecisionOpened(decision) => {
                outputs.push(GameOutput::DecisionOpened(decision.clone()));
            }
            GameEvent::DecisionClosed { decision_id } => {
                outputs.push(GameOutput::DecisionClosed {
                    decision_id: *decision_id,
                });
            }
            GameEvent::CommandRejected {
                player_id,
                decision_id,
                reason,
                ..
            } => {
                outputs.push(GameOutput::CommandRejected {
                    player_id: *player_id,
                    decision_id: *decision_id,
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
            event::{EventCause, EventTransaction, GameEvent},
            output::GameOutput,
        },
        primitives::dev_card::DevCardKind,
    };

    use super::project_transaction;

    #[test]
    fn transaction_projection_preserves_tx_id_and_visibility() {
        let mut transaction = EventTransaction::new(9, EventCause::Start);
        transaction.events.push(GameEvent::DevCardDrawn {
            player_id: 2,
            card: DevCardKind::VictoryPoint,
        });

        let outputs = project_transaction(&transaction);

        let [GameOutput::Event(record)] = outputs.as_slice() else {
            panic!("transaction should project to one event output");
        };
        assert_eq!(record.tx_id, 9);
        assert!(matches!(
            record.event,
            GameEvent::DevCardDrawn { player_id: 2, .. }
        ));
    }

    #[test]
    fn transaction_projection_includes_compatibility_decision_output() {
        use crate::gameplay::game::decision::{
            DecisionId, DecisionKind, DecisionLifetime, OpenDecision,
        };

        let mut transaction = EventTransaction::new(12, EventCause::Start);
        transaction.events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(3),
            player_id: 1,
            kind: DecisionKind::InitPlacement,
            lifetime: DecisionLifetime::OneShot,
        }));

        let outputs = project_transaction(&transaction);

        assert!(matches!(outputs.as_slice(), [
            GameOutput::Event(record),
            GameOutput::DecisionOpened(decision),
        ] if record.tx_id == 12 && decision.id == DecisionId(3)));
    }
}
