use crate::{agent::agent::Agent, gameplay::strategy::async_strategy::AsyncStrategy};

pub struct AsyncAgent {
    strategy: Box<dyn AsyncStrategy>,
}

impl Agent for AsyncAgent {
    fn move_request_init(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
    ) -> super::action::InitialAction {
        todo!()
    }

    fn move_request_after_dice_throw(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
    ) -> super::action::PostDiceThrowAnswer {
        todo!()
    }

    fn move_request_rest(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
    ) -> super::action::FinalStateAnswer {
        todo!()
    }

    fn move_request_rob_hex(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
    ) -> crate::topology::Hex {
        todo!()
    }

    fn move_request_rob_id(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
    ) -> crate::gameplay::primitives::player::PlayerId {
        todo!()
    }

    fn initialization(
        &mut self,
        field: &crate::gameplay::field::state::FieldState,
        round: u8,
    ) -> (
        crate::gameplay::primitives::build::Settlement,
        crate::gameplay::primitives::build::Road,
    ) {
        todo!()
    }

    fn answer_to_trade(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
        trade: &crate::gameplay::primitives::trade::PlayerTrade,
    ) -> super::action::TradeAction {
        todo!()
    }

    fn drop_half(
        &mut self,
        perspective: &crate::gameplay::game::state::Perspective,
    ) -> crate::gameplay::primitives::resource::ResourceCollection {
        todo!()
    }
}
