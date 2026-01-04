use tokio::sync::mpsc::{Receiver, Sender};
use crate::protocol::*;

pub async fn run_game(
    mut from_player: Receiver<ClientToServer>,
    to_player: Sender<ServerToClient>,
) {
    to_player
        .send(ServerToClient::Text("Game started".into()))
        .await
        .unwrap();

    while let Some(msg) = from_player.recv().await {
        match msg {
            ClientToServer::Text(t) => {
                to_player
                    .send(ServerToClient::Text(format!(
                        "Echo from game: {}",
                        t
                    )))
                    .await
                    .unwrap();
            }
        }
    }
}
