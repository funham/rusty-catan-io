use std::sync::mpsc;
use std::thread;

use catan_agents::remote_agent::RemoteAgent;
use catan_core::agent::{AgentRequest, AgentResponse};
use tokio::sync::mpsc::{Receiver, Sender};

use catan_core::GameRunner;
use catan_core::gameplay::game::init::GameInitializationState;

use crate::protocol::*;

pub fn spawn_game(mut from_player: Receiver<ClientToServer>, to_player: Sender<ServerToClient>) {
    thread::spawn(move || {
        // Agent → UI
        let (agent_req_tx, agent_req_rx) = mpsc::channel::<AgentRequest>();

        // UI → Agent
        let (agent_resp_tx, agent_resp_rx) = mpsc::channel::<AgentResponse>();

        let agent = RemoteAgent::new(agent_req_tx, agent_resp_rx);

        // Forward AgentRequest → WebSocket
        let to_player_clone = to_player.clone();
        thread::spawn(move || {
            while let Ok(req) = agent_req_rx.recv() {
                to_player_clone
                    .blocking_send(ServerToClient::AgentRequest(req))
                    .unwrap();
            }
        });

        // Forward WebSocket responses → Agent
        thread::spawn(move || {
            while let Some(msg) = from_player.blocking_recv() {
                if let ClientToServer::AgentResponse(resp) = msg {
                    agent_resp_tx.send(resp).unwrap();
                }
            }
        });

        // Build and run game (blocking)
        let init = GameInitializationState::new(todo!()); // replace later
        let mut runner = GameRunner::new();
        runner.run();
    });
}
