use catan_core::{
    gameplay::{
        game::{
            command::{
                ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
                MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
            },
            decision::{DecisionKind, OpenDecision},
            input::PlayerCommand,
            view::PlayerDecisionContext,
        },
        primitives::{player::PlayerId, resource::ResourceSet},
    },
    topology::Hex,
};

use crate::{
    bot::{BotPolicy, unsupported_decision_command},
    legal,
};

#[derive(Debug, Default)]
pub struct LazyAgent {
    id: PlayerId,
}

impl LazyAgent {
    pub fn new(id: impl Into<PlayerId>) -> Self {
        let id = id.into();
        Self { id }
    }
}

impl LazyAgent {
    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitialPlacementCommand {
        lazy_init_stage_action(context, self.id)
    }

    fn init_action(&mut self, _context: PlayerDecisionContext<'_>) -> InitCommand {
        InitCommand::RollDice
    }

    fn after_dice_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDiceCommand {
        PostDiceCommand::RegularCommand(RegularCommand::EndMove)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardCommand {
        PostDevCardCommand::RollDice
    }

    fn regular_action(&mut self, _context: PlayerDecisionContext<'_>) -> RegularCommand {
        RegularCommand::EndMove
    }

    fn move_robber(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
        lazy_move_robber(context)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChooseRobbedPlayerCommand {
        lazy_choose_player_to_rob(context, robber_pos)
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfCommand {
        lazy_drop_half(context)
    }
}

impl BotPolicy for LazyAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn command_for(
        &mut self,
        decision: &OpenDecision,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match decision.kind {
            DecisionKind::InitialPlacement => Some(PlayerCommand::InitialPlacement(
                self.init_stage_action(context),
            )),
            DecisionKind::InitCommand => {
                Some(PlayerCommand::InitCommand(self.init_action(context)))
            }
            DecisionKind::PostDiceCommand => {
                Some(PlayerCommand::PostDice(self.after_dice_action(context)))
            }
            DecisionKind::PostDevCardCommand => Some(PlayerCommand::PostDevCard(
                self.after_dev_card_action(context),
            )),
            DecisionKind::RegularCommand => {
                Some(PlayerCommand::Regular(self.regular_action(context)))
            }
            DecisionKind::MoveRobber => Some(PlayerCommand::MoveRobber(self.move_robber(context))),
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(
                PlayerCommand::ChooseRobbedPlayer(self.choose_player_to_rob(context, robber_pos)),
            ),
            DecisionKind::DropHalf { .. } => Some(PlayerCommand::DropHalf(self.drop_half(context))),
            DecisionKind::TradeResponse { .. } | DecisionKind::TradeOwnerAction { .. } => {
                unsupported_decision_command(decision.kind)
            }
        }
    }
}

pub fn lazy_drop_half(context: PlayerDecisionContext<'_>) -> DropHalfCommand {
    let number_to_drop = context.private.resources.total() / 2;
    let mut to_drop = ResourceSet::default();
    for (resource, number) in context.private.resources.unroll() {
        let remaining = number_to_drop - to_drop.total();

        if remaining == 0 {
            break;
        }

        to_drop[resource] = remaining.min(number);
    }

    DropHalfCommand(to_drop)
}

pub fn lazy_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
) -> ChooseRobbedPlayerCommand {
    let id = legal::legal_rob_targets(&context, robber_pos)
        .into_iter()
        .next()
        .expect("engine must forbid this case");
    ChooseRobbedPlayerCommand(id)
}

pub fn lazy_move_robber(context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
    for hex in context.public.board.arrangement.hex_iter() {
        if hex != context.public.board_state.robber_pos {
            return MoveRobberCommand(hex);
        }
    }

    unreachable!("there must be a hex without the robber on it")
}

pub fn lazy_init_stage_action(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> InitialPlacementCommand {
    context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, player_id)
        .into_iter()
        .next()
        .expect("there must be an initial placement")
}
