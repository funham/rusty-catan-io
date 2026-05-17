use crate::gameplay::game::{
    decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
    event::{EventBatch, GameEvent},
    input::{GameInput, PlayerCommand},
    lifecycle::EngineCore,
    output::CommandRejectionReason,
    phase::GamePhase,
    run::GameResult,
};
use crate::gameplay::{
    game::command::RegularCommand,
    game::{index::GameIndex, query::GameQuery},
    primitives::{
        PortKind, Tile,
        build::{Build, Establishment},
        resource::ResourceCollection,
        trade::{BankTrade, BankTradeKind},
    },
};
use crate::{
    algorithm,
    math::dice::{DiceOutcome, DiceRoll},
};
use rand::{SeedableRng, rngs::SmallRng};

#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionContext {
    pub max_turns: Option<u64>,
    pub dice_roll: Option<DiceRoll>,
    pub stolen_resource: Option<crate::gameplay::primitives::resource::Resource>,
}

pub fn decide(lifecycle: &EngineCore, input: GameInput) -> EventBatch {
    decide_with_context(lifecycle, input, DecisionContext::default())
}

pub fn decide_with_context(
    lifecycle: &EngineCore,
    input: GameInput,
    context: DecisionContext,
) -> EventBatch {
    match input {
        GameInput::Start => decide_start(lifecycle),
        GameInput::Submit {
            player_id,
            decision_id,
            command,
        } => decide_submit(lifecycle, player_id, decision_id, command, context),
    }
}

fn decide_start(lifecycle: &EngineCore) -> EventBatch {
    let mut events = EventBatch::new();
    let Some(active) = lifecycle.as_active() else {
        return events;
    };
    if active.phase != GamePhase::NotStarted {
        return events;
    }

    events.push(GameEvent::GameStarted);
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id: active.game.turn.get_turn_index(),
        kind: DecisionKind::InitPlacement,
        lifetime: DecisionLifetime::OneShot,
    }));
    events
}

fn decide_submit(
    lifecycle: &EngineCore,
    player_id: crate::gameplay::primitives::player::PlayerId,
    decision_id: DecisionId,
    command: PlayerCommand,
    context: DecisionContext,
) -> EventBatch {
    let mut events = EventBatch::new();
    let Some(active) = lifecycle.as_active() else {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision_id),
            reason: CommandRejectionReason::GameEnded,
            counts_toward_limit: false,
        });
        return events;
    };

    let Some(decision) = active.pending.get(decision_id).cloned() else {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision_id),
            reason: CommandRejectionReason::StaleDecision,
            counts_toward_limit: false,
        });
        return events;
    };

    if decision.player_id != player_id {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision_id),
            reason: CommandRejectionReason::WrongPlayer {
                expected: decision.player_id,
            },
            counts_toward_limit: false,
        });
        return events;
    }

    match (decision.kind, command) {
        (DecisionKind::InitPlacement, PlayerCommand::InitialPlacement(command)) => {
            decide_initial_placement(active, decision, command, context, &mut events);
        }
        (DecisionKind::RegularCommand, PlayerCommand::Regular(RegularCommand::EndMove)) => {
            decide_end_move(active, decision, context, &mut events);
        }
        (
            DecisionKind::RegularCommand,
            PlayerCommand::Regular(RegularCommand::TradeWithBank(trade)),
        ) => {
            decide_bank_trade(active, decision, trade, &mut events);
        }
        (DecisionKind::RegularCommand, PlayerCommand::Regular(RegularCommand::Build(build))) => {
            decide_build(active, decision, build, &mut events);
        }
        (DecisionKind::RegularCommand, PlayerCommand::Regular(RegularCommand::BuyDevCard)) => {
            decide_buy_dev_card(active, decision, &mut events);
        }
        (
            DecisionKind::InitCommand,
            PlayerCommand::InitCommand(crate::gameplay::game::command::InitCommand::RollDice),
        ) => {
            decide_roll_dice(
                active,
                decision,
                DecisionKind::PostDiceCommand,
                context,
                &mut events,
            );
        }
        (
            DecisionKind::PostDevCardCommand,
            PlayerCommand::PostDevCard(
                crate::gameplay::game::command::PostDevCardCommand::RollDice,
            ),
        ) => {
            decide_roll_dice(
                active,
                decision,
                DecisionKind::RegularCommand,
                context,
                &mut events,
            );
        }
        (
            DecisionKind::InitCommand,
            PlayerCommand::InitCommand(crate::gameplay::game::command::InitCommand::UseDevCard(
                usage,
            )),
        ) => {
            decide_use_dev_card(
                active,
                decision,
                usage,
                DecisionKind::PostDevCardCommand,
                context,
                &mut events,
            );
        }
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(crate::gameplay::game::command::PostDiceCommand::UseDevCard(
                usage,
            )),
        ) => {
            decide_use_dev_card(
                active,
                decision,
                usage,
                DecisionKind::RegularCommand,
                context,
                &mut events,
            );
        }
        _ => {}
    }

    events
}

fn decide_use_dev_card(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    usage: crate::gameplay::primitives::dev_card::DevCardUsage,
    next_kind: DecisionKind,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let mut candidate = active.game.clone();
    let mut rng = SmallRng::seed_from_u64(0);
    if candidate
        .use_dev_card_with_rng(usage, player_id, &mut rng)
        .is_err()
    {
        return;
    }
    let candidate_index = GameIndex::rebuild(&candidate);
    if GameQuery::new(&candidate, &candidate_index)
        .check_win_condition()
        .is_some()
    {
        return;
    }

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::DevCardUsed { player_id, usage });
    if let crate::gameplay::primitives::dev_card::DevCardUsage::Knight { rob_hex, robbed_id } =
        usage
    {
        events.push(GameEvent::RobberMoved {
            player_id,
            hex: rob_hex,
            robbed_id,
        });
        if let (Some(robbed_id), Some(resource)) = (robbed_id, context.stolen_resource) {
            events.push(GameEvent::ResourceStolen {
                player_id,
                robbed_id,
                resource,
            });
        }
    }
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id,
        kind: next_kind,
        lifetime: DecisionLifetime::OneShot,
    }));
}

fn decide_buy_dev_card(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let Some(card) = active.game.bank.dev_cards.last().copied() else {
        return;
    };
    if !active
        .game
        .players
        .get(player_id)
        .resources()
        .has_enough(&crate::gameplay::constants::costs::DEV_CARD)
    {
        return;
    }

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::DevCardBought { player_id });
    events.push(GameEvent::DevCardDrawn { player_id, card });
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    }));
}

fn decide_roll_dice(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    next_kind: DecisionKind,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let Some(roll) = context.dice_roll else {
        return;
    };
    let player_id = decision.player_id;
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::DiceRolled {
        player_id,
        value: roll,
    });
    match roll.resolve() {
        DiceOutcome::Harvest(num) => {
            events.push(GameEvent::ResourcesDistributed {
                by_player: algorithm::resource_distribution_for_roll(&active.game, player_id, num),
            });
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id,
                kind: next_kind,
                lifetime: DecisionLifetime::OneShot,
            }));
        }
        DiceOutcome::Seven => {
            open_next_discard_or_robber(active, player_id, events);
        }
    }
}

fn open_next_discard_or_robber(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    robber_player: crate::gameplay::primitives::player::PlayerId,
    events: &mut EventBatch,
) {
    let first_discard = active.pending_discards.first().copied().or_else(|| {
        algorithm::player_order_from(robber_player, active.game.players.count())
            .find(|pid| active.game.players.get(*pid).resources().total() > 7)
    });
    if let Some(player_id) = first_discard {
        let required = active.game.players.get(player_id).resources().total() / 2;
        events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id,
            kind: DecisionKind::DropHalf { required },
            lifetime: DecisionLifetime::OneShot,
        }));
    } else {
        events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id: robber_player,
            kind: DecisionKind::MoveRobber,
            lifetime: DecisionLifetime::OneShot,
        }));
    }
}

fn decide_build(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    build: Build,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let mut candidate = active.game.clone();
    if candidate.build(player_id, build).is_err() {
        return;
    }
    let candidate_index = GameIndex::rebuild(&candidate);
    if GameQuery::new(&candidate, &candidate_index)
        .check_win_condition()
        .is_some()
    {
        return;
    }

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::Built { player_id, build });
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    }));
}

fn decide_bank_trade(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    trade: BankTrade,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    if !can_trade_with_bank(active, player_id, trade) {
        return;
    }

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::BankTradeCompleted { player_id, trade });
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    }));
}

fn can_trade_with_bank(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    player_id: crate::gameplay::primitives::player::PlayerId,
    trade: BankTrade,
) -> bool {
    let required_port = match trade.kind {
        BankTradeKind::BankGeneric => None,
        BankTradeKind::PortGeneric => Some(PortKind::Universal),
        BankTradeKind::PortSpecific => Some(PortKind::Special(trade.give)),
    };
    if let Some(required_port) = required_port
        && !active.index.ports_acquired[player_id.index()].contains(&required_port)
    {
        return false;
    }

    active
        .game
        .players
        .get(player_id)
        .resources()
        .has_enough(&trade.to_bank())
        && active.game.bank.can_pay(&trade.from_bank())
}

fn decide_end_move(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let turn_no = active.game.turn.get_turns_played();
    let mut next_turn = active.game.turn.clone();
    next_turn.next();
    let next_turn_no = next_turn.get_turns_played();

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::TurnEnded { player_id, turn_no });
    if let Some(max_turns) = context.max_turns
        && next_turn_no >= max_turns
    {
        events.push(GameEvent::GameFinished {
            result: GameResult::LimitReached {
                turns: next_turn_no,
            },
            stats: None,
        });
        return;
    }
    events.push(GameEvent::TurnStarted {
        player_id: next_turn.get_turn_index(),
        turn_no: next_turn_no,
    });
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id: next_turn.get_turn_index(),
        kind: DecisionKind::InitCommand,
        lifetime: DecisionLifetime::OneShot,
    }));
}

fn decide_initial_placement(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    command: crate::gameplay::game::command::InitialPlacementCommand,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let (settlement, road) = command.as_builds();

    if let Some(init) = &active.init {
        let mut candidate = init.clone();
        if let Err(err) = candidate.builds.try_init_place(player_id, road, settlement) {
            events.push(GameEvent::CommandRejected {
                player_id,
                decision_id: Some(decision.id),
                reason: CommandRejectionReason::IllegalCommand(format!(
                    "invalid initial placement: {err:?}"
                )),
                counts_toward_limit: true,
            });
            return;
        }

        events.push(GameEvent::DecisionClosed {
            decision_id: decision.id,
        });
        events.push(GameEvent::InitialPlacementBuilt {
            player_id,
            settlement: settlement.vtx,
            road,
        });
        if init.turn.get_rounds_played() == 1 {
            let resources = initial_resources(init, settlement);
            if resources != ResourceCollection::ZERO {
                events.push(GameEvent::InitialResourcesGranted {
                    player_id,
                    resources,
                });
            }
        }

        candidate.turn.next();
        if candidate.turn.get_rounds_played() < 2 {
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id: candidate.turn.get_turn_index(),
                kind: DecisionKind::InitPlacement,
                lifetime: DecisionLifetime::OneShot,
            }));
        } else {
            let regular_turn = candidate.turn.into_regular();
            let turn_no = regular_turn.get_turns_played();
            if let Some(max_turns) = context.max_turns
                && turn_no >= max_turns
            {
                events.push(GameEvent::GameFinished {
                    result: GameResult::LimitReached { turns: turn_no },
                    stats: None,
                });
                return;
            }
            events.push(GameEvent::TurnStarted {
                player_id: regular_turn.get_turn_index(),
                turn_no,
            });
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id: active.game.turn.get_turn_index(),
                kind: DecisionKind::InitCommand,
                lifetime: DecisionLifetime::OneShot,
            }));
        }
        return;
    }

    let mut builds = active.game.builds.clone();
    if let Err(err) = builds.try_init_place(player_id, road, settlement) {
        events.push(GameEvent::CommandRejected {
            player_id,
            decision_id: Some(decision.id),
            reason: CommandRejectionReason::IllegalCommand(format!(
                "invalid initial placement: {err:?}"
            )),
            counts_toward_limit: true,
        });
        return;
    }
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::InitialPlacementBuilt {
        player_id,
        settlement: settlement.vtx,
        road,
    });
}

fn initial_resources(
    init: &crate::gameplay::game::init::GameInitializationState,
    settlement: Establishment,
) -> ResourceCollection {
    let mut resources = ResourceCollection::ZERO;
    for hex in settlement
        .vtx
        .as_set()
        .into_iter()
        .filter(|hex| hex.norm() <= init.board.arrangement.radius() as usize)
    {
        if let Tile::Resource { resource, .. } = init.board.arrangement[hex] {
            resources += &resource.into();
        }
    }
    resources
}
