use crate::protocol::*;
use axum::extract::ws::{Message, WebSocket};
use tokio::sync::mpsc::{Receiver, Sender};

pub struct PlayerSession {
    socket: WebSocket,
    from_game: Receiver<ServerToClient>,
    to_game: Sender<ClientToServer>,
}

impl PlayerSession {
    pub fn new(
        socket: WebSocket,
        from_game: Receiver<ServerToClient>,
        to_game: Sender<ClientToServer>,
    ) -> Self {
        Self {
            socket,
            from_game,
            to_game,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.socket.recv() => {
                    if let Ok(Message::Text(text)) = msg {
                        let parsed: ClientToServer =
                            serde_json::from_str(&text).unwrap();
                        self.to_game.send(parsed).await.unwrap();
                    }
                }

                Some(msg) = self.from_game.recv() => {
                    let text = serde_json::to_string(&msg).unwrap();
                    self.socket.send(Message::Text(text)).await.unwrap();
                }
            }
        }
    }
}
