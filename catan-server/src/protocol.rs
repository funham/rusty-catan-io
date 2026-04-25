use catan_core::{agent::*, gameplay::game::event::GameEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerToClient {
    AgentRequest(AgentRequest),
    GameEvent(GameEvent),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientToServer {
    AgentResponse(AgentAction),
}
