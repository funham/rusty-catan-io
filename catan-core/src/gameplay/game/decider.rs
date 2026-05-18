use crate::gameplay::game::{
    decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
    event::{EventBatch, GameEndPlayerStats, GameEndStats, GameEvent},
    input::{GameInput, PlayerCommand, TradeCommand, TradeResponseCommand},
    lifecycle::EngineCore,
    output::CommandRejectionReason,
    phase::GamePhase,
    run::GameResult,
};
use crate::gameplay::{
    field::state::BoardLayout,
    game::command::RegularCommand,
    game::{index::GameIndex, query::GameQuery},
    primitives::{
        PortKind, Tile,
        build::{Build, Establishment},
        dev_card::UsableDevCard,
        player::player_ids,
        resource::ResourceSet,
        trade::{BankTrade, BankTradeKind, PersonalTradeOffer, PublicTradeOffer},
    },
};
use crate::{
    algorithm, constants,
    math::dice::{DiceOutcome, DiceRoll},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionContext {
    pub max_turns: Option<u64>,
    pub max_invalid_actions: Option<u64>,
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
        player_id: active.setup_turn.as_ref().map_or_else(
            || active.game.turn.get_turn_index(),
            |turn| turn.get_turn_index(),
        ),
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
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(
                crate::gameplay::game::command::PostDiceCommand::RegularCommand(
                    RegularCommand::EndMove,
                ),
            ),
        ) => {
            decide_end_move(active, decision, context, &mut events);
        }
        (DecisionKind::RegularCommand, PlayerCommand::Regular(RegularCommand::Build(build))) => {
            decide_build(active, decision, build, &mut events);
        }
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(
                crate::gameplay::game::command::PostDiceCommand::RegularCommand(
                    RegularCommand::Build(build),
                ),
            ),
        ) => {
            decide_build(active, decision, build, &mut events);
        }
        (DecisionKind::RegularCommand, PlayerCommand::Regular(RegularCommand::BuyDevCard)) => {
            decide_buy_dev_card(active, decision, &mut events);
        }
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(
                crate::gameplay::game::command::PostDiceCommand::RegularCommand(
                    RegularCommand::BuyDevCard,
                ),
            ),
        ) => {
            decide_buy_dev_card(active, decision, &mut events);
        }
        (
            DecisionKind::RegularCommand,
            PlayerCommand::Regular(RegularCommand::OfferPublicTrade(offer)),
        ) => decide_open_trade(
            active,
            decision,
            crate::gameplay::game::trade::TradeScope::Public,
            trade_from_public_offer(offer),
            &mut events,
        ),
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(
                crate::gameplay::game::command::PostDiceCommand::RegularCommand(
                    RegularCommand::OfferPublicTrade(offer),
                ),
            ),
        ) => decide_open_trade(
            active,
            decision,
            crate::gameplay::game::trade::TradeScope::Public,
            trade_from_public_offer(offer),
            &mut events,
        ),
        (
            DecisionKind::RegularCommand,
            PlayerCommand::Regular(RegularCommand::OfferPersonalTrade(offer)),
        ) => {
            let (scope, trade) = trade_from_personal_offer(offer);
            decide_open_trade(active, decision, scope, trade, &mut events);
        }
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(
                crate::gameplay::game::command::PostDiceCommand::RegularCommand(
                    RegularCommand::OfferPersonalTrade(offer),
                ),
            ),
        ) => {
            let (scope, trade) = trade_from_personal_offer(offer);
            decide_open_trade(active, decision, scope, trade, &mut events);
        }
        (
            DecisionKind::PostDiceCommand,
            PlayerCommand::PostDice(
                crate::gameplay::game::command::PostDiceCommand::RegularCommand(
                    RegularCommand::TradeWithBank(trade),
                ),
            ),
        )
        | (
            DecisionKind::RegularCommand,
            PlayerCommand::Regular(RegularCommand::TradeWithBank(trade)),
        ) => {
            decide_bank_trade(active, decision, trade, &mut events);
        }
        (
            DecisionKind::RegularCommand,
            PlayerCommand::Trade(TradeCommand::Propose { scope, offer }),
        ) => decide_open_trade(
            active,
            decision,
            scope,
            trade_from_public_offer(offer),
            &mut events,
        ),
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
        (
            DecisionKind::DropHalf { required },
            PlayerCommand::DropHalf(crate::gameplay::game::command::DropHalfCommand(resources)),
        ) => {
            decide_drop_half(active, decision, required, resources, &mut events);
        }
        (
            DecisionKind::MoveRobber,
            PlayerCommand::MoveRobber(crate::gameplay::game::command::MoveRobberCommand(hex)),
        ) => {
            decide_move_robber(active, decision, hex, context, &mut events);
        }
        (
            DecisionKind::ChooseRobbedPlayer { robber_pos },
            PlayerCommand::ChooseRobbedPlayer(
                crate::gameplay::game::command::ChooseRobbedPlayerCommand(robbed_id),
            ),
        ) => {
            decide_choose_robbed_player(
                active,
                decision,
                robber_pos,
                robbed_id,
                context,
                &mut events,
            );
        }
        (DecisionKind::TradeResponse { session }, PlayerCommand::Trade(command)) => {
            decide_trade_response(active, decision, session, command, &mut events);
        }
        (DecisionKind::TradeOwnerAction { session }, PlayerCommand::Trade(command)) => {
            decide_trade_owner(active, decision, session, command, &mut events);
        }
        _ => {
            reject(
                &mut events,
                decision.player_id,
                Some(decision.id),
                CommandRejectionReason::WrongPhase,
                false,
            );
        }
    }

    finish_after_invalid_action_limit(active, context, &mut events);
    events
}

fn finish_after_invalid_action_limit(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
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
    player_id: crate::gameplay::primitives::player::PlayerId,
    decision_id: Option<DecisionId>,
    reason: CommandRejectionReason,
    counts_toward_limit: bool,
) {
    events.push(GameEvent::CommandRejected {
        player_id,
        decision_id,
        reason,
        counts_toward_limit,
    });
}

fn reject_illegal(events: &mut EventBatch, decision: &OpenDecision, reason: impl Into<String>) {
    reject(
        events,
        decision.player_id,
        Some(decision.id),
        CommandRejectionReason::IllegalCommand(reason.into()),
        true,
    );
}

fn decide_open_trade(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    scope: crate::gameplay::game::trade::TradeScope,
    trade: crate::gameplay::primitives::trade::PlayerTrade,
    events: &mut EventBatch,
) {
    if crate::gameplay::game::trade::trade_has_overlapping_resources(&trade)
        || invalid_trade_scope_reason(scope, decision.player_id, active.game.players.count())
            .is_some()
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
        scope,
        offer_id,
        offer: trade,
    });
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id: decision.player_id,
        kind: DecisionKind::TradeOwnerAction {
            session: session_id,
        },
        lifetime: DecisionLifetime::UntilSessionClosed(session_id),
    }));
    let mut next_decision_id = active.next_decision_id + 1;
    for player_id in player_ids(active.game.players.count()) {
        if player_id != decision.player_id && scope.includes(player_id) {
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(next_decision_id),
                player_id,
                kind: DecisionKind::TradeResponse {
                    session: session_id,
                },
                lifetime: DecisionLifetime::UntilSessionClosed(session_id),
            }));
            next_decision_id += 1;
        }
    }
}

fn trade_from_public_offer(
    offer: PublicTradeOffer,
) -> crate::gameplay::primitives::trade::PlayerTrade {
    crate::gameplay::primitives::trade::PlayerTrade {
        give: offer.give,
        take: offer.take,
    }
}

fn trade_from_personal_offer(
    offer: PersonalTradeOffer,
) -> (
    crate::gameplay::game::trade::TradeScope,
    crate::gameplay::primitives::trade::PlayerTrade,
) {
    (
        crate::gameplay::game::trade::TradeScope::Targeted(offer.peer_id),
        crate::gameplay::primitives::trade::PlayerTrade {
            give: offer.give,
            take: offer.take,
        },
    )
}

fn invalid_trade_scope_reason(
    scope: crate::gameplay::game::trade::TradeScope,
    proposer: crate::gameplay::primitives::player::PlayerId,
    player_count: usize,
) -> Option<()> {
    match scope {
        crate::gameplay::game::trade::TradeScope::Public => None,
        crate::gameplay::game::trade::TradeScope::Targeted(peer)
            if peer.index() >= player_count || peer == proposer =>
        {
            Some(())
        }
        crate::gameplay::game::trade::TradeScope::Targeted(_) => None,
    }
}

fn decide_trade_response(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    command: TradeCommand,
    events: &mut EventBatch,
) {
    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            decision.player_id,
            Some(decision.id),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    if !session.open || !session.scope.includes(decision.player_id) {
        reject(
            events,
            decision.player_id,
            Some(decision.id),
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
            if let Some(peer) = offer.peer
                && peer != decision.player_id
            {
                reject_illegal(
                    events,
                    &decision,
                    "only the counteroffer owner can accept that counteroffer",
                );
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
            if crate::gameplay::game::trade::trade_has_overlapping_resources(&offer) {
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
            decision.player_id,
            Some(decision.id),
            CommandRejectionReason::WrongPhase,
            false,
        ),
    }
}

fn decide_trade_owner(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    command: TradeCommand,
    events: &mut EventBatch,
) {
    match command {
        TradeCommand::Commit { offer_id } => {
            decide_commit_trade(active, decision, session_id, offer_id, events);
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
            decision.player_id,
            Some(decision.id),
            CommandRejectionReason::WrongPhase,
            false,
        ),
    }
}

fn decide_commit_trade(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    session_id: crate::gameplay::game::trade::TradeSessionId,
    offer_id: crate::gameplay::game::trade::TradeOfferId,
    events: &mut EventBatch,
) {
    let Some(session) = active.trade_sessions.get(session_id.0 as usize) else {
        reject(
            events,
            decision.player_id,
            Some(decision.id),
            CommandRejectionReason::StaleDecision,
            false,
        );
        return;
    };
    let Some(offer) = session.offer(offer_id) else {
        reject_illegal(events, &decision, "unknown trade offer");
        return;
    };
    let Some(peer_id) = offer
        .peer
        .or_else(|| session.accepted_peer_for_offer(offer_id))
    else {
        reject_illegal(
            events,
            &decision,
            "no player has accepted the selected trade offer",
        );
        return;
    };
    if !crate::gameplay::game::trade::trade_is_funded(
        active.game.players.get(session.proposer).resources(),
        active.game.players.get(peer_id).resources(),
        &offer.trade,
    ) {
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

fn close_session_decisions(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
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

fn decide_drop_half(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
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
        events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id: next_player,
            kind: DecisionKind::DropHalf { required },
            lifetime: DecisionLifetime::OneShot,
        }));
    } else {
        let robber_player = active.game.turn.get_turn_index();
        events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id: robber_player,
            kind: DecisionKind::MoveRobber,
            lifetime: DecisionLifetime::OneShot,
        }));
    }
}

fn decide_move_robber(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
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
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id,
                kind: DecisionKind::ChooseRobbedPlayer { robber_pos: hex },
                lifetime: DecisionLifetime::OneShot,
            }));
        }
    }
}

fn decide_choose_robbed_player(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    decision: OpenDecision,
    robber_pos: crate::topology::Hex,
    robbed_id: crate::gameplay::primitives::player::PlayerId,
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

fn reopen_regular(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
    player_id: crate::gameplay::primitives::player::PlayerId,
    events: &mut EventBatch,
) {
    events.push(GameEvent::DecisionOpened(OpenDecision {
        id: DecisionId(active.next_decision_id),
        player_id,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    }));
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
        events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id,
            kind: DecisionKind::RegularCommand,
            lifetime: DecisionLifetime::OneShot,
        }));
    }
}

fn decide_roll_dice(
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
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
        events.push(GameEvent::DecisionOpened(OpenDecision {
            id: DecisionId(active.next_decision_id),
            player_id,
            kind: DecisionKind::RegularCommand,
            lifetime: DecisionLifetime::OneShot,
        }));
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
            let has_longest_road = query.longest_road_owner() == Some(player_id);
            let has_largest_army = query.largest_army_owner() == Some(player_id);
            let award_vp = u16::from(has_longest_road) * constants::LONGEST_ROAD_VP
                + u16::from(has_largest_army) * constants::LARGEST_ARMY_VP;
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
    active: &crate::gameplay::game::lifecycle::ActiveEngine,
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

    if let Some(setup_turn) = &active.setup_turn {
        let mut candidate_builds = active.game.builds.clone();
        if let Err(err) = candidate_builds.try_init_place(player_id, road, settlement) {
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
        if setup_turn.get_rounds_played() == 1 {
            let resources = initial_resources(&active.game.board, settlement);
            if resources != ResourceSet::ZERO {
                events.push(GameEvent::InitialResourcesGranted {
                    player_id,
                    resources,
                });
            }
        }

        let mut candidate_turn = setup_turn.clone();
        candidate_turn.next();
        if candidate_turn.get_rounds_played() < 2 {
            events.push(GameEvent::DecisionOpened(OpenDecision {
                id: DecisionId(active.next_decision_id),
                player_id: candidate_turn.get_turn_index(),
                kind: DecisionKind::InitPlacement,
                lifetime: DecisionLifetime::OneShot,
            }));
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

fn initial_resources(board: &BoardLayout, settlement: Establishment) -> ResourceSet {
    let mut resources = ResourceSet::ZERO;
    for hex in settlement
        .vtx
        .as_set()
        .into_iter()
        .filter(|hex| hex.norm() <= board.arrangement.radius() as usize)
    {
        if let Tile::Resource { resource, .. } = board.arrangement[hex] {
            resources += &resource.into();
        }
    }
    resources
}
