use catan_core::GameEvent;
use catan_core::agent::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerToClient {
    AgentRequest(AgentRequest),
    GameEvent(GameEvent),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientToServer {
    AgentResponse(AgentResponse),
}
