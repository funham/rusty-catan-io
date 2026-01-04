use catan_core::{
    agent::{
        action::{InitialAction, PostDiceThrowAnswer},
        *,
    },
    gameplay::game::state::Perspective,
};
use std::sync::mpsc::{Receiver, Sender};

pub struct RemoteAgent {
    tx: Sender<AgentRequest>,
    rx: Receiver<AgentResponse>,
}

impl RemoteAgent {
    pub fn new(tx: Sender<AgentRequest>, rx: Receiver<AgentResponse>) -> Self {
        Self { tx, rx }
    }
}

impl Agent for RemoteAgent {
    fn move_request_init(&mut self, p: &Perspective) -> InitialAction {
        self.tx.send(AgentRequest::Init(p.to_owned())).unwrap();
        match self.rx.recv().unwrap() {
            AgentResponse::Init(a) => a,
            _ => panic!("Protocol error"),
        }
    }

    fn move_request_after_dice_throw(&mut self, p: &Perspective) -> PostDiceThrowAnswer {
        self.tx.send(AgentRequest::AfterDice(p.to_owned())).unwrap();

        match self.rx.recv().unwrap() {
            AgentResponse::AfterDice(a) => a,
            _ => panic!("Protocol error"),
        }
    }

    fn move_request_rest(
        &mut self,
        perspective: &catan_core::gameplay::game::state::Perspective,
    ) -> action::FinalStateAnswer {
        todo!()
    }

    fn move_request_rob_hex(
        &mut self,
        perspective: &catan_core::gameplay::game::state::Perspective,
    ) -> catan_core::topology::Hex {
        todo!()
    }

    fn move_request_rob_id(
        &mut self,
        perspective: &catan_core::gameplay::game::state::Perspective,
    ) -> catan_core::gameplay::primitives::player::PlayerId {
        todo!()
    }

    fn initialization(
        &mut self,
        field: &catan_core::gameplay::field::state::FieldState,
        round: u8,
    ) -> (
        catan_core::gameplay::primitives::build::Settlement,
        catan_core::gameplay::primitives::build::Road,
    ) {
        todo!()
    }

    fn answer_to_trade(
        &mut self,
        perspective: &catan_core::gameplay::game::state::Perspective,
        trade: &catan_core::gameplay::primitives::trade::PlayerTrade,
    ) -> action::TradeAction {
        todo!()
    }

    fn drop_half(
        &mut self,
        perspective: &catan_core::gameplay::game::state::Perspective,
    ) -> catan_core::gameplay::primitives::resource::ResourceCollection {
        todo!()
    }

    // repeat for other methods
}
