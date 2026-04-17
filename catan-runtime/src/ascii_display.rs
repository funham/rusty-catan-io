use catan_core::{GameEvent, GameObserver, gameplay::game::state::GameSnapshot};

#[derive(Default)]
pub struct AsciiDisplay;

impl AsciiDisplay {
    fn print_snapshot(snapshot: &GameSnapshot) {
        println!(
            "turn={} round={} robber={:?}",
            snapshot.current_player_id, snapshot.rounds_played, snapshot.field.robber_pos
        );
        for player in &snapshot.players {
            println!(
                "player {} => cards={}, settlements={}, cities={}, roads={}",
                player.player_id,
                player.public_data.resource_card_count,
                player.builds.settlements.len(),
                player.builds.cities.len(),
                player.builds.roads.len(),
            );
        }
    }
}

impl GameObserver for AsciiDisplay {
    fn on_event(&mut self, event: &GameEvent) {
        match event {
            GameEvent::GameStarted { snapshot } => {
                println!("== game started ==");
                Self::print_snapshot(snapshot);
            }
            GameEvent::TurnStarted { snapshot } => {
                println!("\n== turn started: player {} ==", snapshot.current_player_id);
                Self::print_snapshot(snapshot);
            }
            GameEvent::DiceRolled {
                player_id,
                value,
                snapshot,
            } => {
                let value: u8 = (*value).into();
                println!("player {} rolled {}", player_id, value);
                Self::print_snapshot(snapshot);
            }
            GameEvent::BuildPlaced {
                player_id,
                build,
                snapshot,
            } => {
                println!("player {} built {:?}", player_id, build);
                Self::print_snapshot(snapshot);
            }
            GameEvent::PlayerDiscarded {
                player_id,
                discarded,
                snapshot,
            } => {
                println!("player {} discarded {:?}", player_id, discarded);
                Self::print_snapshot(snapshot);
            }
            GameEvent::RobberMoved {
                player_id,
                hex,
                robbed_id,
                snapshot,
            } => {
                println!(
                    "player {} moved robber to {:?}, robbed {:?}",
                    player_id, hex, robbed_id
                );
                Self::print_snapshot(snapshot);
            }
            GameEvent::GameFinished { winner_id, snapshot } => {
                println!("\n== game finished: winner {} ==", winner_id);
                Self::print_snapshot(snapshot);
            }
        }
    }
}
