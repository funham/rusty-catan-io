use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerToClient {
    Text(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientToServer {
    Text(String),
}
