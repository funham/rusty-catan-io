use crate::gameplay::{
    field::state::BoardLayout,
    game::{
        engine::lifecycle::{PlayingEngine, SetupEngine},
        index::GameIndex,
        query::GameQuery,
    },
    primitives::{
        PlayerId, PortKind, Tile,
        build::{Build, Establishment},
        dev_card::{DevCardUsage, UsableDevCard},
        player::player_ids,
        resource::ResourceSet,
        trade::{BankTrade, BankTradeKind, PlayerTrade},
    },
};
use crate::{
    DecisionResponse,
    gameplay::game::{
        command::{
            self, ChooseRobbedPlayerCommand, DiscardHalfCommand, InitCommand, MoveRobberCommand,
            PostDevCardCommand, PostDiceCommand, RegularCommand,
        },
        decision::{
            DecisionAllocator, DecisionKind, DecisionLifetime, OpenDecision, PendingDecisions,
        },
        event::{EventBatch, GameEndPlayerStats, GameEndStats, GameEvent},
        input::{DecisionToken, GameInput, PlayerCommand, TradeCommand, TradeResponseCommand},
        output::CommandRejectionReason,
        run::GameResult,
    },
};
use crate::{
    algorithm,
    math::dice::{DiceOutcome, DiceRoll},
};

use super::lifecycle::EngineState;

#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionContext {
    pub max_turns: Option<u64>,
    pub max_invalid_actions: Option<u64>,
    pub dice_roll: Option<DiceRoll>,
    pub stolen_resource: Option<crate::gameplay::primitives::resource::Resource>,
}

pub fn decide(lifecycle: &EngineState, input: GameInput) -> EventBatch {
    decide_with_context(lifecycle, input, DecisionContext::default())
}

pub fn decide_with_context(
    lifecycle: &EngineState,
    input: GameInput,
    context: DecisionContext,
) -> EventBatch {
    match input {
        GameInput::Start => decide_start(lifecycle),
        GameInput::Submit(response) => decide_submit(lifecycle, response, context),
    }
}

fn decide_start(lifecycle: &EngineState) -> EventBatch {
    let mut events = EventBatch::new();
    let EngineState::Unstarted(unstarted) = lifecycle else {
        return events;
    };

    events.push(GameEvent::GameStarted);
    let mut decisions = unstarted.decisions();
    open_decision(
        &mut events,
        &mut decisions,
        unstarted.setup_turn.get_turn_index(),
        DecisionKind::InitialPlacement,
        DecisionLifetime::OneShot,
    );
    events
}

fn decide_submit(
    lifecycle: &EngineState,
    response: DecisionResponse,
    context: DecisionContext,
) -> EventBatch {
    match lifecycle {
        EngineState::Setup(setup) => decide_setup_submit(setup, response, context),
        EngineState::Playing(active) => decide_playing_submit(active, response, context),
        EngineState::Finished(_) => {
            reject_submit(response.token, CommandRejectionReason::GameEnded, false)
        }
        EngineState::Unstarted(_) => {
            reject_submit(response.token, CommandRejectionReason::StaleDecision, false)
        }
    }
}

fn decide_setup_submit(
    active: &SetupEngine,
    response: DecisionResponse,
    context: DecisionContext,
) -> EventBatch {
    let mut events = EventBatch::new();
    let DecisionResponse { token, command } = response;
    let decision = match resolve_submission(&active.pending, token) {
        Ok(decision) => decision,
        Err(reason) => {
            reject(&mut events, token, reason, false);
            return events;
        }
    };

    match command_for_decision(decision.kind, command) {
        Some(DecisionCommand::InitialPlacement(command)) => {
            decide_initial_placement(active, decision, command, context, &mut events);
        }
        _ => {
            reject(
                &mut events,
                DecisionToken::from(&decision),
                CommandRejectionReason::WrongPhase,
                false,
            );
        }
    }
    events
}

fn decide_playing_submit(
    active: &PlayingEngine,
    response: DecisionResponse,
    context: DecisionContext,
) -> EventBatch {
    let mut events = EventBatch::new();
    let DecisionResponse { token, command } = response;
    let decision = match resolve_submission(&active.pending, token) {
        Ok(decision) => decision,
        Err(reason) => {
            reject(&mut events, token, reason, false);
            return events;
        }
    };

    match command_for_decision(decision.kind, command) {
        Some(DecisionCommand::InitialPlacement(_)) => {
            reject(
                &mut events,
                DecisionToken::from(&decision),
                CommandRejectionReason::WrongPhase,
                false,
            );
        }
        Some(DecisionCommand::Regular(command)) => {
            decide_regular_command(active, decision, command, context, &mut events);
        }
        Some(DecisionCommand::OpenTrade { trade }) => {
            decide_open_trade(active, decision, trade, &mut events);
        }
        Some(DecisionCommand::RollDice { next_kind }) => {
            decide_roll_dice(active, decision, next_kind, context, &mut events);
        }
        Some(DecisionCommand::UseDevCard { usage, next_kind }) => {
            decide_use_dev_card(active, decision, usage, next_kind, context, &mut events);
        }
        Some(DecisionCommand::DiscardHalf {
            required,
            resources,
        }) => {
            decide_discard_half(active, decision, required, resources, &mut events);
        }
        Some(DecisionCommand::MoveRobber(hex)) => {
            decide_move_robber(active, decision, hex, context, &mut events);
        }
        Some(DecisionCommand::ChooseRobbedPlayer {
            robber_pos,
            robbed_id,
        }) => {
            decide_choose_robbed_player(
                active,
                decision,
                robber_pos,
                robbed_id,
                context,
                &mut events,
            );
        }
        Some(DecisionCommand::TradeResponse { session, command }) => {
            decide_trade_response(active, decision, session, command, &mut events);
        }
        Some(DecisionCommand::TradeOwner { session, command }) => {
            decide_trade_owner(active, decision, session, command, &mut events);
        }
        None => {
            reject(
                &mut events,
                DecisionToken::from(&decision),
                CommandRejectionReason::WrongPhase,
                false,
            );
        }
    }

    finish_after_invalid_action_limit(active, context, &mut events);
    events
}

fn reject_submit(
    token: DecisionToken,
    reason: CommandRejectionReason,
    counts_toward_limit: bool,
) -> EventBatch {
    let mut events = EventBatch::new();
    reject(&mut events, token, reason, counts_toward_limit);
    events
}

fn resolve_submission(
    pending: &PendingDecisions,
    token: DecisionToken,
) -> Result<OpenDecision, CommandRejectionReason> {
    let Some(decision) = pending.get(token.id).cloned() else {
        return Err(CommandRejectionReason::StaleDecision);
    };
    if decision.player_id != token.player_id {
        return Err(CommandRejectionReason::WrongPlayer {
            expected: decision.player_id,
        });
    }
    Ok(decision)
}

enum DecisionCommand {
    InitialPlacement(crate::gameplay::game::command::InitialPlacementCommand),
    Regular(RegularCommand),
    OpenTrade {
        trade: PlayerTrade,
    },
    RollDice {
        next_kind: DecisionKind,
    },
    UseDevCard {
        usage: DevCardUsage,
        next_kind: DecisionKind,
    },
    DiscardHalf {
        required: u16,
        resources: ResourceSet,
    },
    MoveRobber(crate::topology::Hex),
    ChooseRobbedPlayer {
        robber_pos: crate::topology::Hex,
        robbed_id: PlayerId,
    },
    TradeResponse {
        session: crate::gameplay::game::trade::TradeSessionId,
        command: TradeCommand,
    },
    TradeOwner {
        session: crate::gameplay::game::trade::TradeSessionId,
        command: TradeCommand,
    },
}

fn command_for_decision(kind: DecisionKind, command: PlayerCommand) -> Option<DecisionCommand> {
    match (kind, command) {
        (DecisionKind::InitialPlacement, PlayerCommand::InitialPlacement(command)) => {
            Some(DecisionCommand::InitialPlacement(command))
        }
        (DecisionKind::RegularCommand, PlayerCommand::Regular(command))
        | (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(PostDiceCommand::RegularCommand(command)),
        ) => Some(DecisionCommand::Regular(command)),
        (DecisionKind::RegularCommand, PlayerCommand::Trade(TradeCommand::Propose { offer })) => {
            Some(DecisionCommand::OpenTrade { trade: offer })
        }
        (DecisionKind::InitCommand, PlayerCommand::InitCommand(InitCommand::RollDice)) => {
            Some(DecisionCommand::RollDice {
                next_kind: DecisionKind::PostDiceCommand,
            })
        }
        (
            DecisionKind::PostDevCardCommand,
            PlayerCommand::PostDevCard(PostDevCardCommand::RollDice),
        ) => Some(DecisionCommand::RollDice {
            next_kind: DecisionKind::RegularCommand,
        }),
        (DecisionKind::InitCommand, PlayerCommand::InitCommand(InitCommand::UseDevCard(usage))) => {
            Some(DecisionCommand::UseDevCard {
                usage,
                next_kind: DecisionKind::PostDevCardCommand,
            })
        }
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(PostDiceCommand::UseDevCard(usage)),
        ) => Some(DecisionCommand::UseDevCard {
            usage,
            next_kind: DecisionKind::RegularCommand,
        }),
        (
            DecisionKind::DiscardHalf { required },
            PlayerCommand::DiscardHalf(DiscardHalfCommand(resources)),
        ) => Some(DecisionCommand::DiscardHalf {
            required,
            resources,
        }),
        (DecisionKind::MoveRobber, PlayerCommand::MoveRobber(MoveRobberCommand(hex))) => {
            Some(DecisionCommand::MoveRobber(hex))
        }
        (
            DecisionKind::ChooseRobbedPlayer { robber_pos },
            PlayerCommand::ChooseRobbedPlayer(ChooseRobbedPlayerCommand(robbed_id)),
        ) => Some(DecisionCommand::ChooseRobbedPlayer {
            robber_pos,
            robbed_id,
        }),
        (DecisionKind::TradeResponse { session }, PlayerCommand::Trade(command)) => {
            Some(DecisionCommand::TradeResponse { session, command })
        }
        (DecisionKind::TradeOwnerAction { session }, PlayerCommand::Trade(command)) => {
            Some(DecisionCommand::TradeOwner { session, command })
        }
        _ => None,
    }
}

fn decide_regular_command(
    active: &PlayingEngine,
    decision: OpenDecision,
    command: RegularCommand,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    match command {
        RegularCommand::EndMove => decide_end_move(active, decision, context, events),
        RegularCommand::Build(build) => decide_build(active, decision, build, events),
        RegularCommand::BuyDevCard => decide_buy_dev_card(active, decision, events),
        RegularCommand::UseDevCard(usage) => decide_use_dev_card(
            active,
            decision,
            usage,
            DecisionKind::RegularCommand,
            context,
            events,
        ),
        RegularCommand::OfferTrade(offer) => decide_open_trade(active, decision, offer, events),
        RegularCommand::TradeWithBank(trade) => decide_bank_trade(active, decision, trade, events),
    }
}

fn finish_after_invalid_action_limit(
    active: &PlayingEngine,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let Some(limit) = context.max_invalid_actions else {
        return;
    };
    let rejected_actions = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GameEvent::CommandRejected {
                    counts_toward_limit: true,
                    ..
                }
            )
        })
        .count() as u64;
    if rejected_actions == 0
        || active.invalid_actions + rejected_actions < limit
        || events
            .iter()
            .any(|event| matches!(event, GameEvent::GameFinished { .. }))
    {
        return;
    }
    events.push(GameEvent::GameFinished {
        result: GameResult::Interrupted {
            reason: format!("too many invalid actions ({limit})"),
        },
        stats: None,
    });
}

fn reject(
    events: &mut EventBatch,
    token: DecisionToken,
    reason: CommandRejectionReason,
    counts_toward_limit: bool,
) {
    events.push(GameEvent::CommandRejected {
        token,
        reason,
        counts_toward_limit,
    });
}

fn reject_illegal(events: &mut EventBatch, decision: &OpenDecision, reason: impl Into<String>) {
    reject(
        events,
        DecisionToken::from(decision),
        CommandRejectionReason::IllegalCommand(reason.into()),
        true,
    );
}

fn open_decision(
    events: &mut EventBatch,
    decisions: &mut DecisionAllocator,
    player_id: PlayerId,
    kind: DecisionKind,
    lifetime: DecisionLifetime,
) {
    events.push(GameEvent::DecisionOpened(
        decisions.open(player_id, kind, lifetime),
    ));
}

fn decide_open_trade(
    active: &PlayingEngine,
    decision: OpenDecision,
    trade: PlayerTrade,
    events: &mut EventBatch,
) {
    if decision.player_id != active.game.turn.get_turn_index() {
        reject_illegal(events, &decision, "only the active player can offer trades");
        return;
    }
    if crate::gameplay::game::trade::trade_has_overlapping_resources(&trade)
        || !active
            .game
            .players
            .get(decision.player_id)
            .resources()
            .has_enough(&trade.give)
    {
        reject_illegal(events, &decision, "invalid player trade proposal");
        return;
    }
    let session_id =
        crate::gameplay::game::trade::TradeSessionId(active.trade_sessions.len() as u64);
    let offer_id = crate::gameplay::game::trade::TradeOfferId(0);
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::TradeOpened {
        session_id,
        proposer_id: decision.player_id,
        offer_id,
        offer: trade,
    });
    let mut decisions = active.decisions();
    for player_id in player_ids(active.game.players.count()) {
        if player_id != decision.player_id {
            open_decision(
                events,
                &mut decisions,
                player_id,
                DecisionKind::TradeResponse {
                    session: session_id,
                },
                DecisionLifetime::UntilSessionClosed(session_id),
            );
        }
    }
    open_decision(
        events,
        &mut decisions,
        decision.player_id,
        DecisionKind::TradeOwnerAction {
            session: session_id,
        },
        DecisionLifetime::UntilSessionClosed(session_id),
    );
}

fn decide_trade_response(
    active: &PlayingEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    command: TradeCommand,
    events: &mut EventBatch,
) {
    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    if !session.open || decision.player_id == session.proposer {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::WrongPhase,
            false,
        );
        return;
    }
    match command {
        TradeCommand::Respond(TradeResponseCommand::Accept { offer_id }) => {
            let Some(offer) = session.offer(offer_id) else {
                reject_illegal(events, &decision, "unknown trade offer");
                return;
            };
            if offer.proposer != session.proposer {
                reject_illegal(
                    events,
                    &decision,
                    "only the active player can confirm counter offers",
                );
                return;
            }
            if !crate::gameplay::game::trade::trade_is_funded(
                active.game.players.get(session.proposer).resources(),
                active.game.players.get(decision.player_id).resources(),
                &offer.trade,
            ) {
                reject_illegal(events, &decision, "trade resources are not available");
                return;
            }
            events.push(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: decision.player_id,
                response: crate::gameplay::game::trade::TradeResponseState::Accepted { offer_id },
            });
        }
        TradeCommand::Respond(TradeResponseCommand::Reject) => {
            events.push(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: decision.player_id,
                response: crate::gameplay::game::trade::TradeResponseState::Rejected,
            });
        }
        TradeCommand::Respond(TradeResponseCommand::Counter { offer }) => {
            if crate::gameplay::game::trade::trade_has_overlapping_resources(&offer)
                || !active
                    .game
                    .players
                    .get(decision.player_id)
                    .resources()
                    .has_enough(&offer.give)
            {
                reject_illegal(events, &decision, "invalid counter offer");
                return;
            }
            let offer_id = crate::gameplay::game::trade::TradeOfferId(session.offers.len() as u64);
            events.push(GameEvent::TradeOfferAdded {
                session_id,
                player_id: decision.player_id,
                offer_id,
                offer,
            });
            events.push(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: decision.player_id,
                response: crate::gameplay::game::trade::TradeResponseState::Countered { offer_id },
            });
        }
        _ => reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::WrongPhase,
            false,
        ),
    }
}

fn decide_trade_owner(
    active: &PlayingEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    command: TradeCommand,
    events: &mut EventBatch,
) {
    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    if !session.open || decision.player_id != session.proposer {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::WrongPhase,
            false,
        );
        return;
    }
    match command {
        TradeCommand::Commit { offer_id } => {
            decide_commit_trade(active, decision, session_id, offer_id, events);
        }
        TradeCommand::Reject { offer_id } => {
            decide_reject_counter_offer(active, decision, session_id, offer_id, events);
        }
        TradeCommand::Propose { offer } => {
            decide_add_prime_trade_offer(active, decision, session_id, offer, events);
        }
        TradeCommand::Cancel => {
            close_session_decisions(active, session_id, events);
            events.push(GameEvent::TradeCancelled {
                session_id,
                proposer_id: decision.player_id,
            });
            reopen_regular(active, decision.player_id, events);
        }
        _ => reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::WrongPhase,
            false,
        ),
    }
}

fn decide_add_prime_trade_offer(
    active: &PlayingEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    offer: PlayerTrade,
    events: &mut EventBatch,
) {
    if crate::gameplay::game::trade::trade_has_overlapping_resources(&offer)
        || !active
            .game
            .players
            .get(decision.player_id)
            .resources()
            .has_enough(&offer.give)
    {
        reject_illegal(events, &decision, "invalid trade offer");
        return;
    }

    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    let offer_id = crate::gameplay::game::trade::TradeOfferId(session.offers.len() as u64);
    close_session_decisions(active, session_id, events);
    events.push(GameEvent::TradeOfferAdded {
        session_id,
        player_id: decision.player_id,
        offer_id,
        offer,
    });
    let mut decisions = active.decisions();
    for player_id in player_ids(active.game.players.count()) {
        if player_id == decision.player_id {
            continue;
        }
        events.push(GameEvent::TradeResponseUpdated {
            session_id,
            player_id,
            response: crate::gameplay::game::trade::TradeResponseState::Waiting,
        });
        open_decision(
            events,
            &mut decisions,
            player_id,
            DecisionKind::TradeResponse {
                session: session_id,
            },
            DecisionLifetime::UntilSessionClosed(session_id),
        );
    }
    open_decision(
        events,
        &mut decisions,
        decision.player_id,
        DecisionKind::TradeOwnerAction {
            session: session_id,
        },
        DecisionLifetime::UntilSessionClosed(session_id),
    );
}

fn decide_commit_trade(
    active: &PlayingEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    offer_id: crate::gameplay::game::trade::TradeOfferId,
    events: &mut EventBatch,
) {
    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    let Some(offer) = session.offer(offer_id) else {
        reject_illegal(events, &decision, "unknown trade offer");
        return;
    };
    let peer_id = if offer.proposer == session.proposer {
        session.accepted_peer_for_offer(offer_id)
    } else {
        Some(offer.proposer)
    };
    let Some(peer_id) = peer_id else {
        reject_illegal(
            events,
            &decision,
            "no player has accepted the selected trade offer",
        );
        return;
    };
    let funded = if offer.proposer == session.proposer {
        crate::gameplay::game::trade::trade_is_funded(
            active.game.players.get(session.proposer).resources(),
            active.game.players.get(peer_id).resources(),
            &offer.trade,
        )
    } else {
        crate::gameplay::game::trade::trade_is_funded(
            active.game.players.get(peer_id).resources(),
            active.game.players.get(session.proposer).resources(),
            &offer.trade,
        )
    };
    if !funded {
        reject_illegal(events, &decision, "trade resources are no longer available");
        return;
    }
    close_session_decisions(active, session_id, events);
    events.push(GameEvent::TradeCompleted {
        session_id,
        proposer_id: session.proposer,
        peer_id,
        offer_id,
    });
    reopen_regular(active, session.proposer, events);
}

fn decide_reject_counter_offer(
    active: &PlayingEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    offer_id: crate::gameplay::game::trade::TradeOfferId,
    events: &mut EventBatch,
) {
    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            DecisionToken::from(&decision),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    let Some(offer) = session.offer(offer_id) else {
        reject_illegal(events, &decision, "unknown trade offer");
        return;
    };
    if offer.proposer == session.proposer {
        reject_illegal(events, &decision, "cancel the original offer instead");
        return;
    }

    events.push(GameEvent::TradeResponseUpdated {
        session_id,
        player_id: offer.proposer,
        response: crate::gameplay::game::trade::TradeResponseState::Rejected,
    });
    close_session_decisions(active, session_id, events);
    events.push(GameEvent::TradeCancelled {
        session_id,
        proposer_id: session.proposer,
    });
    reopen_regular(active, session.proposer, events);
}

fn close_session_decisions(
    active: &PlayingEngine,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    events: &mut EventBatch,
) {
    for decision in active
        .pending
        .iter()
        .filter(|decision| decision.lifetime == DecisionLifetime::UntilSessionClosed(session_id))
    {
        events.push(GameEvent::DecisionClosed {
            decision_id: decision.id,
        });
    }
}

fn decide_discard_half(
    active: &PlayingEngine,
    decision: OpenDecision,
    required: u16,
    resources: ResourceSet,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    if resources.total() != required
        || !active
            .game
            .players
            .get(player_id)
            .resources()
            .has_enough(&resources)
    {
        reject_illegal(
            events,
            &decision,
            format!("must discard exactly {required} available cards"),
        );
        return;
    }
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::PlayerDiscarded {
        player_id,
        resources,
    });

    let mut remaining = active.pending_discards.iter().copied();
    if active.pending_discards.first() == Some(&player_id) {
        remaining.next();
    }
    if let Some(next_player) = remaining.next() {
        let required = active.game.players.get(next_player).resources().total() / 2;
        let mut decisions = active.decisions();
        open_decision(
            events,
            &mut decisions,
            next_player,
            DecisionKind::DiscardHalf { required },
            DecisionLifetime::OneShot,
        );
    } else {
        let robber_player = active.game.turn.get_turn_index();
        let mut decisions = active.decisions();
        open_decision(
            events,
            &mut decisions,
            robber_player,
            DecisionKind::MoveRobber,
            DecisionLifetime::OneShot,
        );
    }
}

fn decide_move_robber(
    active: &PlayingEngine,
    decision: OpenDecision,
    hex: crate::topology::Hex,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    if hex == active.game.board_state.robber_pos {
        reject_illegal(events, &decision, "robber must move to a new hex");
        return;
    }
    let candidates: EventBatch =
        algorithm::robbery_candidates(hex, player_id, &active.game.builds, &active.game.players)
            .map(|robbed_id| GameEvent::ResourceStolen {
                player_id,
                robbed_id,
                resource: context
                    .stolen_resource
                    .unwrap_or(crate::gameplay::primitives::resource::Resource::Brick),
            })
            .collect();
    match candidates.as_slice() {
        [] => {
            events.push(GameEvent::DecisionClosed {
                decision_id: decision.id,
            });
            events.push(GameEvent::RobberMoved {
                player_id,
                hex,
                robbed_id: None,
            });
            reopen_regular(active, player_id, events);
        }
        [
            GameEvent::ResourceStolen {
                robbed_id,
                resource,
                ..
            },
        ] => {
            events.push(GameEvent::DecisionClosed {
                decision_id: decision.id,
            });
            events.push(GameEvent::RobberMoved {
                player_id,
                hex,
                robbed_id: Some(*robbed_id),
            });
            events.push(GameEvent::ResourceStolen {
                player_id,
                robbed_id: *robbed_id,
                resource: *resource,
            });
            reopen_regular(active, player_id, events);
        }
        _ => {
            events.push(GameEvent::DecisionClosed {
                decision_id: decision.id,
            });
            let mut decisions = active.decisions();
            open_decision(
                events,
                &mut decisions,
                player_id,
                DecisionKind::ChooseRobbedPlayer { robber_pos: hex },
                DecisionLifetime::OneShot,
            );
        }
    }
}

fn decide_choose_robbed_player(
    active: &PlayingEngine,
    decision: OpenDecision,
    robber_pos: crate::topology::Hex,
    robbed_id: PlayerId,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    if !algorithm::robbery_candidates(
        robber_pos,
        player_id,
        &active.game.builds,
        &active.game.players,
    )
    .any(|candidate| candidate == robbed_id)
    {
        reject_illegal(
            events,
            &decision,
            "chosen player cannot be robbed from the selected hex",
        );
        return;
    }
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::RobberMoved {
        player_id,
        hex: robber_pos,
        robbed_id: Some(robbed_id),
    });
    if let Some(resource) = context.stolen_resource {
        events.push(GameEvent::ResourceStolen {
            player_id,
            robbed_id,
            resource,
        });
    }
    reopen_regular(active, player_id, events);
}

fn reopen_regular(active: &PlayingEngine, player_id: PlayerId, events: &mut EventBatch) {
    let mut decisions = active.decisions();
    open_decision(
        events,
        &mut decisions,
        player_id,
        DecisionKind::RegularCommand,
        DecisionLifetime::OneShot,
    );
}

fn decide_use_dev_card(
    active: &PlayingEngine,
    decision: OpenDecision,
    usage: crate::gameplay::primitives::dev_card::DevCardUsage,
    next_kind: DecisionKind,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    if active.dev_card_used_this_turn {
        reject_illegal(events, &decision, "development card already used this turn");
        return;
    }
    let mut candidate = active.game.clone();
    if candidate
        .use_dev_card(usage, player_id, context.stolen_resource)
        .is_err()
    {
        reject_illegal(events, &decision, "invalid dev-card usage");
        return;
    }
    let candidate_index = GameIndex::rebuild(&candidate);
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
    if GameQuery::new(&candidate, &candidate_index)
        .check_win_condition()
        .is_some()
    {
        events.push(GameEvent::GameFinished {
            result: GameResult::Win(player_id),
            stats: Some(game_end_stats(&candidate, &candidate_index)),
        });
        return;
    }
    let mut decisions = active.decisions();
    open_decision(
        events,
        &mut decisions,
        player_id,
        next_kind,
        DecisionLifetime::OneShot,
    );
}

fn decide_buy_dev_card(active: &PlayingEngine, decision: OpenDecision, events: &mut EventBatch) {
    let player_id = decision.player_id;
    let Some(card) = active.game.bank.dev_cards.last().copied() else {
        reject_illegal(events, &decision, "development card bank is empty");
        return;
    };
    if !active
        .game
        .players
        .get(player_id)
        .resources()
        .has_enough(&crate::gameplay::constants::costs::DEV_CARD)
    {
        reject_illegal(
            events,
            &decision,
            "not enough resources to buy development card",
        );
        return;
    }
    let mut candidate = active.game.clone();
    if candidate.buy_dev_card(player_id).is_err() {
        reject_illegal(events, &decision, "invalid buy-dev-card action");
        return;
    }
    let candidate_index = GameIndex::rebuild(&candidate);

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::DevCardBought { player_id });
    events.push(GameEvent::DevCardDrawn { player_id, card });
    if let Some(winner) = GameQuery::new(&candidate, &candidate_index).check_win_condition() {
        events.push(GameEvent::GameFinished {
            result: GameResult::Win(winner),
            stats: Some(game_end_stats(&candidate, &candidate_index)),
        });
    } else {
        let mut decisions = active.decisions();
        open_decision(
            events,
            &mut decisions,
            player_id,
            DecisionKind::RegularCommand,
            DecisionLifetime::OneShot,
        );
    }
}

fn decide_roll_dice(
    active: &PlayingEngine,
    decision: OpenDecision,
    next_kind: DecisionKind,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let Some(roll) = context.dice_roll else {
        reject_illegal(events, &decision, "dice roll is missing");
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
            let mut decisions = active.decisions();
            open_decision(
                events,
                &mut decisions,
                player_id,
                next_kind,
                DecisionLifetime::OneShot,
            );
        }
        DiceOutcome::Seven => {
            open_next_discard_or_robber(active, player_id, events);
        }
    }
}

fn open_next_discard_or_robber(
    active: &PlayingEngine,
    robber_player: PlayerId,
    events: &mut EventBatch,
) {
    let first_discard = active.pending_discards.first().copied().or_else(|| {
        algorithm::player_order_from(robber_player, active.game.players.count())
            .find(|pid| active.game.players.get(*pid).resources().total() > 7)
    });
    if let Some(player_id) = first_discard {
        let required = active.game.players.get(player_id).resources().total() / 2;
        let mut decisions = active.decisions();
        open_decision(
            events,
            &mut decisions,
            player_id,
            DecisionKind::DiscardHalf { required },
            DecisionLifetime::OneShot,
        );
    } else {
        let mut decisions = active.decisions();
        open_decision(
            events,
            &mut decisions,
            robber_player,
            DecisionKind::MoveRobber,
            DecisionLifetime::OneShot,
        );
    }
}

fn decide_build(
    active: &PlayingEngine,
    decision: OpenDecision,
    build: Build,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    let mut candidate = active.game.clone();
    if candidate.build(player_id, build).is_err() {
        reject_illegal(events, &decision, "invalid build action");
        return;
    }
    let candidate_index = GameIndex::rebuild(&candidate);
    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::Built { player_id, build });
    if let Some(winner) = GameQuery::new(&candidate, &candidate_index).check_win_condition() {
        events.push(GameEvent::GameFinished {
            result: GameResult::Win(winner),
            stats: Some(game_end_stats(&candidate, &candidate_index)),
        });
    } else {
        let mut decisions = active.decisions();
        open_decision(
            events,
            &mut decisions,
            player_id,
            DecisionKind::RegularCommand,
            DecisionLifetime::OneShot,
        );
    }
}

fn game_end_stats(
    game: &crate::gameplay::game::state::GameState,
    index: &GameIndex,
) -> GameEndStats {
    let query = GameQuery::new(game, index);
    player_ids(game.players.count())
        .map(|player_id| {
            let build_vp = query.count_build_vp(player_id);
            let dev_card_vp = query.count_dev_card_vp(player_id);
            let has_longest_road = query.has_longest_road(player_id);
            let has_largest_army = query.has_largest_army(player_id);
            let award_vp = query.award_vp(player_id);
            let builds = game.builds.by_player(player_id);

            GameEndPlayerStats {
                player_id,
                total_vp: build_vp + dev_card_vp + award_vp,
                build_vp,
                dev_card_vp,
                award_vp,
                settlements: builds.settlements_count() as u16,
                cities: builds.cities_count() as u16,
                roads: builds.roads_count() as u16,
                longest_road_length: query.count_max_tract_length(player_id),
                knights_used: game.players.get(player_id).dev_cards().used[UsableDevCard::Knight],
                has_longest_road,
                has_largest_army,
            }
        })
        .collect()
}

fn decide_bank_trade(
    active: &PlayingEngine,
    decision: OpenDecision,
    trade: BankTrade,
    events: &mut EventBatch,
) {
    let player_id = decision.player_id;
    if !can_trade_with_bank(active, player_id, trade) {
        reject_illegal(events, &decision, "invalid bank trade action");
        return;
    }

    events.push(GameEvent::DecisionClosed {
        decision_id: decision.id,
    });
    events.push(GameEvent::BankTradeCompleted { player_id, trade });
    let mut decisions = active.decisions();
    open_decision(
        events,
        &mut decisions,
        player_id,
        DecisionKind::RegularCommand,
        DecisionLifetime::OneShot,
    );
}

fn can_trade_with_bank(active: &PlayingEngine, player_id: PlayerId, trade: BankTrade) -> bool {
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
    active: &PlayingEngine,
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
    let mut decisions = active.decisions();
    open_decision(
        events,
        &mut decisions,
        next_turn.get_turn_index(),
        DecisionKind::InitCommand,
        DecisionLifetime::OneShot,
    );
}

fn decide_initial_placement(
    active: &SetupEngine,
    decision: OpenDecision,
    command: command::InitialPlacementCommand,
    context: DecisionContext,
    events: &mut EventBatch,
) {
    let (settlement, road) = command.as_builds();

    let mut candidate_builds = active.table.builds.clone();
    if let Err(err) = candidate_builds.try_init_place(decision.player_id, road, settlement) {
        events.push(GameEvent::CommandRejected {
            token: DecisionToken::from(&decision),
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
        player_id: decision.player_id,
        settlement: settlement.vtx,
        road,
    });
    if active.setup_turn.get_rounds_played() == 1 {
        let resources = initial_resources(&active.table.board, settlement);
        if resources != ResourceSet::EMPTY {
            events.push(GameEvent::InitialResourcesGranted {
                player_id: decision.player_id,
                resources,
            });
        }
    }

    let mut candidate_turn = active.setup_turn.clone();
    candidate_turn.next();
    let mut decisions = active.decisions();
    if candidate_turn.get_rounds_played() < 2 {
        open_decision(
            events,
            &mut decisions,
            candidate_turn.get_turn_index(),
            DecisionKind::InitialPlacement,
            DecisionLifetime::OneShot,
        );
    } else {
        let regular_turn = candidate_turn.into_regular();
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
        open_decision(
            events,
            &mut decisions,
            regular_turn.get_turn_index(),
            DecisionKind::InitCommand,
            DecisionLifetime::OneShot,
        );
    }
}

fn initial_resources(board: &BoardLayout, settlement: Establishment) -> ResourceSet {
    let mut resources = ResourceSet::EMPTY;
    for hex in settlement
        .vtx
        .as_set()
        .into_iter()
        .filter(|hex| hex.norm() <= board.arrangement.radius() as usize)
    {
        if let Tile::Resource { resource, .. } = board.arrangement[hex] {
            resources += resource.into();
        }
    }
    resources
}
