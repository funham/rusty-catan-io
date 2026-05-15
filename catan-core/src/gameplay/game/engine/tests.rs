use super::{GameEngine, GameStatus};
use crate::{
    gameplay::{
        game::{
            decision::{DecisionKind, DecisionLifetime, OpenDecision},
            event::GameEvent,
            init::GameInitializationState,
            input::{GameInput, PlayerCommand, TradeCommand, TradeResponseCommand},
            output::{CommandRejectionReason, GameOutput, VecOutputSink},
            run::RunOptions,
            trade::TradeScope,
        },
        primitives::{
            resource::ResourceCollection,
            trade::{PlayerTrade, PublicTradeOffer},
        },
    },
    topology::Hex,
};

fn one_brick() -> ResourceCollection {
    ResourceCollection {
        brick: 1,
        ..ResourceCollection::ZERO
    }
}

fn one_wood() -> ResourceCollection {
    ResourceCollection {
        wood: 1,
        ..ResourceCollection::ZERO
    }
}

fn started_engine() -> (GameEngine, Vec<GameOutput>) {
    let init = GameInitializationState::default();
    let mut engine = GameEngine::from_init(init, RunOptions::default());
    let mut sink = VecOutputSink::default();
    engine.start(&mut sink);
    (engine, sink.into_vec())
}

fn first_open_decision(outputs: &[GameOutput]) -> OpenDecision {
    outputs
        .iter()
        .find_map(|output| match output {
            GameOutput::DecisionOpened(decision) => Some(decision.clone()),
            _ => None,
        })
        .expect("engine should open a decision")
}

#[test]
fn start_opens_one_shot_init_decision() {
    let (_engine, outputs) = started_engine();

    let decision = first_open_decision(&outputs);

    assert_eq!(decision.player_id, 0);
    assert!(matches!(decision.kind, DecisionKind::InitPlacement));
    assert_eq!(decision.lifetime, DecisionLifetime::OneShot);
}

#[test]
fn wrong_player_is_rejected_without_closing_decision() {
    let (mut engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);
    let mut sink = VecOutputSink::default();

    let status = engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: decision.id,
            command: PlayerCommand::MoveRobbers(crate::agent::action::MoveRobbersAction(Hex::new(
                0, 0,
            ))),
        },
        &mut sink,
    );

    assert_eq!(status, GameStatus::Waiting);
    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                player_id: 1,
                decision_id: Some(id),
                reason: CommandRejectionReason::WrongPlayer { expected: 0 },
            } if *id == decision.id
        )
    }));
}

#[test]
fn stale_decision_is_rejected_after_one_shot_closes() {
    let (mut engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);
    let placement = engine
        .legal_initial_placements(0)
        .into_iter()
        .next()
        .expect("default board should have an initial placement");
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: decision.id,
            command: PlayerCommand::InitialPlacement(placement),
        },
        &mut sink,
    );
    let mut stale_sink = VecOutputSink::default();
    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: decision.id,
            command: PlayerCommand::InitialPlacement(placement),
        },
        &mut stale_sink,
    );

    assert!(stale_sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                player_id: 0,
                decision_id: Some(id),
                reason: CommandRejectionReason::StaleDecision,
            } if *id == decision.id
        )
    }));
}

#[test]
fn reusable_trade_response_decision_can_be_updated_until_session_closes() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    engine.test_give_resources(0, one_brick());
    engine.test_give_resources(1, one_wood());

    let mut sink = VecOutputSink::default();
    let owner_decision = engine.open_decision_for_test(0, DecisionKind::RegularAction);
    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: owner_decision.id,
            command: PlayerCommand::Trade(TradeCommand::Propose {
                scope: TradeScope::Public,
                offer: PublicTradeOffer {
                    give: one_brick(),
                    take: one_wood(),
                },
            }),
        },
        &mut sink,
    );

    let response_decision = sink
        .as_slice()
        .iter()
        .find_map(|output| match output {
            GameOutput::DecisionOpened(decision)
                if decision.player_id == 1
                    && matches!(decision.kind, DecisionKind::TradeResponse { .. }) =>
            {
                Some(decision.clone())
            }
            _ => None,
        })
        .expect("trade should open response decision for peer");

    let mut update_sink = VecOutputSink::default();
    engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: response_decision.id,
            command: PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Accept {
                offer_id: 0.into(),
            })),
        },
        &mut update_sink,
    );
    engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: response_decision.id,
            command: PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Reject)),
        },
        &mut update_sink,
    );

    let updates = update_sink
        .into_vec()
        .into_iter()
        .filter(|output| {
            matches!(
                output,
                GameOutput::Event(GameEvent::TradeResponseUpdated { .. })
            )
        })
        .count();
    assert_eq!(updates, 2);
}

#[test]
fn trade_commit_revalidates_resources_and_rejects_missing_resources() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    engine.test_give_resources(0, one_brick());
    engine.test_give_resources(1, one_wood());
    let session = engine.test_open_trade_session(
        0,
        TradeScope::Public,
        PlayerTrade {
            give: one_brick(),
            take: one_wood(),
        },
    );
    let offer = engine.test_trade_original_offer(session);
    engine.test_set_trade_response_accept(session, 1, offer);
    engine.test_take_resources(1, one_wood());
    let owner = engine.open_decision_for_test(0, DecisionKind::TradeOwnerAction { session });
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: owner.id,
            command: PlayerCommand::Trade(TradeCommand::Commit { offer_id: offer }),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                player_id: 0,
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            }
        )
    }));
}

#[test]
fn same_resource_on_both_sides_is_rejected() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let owner = engine.open_decision_for_test(0, DecisionKind::RegularAction);
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: owner.id,
            command: PlayerCommand::Trade(TradeCommand::Propose {
                scope: TradeScope::Public,
                offer: PublicTradeOffer {
                    give: one_brick(),
                    take: one_brick(),
                },
            }),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            }
        )
    }));
}

#[test]
fn player_can_reject_trade() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let session = engine.test_open_trade_session(
        0,
        TradeScope::Public,
        PlayerTrade {
            give: one_brick(),
            take: one_wood(),
        },
    );
    let response = engine.open_decision_for_test(1, DecisionKind::TradeResponse { session });
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: response.id,
            command: PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Reject)),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::Event(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: 1,
                response: crate::gameplay::game::trade::TradeResponseState::Rejected,
            }) if *session_id == session
        )
    }));
}

#[test]
fn player_can_counter_trade() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let session = engine.test_open_trade_session(
        0,
        TradeScope::Public,
        PlayerTrade {
            give: one_brick(),
            take: one_wood(),
        },
    );
    let response = engine.open_decision_for_test(1, DecisionKind::TradeResponse { session });
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: response.id,
            command: PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Counter {
                offer: PlayerTrade {
                    give: one_wood(),
                    take: one_brick(),
                },
            })),
        },
        &mut sink,
    );

    let outputs = sink.into_vec();
    assert!(outputs.iter().any(|output| {
        matches!(
            output,
            GameOutput::Event(GameEvent::TradeOfferAdded {
                session_id,
                player_id: 1,
                ..
            }) if *session_id == session
        )
    }));
    assert!(outputs.iter().any(|output| {
        matches!(
            output,
            GameOutput::Event(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: 1,
                response: crate::gameplay::game::trade::TradeResponseState::Countered { .. },
            }) if *session_id == session
        )
    }));
}

#[test]
fn active_player_can_commit_accepted_offer() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    engine.test_give_resources(0, one_brick());
    engine.test_give_resources(1, one_wood());
    let session = engine.test_open_trade_session(
        0,
        TradeScope::Public,
        PlayerTrade {
            give: one_brick(),
            take: one_wood(),
        },
    );
    let offer = engine.test_trade_original_offer(session);
    engine.test_set_trade_response_accept(session, 1, offer);
    let owner = engine.open_decision_for_test(0, DecisionKind::TradeOwnerAction { session });
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: owner.id,
            command: PlayerCommand::Trade(TradeCommand::Commit { offer_id: offer }),
        },
        &mut sink,
    );

    assert_eq!(*engine.state().players.get(0).resources(), one_wood());
    assert_eq!(*engine.state().players.get(1).resources(), one_brick());
    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::Event(GameEvent::TradeCompleted {
                session_id,
                proposer_id: 0,
                peer_id: 1,
                offer_id: completed,
            }) if *session_id == session && *completed == offer
        )
    }));
}

#[test]
fn active_player_can_cancel_trade_and_close_trade_decisions() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let session = engine.test_open_trade_session(
        0,
        TradeScope::Public,
        PlayerTrade {
            give: one_brick(),
            take: one_wood(),
        },
    );
    let owner = engine.open_decision_for_test(0, DecisionKind::TradeOwnerAction { session });
    let peer = engine.open_decision_for_test(1, DecisionKind::TradeResponse { session });
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: owner.id,
            command: PlayerCommand::Trade(TradeCommand::Cancel),
        },
        &mut sink,
    );

    let outputs = sink.into_vec();
    assert!(outputs.iter().any(|output| {
        matches!(
            output,
            GameOutput::DecisionClosed { decision_id } if *decision_id == owner.id
        )
    }));
    assert!(outputs.iter().any(|output| {
        matches!(
            output,
            GameOutput::DecisionClosed { decision_id } if *decision_id == peer.id
        )
    }));
    assert!(outputs.iter().any(|output| {
        matches!(
            output,
            GameOutput::Event(GameEvent::TradeCancelled {
                session_id,
                proposer_id: 0,
            }) if *session_id == session
        )
    }));
}

#[test]
fn player_cannot_accept_another_players_counteroffer() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let session = engine.test_open_trade_session(
        0,
        TradeScope::Public,
        PlayerTrade {
            give: one_brick(),
            take: one_wood(),
        },
    );
    let countering_player =
        engine.open_decision_for_test(1, DecisionKind::TradeResponse { session });
    let mut counter_sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: countering_player.id,
            command: PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Counter {
                offer: PlayerTrade {
                    give: one_wood(),
                    take: one_brick(),
                },
            })),
        },
        &mut counter_sink,
    );
    let counter_offer_id = counter_sink
        .as_slice()
        .iter()
        .find_map(|output| match output {
            GameOutput::Event(GameEvent::TradeOfferAdded { offer_id, .. }) => Some(*offer_id),
            _ => None,
        })
        .expect("countering should add an offer");
    let other_player = engine.open_decision_for_test(2, DecisionKind::TradeResponse { session });
    let mut accept_sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 2,
            decision_id: other_player.id,
            command: PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Accept {
                offer_id: counter_offer_id,
            })),
        },
        &mut accept_sink,
    );

    assert!(accept_sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                player_id: 2,
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            }
        )
    }));
}

#[test]
fn targeted_trade_rejects_invalid_target() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let owner = engine.open_decision_for_test(0, DecisionKind::RegularAction);
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: owner.id,
            command: PlayerCommand::Trade(TradeCommand::Propose {
                scope: TradeScope::Targeted(99),
                offer: PublicTradeOffer {
                    give: one_brick(),
                    take: one_wood(),
                },
            }),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                player_id: 0,
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            }
        )
    }));
}

#[test]
fn submit_after_game_end_is_rejected_without_mutation() {
    let (mut engine, _outputs) = started_engine();
    engine.test_mark_ended();
    let decision = engine.open_decision_for_test(0, DecisionKind::RegularAction);
    let mut sink = VecOutputSink::default();

    let status = engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: decision.id,
            command: PlayerCommand::Regular(crate::agent::action::RegularAction::EndMove),
        },
        &mut sink,
    );

    assert_eq!(status, GameStatus::Ended);
    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                player_id: 0,
                reason: CommandRejectionReason::GameEnded,
                ..
            }
        )
    }));
}
