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
    transaction
        .events
        .iter()
        .cloned()
        .map(|event| project_event(transaction.tx_id, event))
        .collect()
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
}
