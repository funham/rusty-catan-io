use catan_core::gameplay::{game::view::GameSnapshot, primitives::build::EstablishmentType};

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
                player
                    .builds
                    .establishments
                    .iter()
                    .filter(|est| est.stage == EstablishmentType::Settlement)
                    .count(),
                player
                    .builds
                    .establishments
                    .iter()
                    .filter(|est| est.stage == EstablishmentType::City)
                    .count(),
                player.builds.roads.len(),
            );
        }
    }
}
