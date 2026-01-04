use axum::{extract::ws::WebSocketUpgrade, response::IntoResponse};
use tokio::sync::mpsc;

use crate::{game::spawn_game, session::PlayerSession};

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move {
        let (to_game_tx, to_game_rx) = mpsc::channel(32);
        let (from_game_tx, from_game_rx) = mpsc::channel(32);

        spawn_game(to_game_rx, from_game_tx);

        let session = PlayerSession::new(socket, from_game_rx, to_game_tx);
        session.run().await;
    })
}
