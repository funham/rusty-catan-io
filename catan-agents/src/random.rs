use crate::{lazy, legal};
use catan_core::{
    agent::{
        action::{
            ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
        agent::PlayerRuntime,
    },
    gameplay::{
        game::{event::PlayerNotification, view::PlayerDecisionContext},
        primitives::{player::PlayerId, resource::ResourceCollection},
    },
    topology::{Hex, HexIndex},
};
use rand::{
    RngExt,
    rngs::ThreadRng,
    seq::{IndexedRandom, IteratorRandom},
};

#[derive(Debug, Default)]
pub struct RandomAgent {
    id: PlayerId,
    rng: ThreadRng,
}

impl RandomAgent {
    pub fn new(id: PlayerId) -> Self {
        Self {
            id,
            rng: rand::rng(),
        }
    }
}

impl PlayerNotification for RandomAgent {}

impl PlayerRuntime for RandomAgent {
    fn player_id(&self) -> PlayerId {
        self.id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        rand_init_stage_action(context, &mut self.rng)
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        rand_init_action(context, &mut self.rng)
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        rand_after_dice_action(context, &mut self.rng)
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        rand_regular_action(context, &mut self.rng)
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        rand_move_robbers(context, &mut self.rng)
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        rand_choose_player_to_rob(context, robber_pos, &mut self.rng)
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Decline
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        rand_drop_half(context, &mut self.rng)
    }
}

pub fn rand_init_stage_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut ThreadRng,
) -> InitStageAction {
    let (establishment, road) = context
        .public
        .builds
        .query()
        .possible_initial_placements(context.public.board, context.actor)
        .choose(rng)
        .unwrap()
        .clone();

    InitStageAction {
        establishment_position: establishment.pos,
        road,
    }
}

pub fn rand_init_action(context: PlayerDecisionContext<'_>, rng: &mut ThreadRng) -> InitAction {
    if let usages = legal::legal_dev_card_usages(&context, context.actor)
        && !usages.is_empty()
        && rng.random_bool(0.8)
    {
        return InitAction::UseDevCard(*usages.choose(rng).unwrap());
    }

    InitAction::RollDice
}

pub fn rand_after_dice_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut ThreadRng,
) -> PostDiceAction {
    if let usages = legal::legal_dev_card_usages(&context, context.actor)
        && !usages.is_empty()
        && rng.random_bool(0.8)
    {
        return PostDiceAction::UseDevCard(*usages.choose(rng).unwrap());
    }

    PostDiceAction::RegularAction(rand_regular_action(context, rng))
}

pub fn rand_regular_action(
    context: PlayerDecisionContext<'_>,
    rng: &mut ThreadRng,
) -> RegularAction {
    legal::legal_regular_action(&context)
        .choose(rng)
        .unwrap()
        .clone()
}

pub fn rand_move_robbers(
    context: PlayerDecisionContext<'_>,
    rng: &mut ThreadRng,
) -> MoveRobbersAction {
    let n = context.public.board.arrangement.len();
    let tile_index = match rng.random_range(0..n - 1) {
        index if index == context.public.board_state.robber_pos.index().to_spiral() => n - 1,
        index => index,
    };

    MoveRobbersAction(HexIndex::spiral_to_hex(tile_index))
}

pub fn rand_choose_player_to_rob(
    context: PlayerDecisionContext<'_>,
    robber_pos: Hex,
    rng: &mut ThreadRng,
) -> ChoosePlayerToRobAction {
    let id = context
        .public
        .players_on_hex(robber_pos)
        .into_iter()
        .filter(|id| *id != context.actor)
        .choose(rng)
        .expect("controller must forbid this case");

    ChoosePlayerToRobAction(id)
}

pub fn rand_answer_trade(_context: PlayerDecisionContext<'_>) -> TradeAnswer {
    TradeAnswer::Decline
}

pub fn rand_drop_half(context: PlayerDecisionContext<'_>, _rng: &mut ThreadRng) -> DropHalfAction {
    let number_to_drop = context.private.resources.total() / 2;

    match context.search {
        Some(search) => {
            let mut to_drop = ResourceCollection::default();
            let search = search.make_owned();
            let mut res = search
                .state
                .players
                .get(search.root_player)
                .resources()
                .clone();

            for _ in 0..number_to_drop {
                let card = res
                    .pop_random()
                    .expect(&format!("must contain {} cards", number_to_drop));
                to_drop[card] += 1;
            }

            DropHalfAction(to_drop)
        }
        None => {
            log::error!("couldn't find search context for random agent");
            lazy::lazy_drop_half(context)
        }
    }
}
