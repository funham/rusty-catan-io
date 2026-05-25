use serde::{Deserialize, Serialize};

use crate::gameplay::{
    game::{
        command::{
            ChooseRobbedPlayerCommand, DiscardHalfCommand, InitCommand, InitialPlacementCommand,
            MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
        },
        decision::{DecisionId, DecisionKind, OpenDecision},
        trade::TradeOfferId,
    },
    primitives::{player::PlayerId, trade::PlayerTrade},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameInput {
    Start,
    Submit(DecisionResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerCommand {
    InitialPlacement(InitialPlacementCommand),
    InitCommand(InitCommand),
    PostDice(PostDiceCommand),
    PostDevCard(PostDevCardCommand),
    Regular(RegularCommand),
    MoveRobber(MoveRobberCommand),
    ChooseRobbedPlayer(ChooseRobbedPlayerCommand),
    DiscardHalf(DiscardHalfCommand),
    Trade(TradeCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub token: DecisionToken,
    pub kind: DecisionKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionToken {
    pub id: DecisionId,
    pub player_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResponse {
    pub token: DecisionToken,
    pub command: PlayerCommand,
}

impl DecisionRequest {
    pub fn from_open_decision(decision: &OpenDecision) -> Self {
        Self {
            token: DecisionToken::from(decision),
            kind: decision.kind,
        }
    }

    pub fn player_id(&self) -> PlayerId {
        self.token.player_id
    }

    pub fn id(&self) -> DecisionId {
        self.token.id
    }

    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    pub fn token(&self) -> DecisionToken {
        self.token
    }

    pub fn respond_initial_placement(
        &self,
        command: InitialPlacementCommand,
    ) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::InitialPlacement)
            .then(|| self.respond(PlayerCommand::InitialPlacement(command)))
    }

    pub fn respond_init(&self, command: InitCommand) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::InitCommand)
            .then(|| self.respond(PlayerCommand::InitCommand(command)))
    }

    pub fn respond_post_dice(&self, command: PostDiceCommand) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::PostDiceCommand)
            .then(|| self.respond(PlayerCommand::PostDice(command)))
    }

    pub fn respond_post_dev_card(&self, command: PostDevCardCommand) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::PostDevCardCommand)
            .then(|| self.respond(PlayerCommand::PostDevCard(command)))
    }

    pub fn respond_regular(&self, command: RegularCommand) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::RegularCommand)
            .then(|| self.respond(PlayerCommand::Regular(command)))
    }

    pub fn respond_move_robber(&self, command: MoveRobberCommand) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::MoveRobber)
            .then(|| self.respond(PlayerCommand::MoveRobber(command)))
    }

    pub fn respond_choose_robbed_player(
        &self,
        command: ChooseRobbedPlayerCommand,
    ) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::ChooseRobbedPlayer { .. })
            .then(|| self.respond(PlayerCommand::ChooseRobbedPlayer(command)))
    }

    pub fn respond_discard_half(&self, command: DiscardHalfCommand) -> Option<DecisionResponse> {
        matches!(self.kind, DecisionKind::DiscardHalf { .. })
            .then(|| self.respond(PlayerCommand::DiscardHalf(command)))
    }

    pub fn respond_trade(&self, command: TradeCommand) -> Option<DecisionResponse> {
        matches!(
            self.kind,
            DecisionKind::TradeResponse { .. } | DecisionKind::TradeOwnerAction { .. }
        )
        .then(|| self.respond(PlayerCommand::Trade(command)))
    }

    fn respond(&self, command: PlayerCommand) -> DecisionResponse {
        DecisionResponse {
            token: self.token,
            command,
        }
    }

    pub fn respond_command(&self, command: PlayerCommand) -> Option<DecisionResponse> {
        match (self.kind, command) {
            (DecisionKind::InitialPlacement, PlayerCommand::InitialPlacement(command)) => {
                self.respond_initial_placement(command)
            }
            (DecisionKind::InitCommand, PlayerCommand::InitCommand(command)) => {
                self.respond_init(command)
            }
            (DecisionKind::PostDiceCommand, PlayerCommand::PostDice(command)) => {
                self.respond_post_dice(command)
            }
            (DecisionKind::PostDevCardCommand, PlayerCommand::PostDevCard(command)) => {
                self.respond_post_dev_card(command)
            }
            (DecisionKind::RegularCommand, PlayerCommand::Regular(command)) => {
                self.respond_regular(command)
            }
            (DecisionKind::MoveRobber, PlayerCommand::MoveRobber(command)) => {
                self.respond_move_robber(command)
            }
            (
                DecisionKind::ChooseRobbedPlayer { .. },
                PlayerCommand::ChooseRobbedPlayer(command),
            ) => self.respond_choose_robbed_player(command),
            (DecisionKind::DiscardHalf { .. }, PlayerCommand::DiscardHalf(command)) => {
                self.respond_discard_half(command)
            }
            (
                DecisionKind::TradeResponse { .. } | DecisionKind::TradeOwnerAction { .. },
                PlayerCommand::Trade(command),
            ) => self.respond_trade(command),
            _ => None,
        }
    }
}

impl From<&OpenDecision> for DecisionToken {
    fn from(decision: &OpenDecision) -> Self {
        Self {
            id: decision.id,
            player_id: decision.player_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeCommand {
    Propose { offer: PlayerTrade },
    Respond(TradeResponseCommand),
    Commit { offer_id: TradeOfferId },
    Reject { offer_id: TradeOfferId },
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeResponseCommand {
    Accept { offer_id: TradeOfferId },
    Reject,
    Counter { offer: PlayerTrade },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::game::{
        command::InitCommand,
        decision::{DecisionLifetime, OpenDecision},
        trade::TradeSessionId,
    };

    const P0: PlayerId = PlayerId::new(0);

    #[test]
    fn decision_request_preserves_trade_session_kind() {
        let response = DecisionRequest::from_open_decision(&OpenDecision {
            id: DecisionId(7),
            player_id: P0,
            kind: DecisionKind::TradeResponse {
                session: TradeSessionId(42),
            },
            lifetime: DecisionLifetime::UntilSessionClosed(TradeSessionId(42)),
        });
        assert_eq!(
            response.kind(),
            DecisionKind::TradeResponse {
                session: TradeSessionId(42)
            }
        );

        let owner = DecisionRequest::from_open_decision(&OpenDecision {
            id: DecisionId(8),
            player_id: P0,
            kind: DecisionKind::TradeOwnerAction {
                session: TradeSessionId(43),
            },
            lifetime: DecisionLifetime::UntilSessionClosed(TradeSessionId(43)),
        });
        assert_eq!(
            owner.kind(),
            DecisionKind::TradeOwnerAction {
                session: TradeSessionId(43)
            }
        );
    }

    #[test]
    fn respond_command_rejects_wrong_decision_kind() {
        let request = DecisionRequest::from_open_decision(&OpenDecision {
            id: DecisionId(9),
            player_id: P0,
            kind: DecisionKind::InitialPlacement,
            lifetime: DecisionLifetime::OneShot,
        });

        assert!(
            request
                .respond_command(PlayerCommand::InitCommand(InitCommand::RollDice))
                .is_none()
        );
    }
}
