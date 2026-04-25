// use catan_core::{
//     agent::Agent,
//     gameplay::{
//         game::event::{GameEvent, PlayerObserver},
//         primitives::player::PlayerId,
//     },
// };
// use std::sync::mpsc::{Receiver, Sender};

// pub struct RemoteAgent {
//     player_id: PlayerId,
//     req_tx: Sender<AgentRequest>,
//     resp_rx: Receiver<AgentAction>,
//     event_rx: Receiver<GameEvent>,
// }

// impl RemoteAgent {
//     pub fn new(
//         id: PlayerId,
//         tx: Sender<AgentRequest>,
//         rx: Receiver<AgentAction>,
//         event_rx: Receiver<GameEvent>,
//     ) -> Self {
//         Self {
//             player_id: id,
//             req_tx: tx,
//             resp_rx: rx,
//             event_rx,
//         }
//     }
// }

// impl PlayerObserver for RemoteAgent {
//     fn player_id(&self) -> PlayerId {
//         todo!()
//     }
// }

// impl Agent for RemoteAgent {
//     fn respond(&mut self, request: AgentRequest) -> AgentAction {
//         self.req_tx.send(request).unwrap();
//         self.resp_rx.recv().unwrap()
//     }
// }
