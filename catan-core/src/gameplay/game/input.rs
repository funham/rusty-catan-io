use serde::{Deserialize, Serialize};

use crate::{
    gameplay::{
        game::command::{
            ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
            MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
        },
        primitives::{
            player::PlayerId,
            trade::{PlayerTrade, PublicTradeOffer},
        },
    },
    topology::Hex,
};

use super::{
    decision::{DecisionId, DecisionKind, OpenDecision},
    trade::{TradeOfferId, TradeScope},
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
    DropHalf(DropHalfCommand),
    Trade(TradeCommand),
}

// TODO: move decision token outside of enum variants, and provide it parallel instead
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionRequest {
    InitialPlacement(DecisionToken),
    InitCommand(DecisionToken),
    PostDice(DecisionToken),
    PostDevCard(DecisionToken),
    Regular(DecisionToken),
    MoveRobber(DecisionToken),
    ChooseRobbedPlayer {
        token: DecisionToken,
        robber_pos: Hex,
    },
    DropHalf {
        token: DecisionToken,
        required: u16,
    },
    TradeResponse {
        token: DecisionToken,
    },
    TradeOwner {
        token: DecisionToken,
    },
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
        let token = DecisionToken::from(decision);
        match decision.kind {
            DecisionKind::InitialPlacement => Self::InitialPlacement(token),
            DecisionKind::InitCommand => Self::InitCommand(token),
            DecisionKind::PostDiceCommand => Self::PostDice(token),
            DecisionKind::PostDevCardCommand => Self::PostDevCard(token),
            DecisionKind::RegularCommand => Self::Regular(token),
            DecisionKind::MoveRobber => Self::MoveRobber(token),
            DecisionKind::ChooseRobbedPlayer { robber_pos } => {
                Self::ChooseRobbedPlayer { token, robber_pos }
            }
            DecisionKind::DropHalf { required } => Self::DropHalf { token, required },
            DecisionKind::TradeResponse { .. } => Self::TradeResponse { token },
            DecisionKind::TradeOwnerAction { .. } => Self::TradeOwner { token },
        }
    }

    pub fn player_id(&self) -> PlayerId {
        self.token().player_id
    }

    pub fn id(&self) -> DecisionId {
        self.token().id
    }

    pub fn kind(&self) -> DecisionKind {
        match self {
            Self::InitialPlacement(_) => DecisionKind::InitialPlacement,
            Self::InitCommand(_) => DecisionKind::InitCommand,
            Self::PostDice(_) => DecisionKind::PostDiceCommand,
            Self::PostDevCard(_) => DecisionKind::PostDevCardCommand,
            Self::Regular(_) => DecisionKind::RegularCommand,
            Self::MoveRobber(_) => DecisionKind::MoveRobber,
            Self::ChooseRobbedPlayer { robber_pos, .. } => DecisionKind::ChooseRobbedPlayer {
                robber_pos: *robber_pos,
            },
            Self::DropHalf { required, .. } => DecisionKind::DropHalf {
                required: *required,
            },
            Self::TradeResponse { .. } => DecisionKind::TradeResponse {
                session: super::trade::TradeSessionId(0),
            },
            Self::TradeOwner { .. } => DecisionKind::TradeOwnerAction {
                session: super::trade::TradeSessionId(0),
            },
        }
    }

    pub fn token(&self) -> DecisionToken {
        match self {
            Self::InitialPlacement(token)
            | Self::InitCommand(token)
            | Self::PostDice(token)
            | Self::PostDevCard(token)
            | Self::Regular(token)
            | Self::MoveRobber(token) => *token,
            Self::ChooseRobbedPlayer { token, .. }
            | Self::DropHalf { token, .. }
            | Self::TradeResponse { token }
            | Self::TradeOwner { token } => *token,
        }
    }

    pub fn respond_initial_placement(
        &self,
        command: InitialPlacementCommand,
    ) -> Option<DecisionResponse> {
        matches!(self, Self::InitialPlacement(_))
            .then(|| self.respond(PlayerCommand::InitialPlacement(command)))
    }

    pub fn respond_init(&self, command: InitCommand) -> Option<DecisionResponse> {
        matches!(self, Self::InitCommand(_))
            .then(|| self.respond(PlayerCommand::InitCommand(command)))
    }

    pub fn respond_post_dice(&self, command: PostDiceCommand) -> Option<DecisionResponse> {
        matches!(self, Self::PostDice(_)).then(|| self.respond(PlayerCommand::PostDice(command)))
    }

    pub fn respond_post_dev_card(&self, command: PostDevCardCommand) -> Option<DecisionResponse> {
        matches!(self, Self::PostDevCard(_))
            .then(|| self.respond(PlayerCommand::PostDevCard(command)))
    }

    pub fn respond_regular(&self, command: RegularCommand) -> Option<DecisionResponse> {
        matches!(self, Self::Regular(_)).then(|| self.respond(PlayerCommand::Regular(command)))
    }

    pub fn respond_move_robber(&self, command: MoveRobberCommand) -> Option<DecisionResponse> {
        matches!(self, Self::MoveRobber(_))
            .then(|| self.respond(PlayerCommand::MoveRobber(command)))
    }

    pub fn respond_choose_robbed_player(
        &self,
        command: ChooseRobbedPlayerCommand,
    ) -> Option<DecisionResponse> {
        matches!(self, Self::ChooseRobbedPlayer { .. })
            .then(|| self.respond(PlayerCommand::ChooseRobbedPlayer(command)))
    }

    pub fn respond_drop_half(&self, command: DropHalfCommand) -> Option<DecisionResponse> {
        matches!(self, Self::DropHalf { .. })
            .then(|| self.respond(PlayerCommand::DropHalf(command)))
    }

    pub fn respond_trade(&self, command: TradeCommand) -> Option<DecisionResponse> {
        matches!(self, Self::TradeResponse { .. } | Self::TradeOwner { .. })
            .then(|| self.respond(PlayerCommand::Trade(command)))
    }

    fn respond(&self, command: PlayerCommand) -> DecisionResponse {
        DecisionResponse {
            token: self.token(),
            command,
        }
    }

    pub fn respond_command(&self, command: PlayerCommand) -> Option<DecisionResponse> {
        match (self, command) {
            (Self::InitialPlacement(_), PlayerCommand::InitialPlacement(command)) => {
                self.respond_initial_placement(command)
            }
            (Self::InitCommand(_), PlayerCommand::InitCommand(command)) => {
                self.respond_init(command)
            }
            (Self::PostDice(_), PlayerCommand::PostDice(command)) => {
                self.respond_post_dice(command)
            }
            (Self::PostDevCard(_), PlayerCommand::PostDevCard(command)) => {
                self.respond_post_dev_card(command)
            }
            (Self::Regular(_), PlayerCommand::Regular(command)) => self.respond_regular(command),
            (Self::MoveRobber(_), PlayerCommand::MoveRobber(command)) => {
                self.respond_move_robber(command)
            }
            (Self::ChooseRobbedPlayer { .. }, PlayerCommand::ChooseRobbedPlayer(command)) => {
                self.respond_choose_robbed_player(command)
            }
            (Self::DropHalf { .. }, PlayerCommand::DropHalf(command)) => {
                self.respond_drop_half(command)
            }
            (
                Self::TradeResponse { .. } | Self::TradeOwner { .. },
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
    Propose {
        scope: TradeScope,
        offer: PublicTradeOffer,
    },
    Respond(TradeResponseCommand),
    Commit {
        offer_id: TradeOfferId,
    },
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeResponseCommand {
    Accept { offer_id: TradeOfferId },
    Reject,
    Counter { offer: PlayerTrade },
}
