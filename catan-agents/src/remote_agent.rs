use catan_core::agent::{Agent, AgentRequest, AgentResponse};
use std::sync::mpsc::{Receiver, Sender};

pub struct RemoteAgent {
    req_tx: Sender<AgentRequest>,
    resp_rx: Receiver<AgentResponse>,
}

impl RemoteAgent {
    pub fn new(tx: Sender<AgentRequest>, rx: Receiver<AgentResponse>) -> Self {
        Self {
            req_tx: tx,
            resp_rx: rx,
        }
    }
}

impl Agent for RemoteAgent {
    fn respond(&mut self, request: AgentRequest) -> AgentResponse {
        self.req_tx.send(request).unwrap();
        self.resp_rx.recv().unwrap()
    }
}
