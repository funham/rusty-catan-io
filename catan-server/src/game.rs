use tokio::sync::mpsc::{Receiver, Sender};

use crate::protocol::{ClientToServer, ServerToClient};

pub fn spawn_game(_from_player: Receiver<ClientToServer>, _to_player: Sender<ServerToClient>) {
    log::warn!("websocket game server is not wired for the current MVP runtime protocol");
}
