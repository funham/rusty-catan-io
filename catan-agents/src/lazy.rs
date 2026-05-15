use catan_core::{
    gameplay::{
        game::{
            action::{
                ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
                MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction,
            },
            decision::{DecisionKind, OpenDecision},
            input::PlayerCommand,
            view::PlayerDecisionContext,
        },
        primitives::{player::PlayerId, resource::ResourceCollection},
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
    pub fn new(id: PlayerId) -> Self {
        Self { id }
    }
}

impl LazyAgent {
    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        lazy_init_stage_action(context, self.id)
    }

    fn init_action(&mut self, _context: PlayerDecisionContext<'_>) -> InitAction {
        InitAction::RollDice
    }

    fn after_dice_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDiceAction {
        PostDiceAction::RegularAction(RegularAction::EndMove)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, _context: PlayerDecisionContext<'_>) -> RegularAction {
        RegularAction::EndMove
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        lazy_move_robbers(context)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        lazy_choose_player_to_rob(context, robber_pos)
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
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
            DecisionKind::InitPlacement => Some(PlayerCommand::InitialPlacement(
                self.init_stage_action(context),
            )),
            DecisionKind::InitAction => Some(PlayerCommand::InitAction(self.init_action(context))),
            DecisionKind::PostDiceAction => {
                Some(PlayerCommand::PostDice(self.after_dice_action(context)))
            }
            DecisionKind::PostDevCardAction => Some(PlayerCommand::PostDevCard(
                self.after_dev_card_action(context),
            )),
            DecisionKind::RegularAction => {
                Some(PlayerCommand::Regular(self.regular_action(context)))
            }
            DecisionKind::MoveRobber => {
                Some(PlayerCommand::MoveRobbers(self.move_robbers(context)))
            }
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

pub fn lazy_drop_half(context: PlayerDecisionContext<'_>) -> DropHalfAction {
    let number_to_drop = context.private.resources.total() / 2;
    let mut to_drop = ResourceCollection::default();
    for (resource, number) in context.private.resources.unroll() {
        let remaining = number_to_drop - to_drop.total();

        if remaining == 0 {
            break;
        }

        to_drop[resource] = remaining.min(number);
    }

    DropHalfAction(to_drop)
}

pub fn lazy_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
) -> ChoosePlayerToRobAction {
    let id = legal::legal_rob_targets(&context, robber_pos)
        .into_iter()
        .next()
        .expect("engine must forbid this case");
    ChoosePlayerToRobAction(id)
}

pub fn lazy_move_robbers(context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
    for hex in context.public.board.arrangement.hex_iter() {
        if hex != context.public.board_state.robber_pos {
            return MoveRobbersAction(hex);
        }
    }

    unreachable!("there must be a hex without the robber on it")
}

pub fn lazy_init_stage_action(
    context: PlayerDecisionContext<'_>,
    player_id: PlayerId,
) -> InitStageAction {
    context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, player_id)
        .into_iter()
        .next()
        .expect("there must be an initial placement")
}
