use catan_core::gameplay::{
    game::{
        decision::DecisionKind,
        input::{DecisionRequest, PlayerCommand, TradeCommand, TradeResponseCommand},
        view::PlayerDecisionContext,
    },
    primitives::player::PlayerId,
};

pub trait BotPolicy {
    fn player_id(&self) -> PlayerId;
    fn command_for(
        &mut self,
        request: &DecisionRequest,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand>;
}

pub fn decline_trade_command() -> PlayerCommand {
    PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Reject))
}

pub fn cancel_trade_command() -> PlayerCommand {
    PlayerCommand::Trade(TradeCommand::Cancel)
}

pub fn unsupported_decision_command(request: &DecisionRequest) -> Option<PlayerCommand> {
    match request.kind() {
        DecisionKind::TradeResponse { .. } => Some(decline_trade_command()),
        DecisionKind::TradeOwnerAction { .. } => Some(cancel_trade_command()),
        _ => None,
    }
}
