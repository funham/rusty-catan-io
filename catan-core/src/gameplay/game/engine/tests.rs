use super::{GameEngine, GameStatus};
use crate::{
    gameplay::{
        game::{
            decider,
            decision::{DecisionKind, DecisionLifetime, OpenDecision},
            event::{EventBatch, GameEvent},
            init::GameInitializationState,
            input::{GameInput, PlayerCommand, TradeCommand, TradeResponseCommand},
            lifecycle::EngineCore,
            output::{CommandRejectionReason, GameOutput, VecOutputSink},
            phase::GamePhase,
            reducer,
            run::RunOptions,
            trade::TradeScope,
        },
        primitives::{
            dev_card::DevCardKind,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind, PlayerTrade, PublicTradeOffer},
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

fn output_event(output: &GameOutput) -> Option<&GameEvent> {
    match output {
        GameOutput::Event(record) => Some(&record.event),
        _ => None,
    }
}

fn add_two_initial_settlements(engine: &mut GameEngine) -> Hex {
    let mut victim_hex = None;

    for player_id in 0..2 {
        let (establishment, road) = engine
            .game
            .builds
            .query()
            .possible_initial_placements(&engine.game.board, player_id)
            .iter()
            .map(crate::gameplay::game::action::InitStageAction::as_builds)
            .next()
            .expect("default board should have initial placements");

        if player_id == 1 {
            let board_hexes = engine.game.board.arrangement.hex_iter().collect::<Vec<_>>();
            victim_hex = establishment.vtx.as_set().into_iter().find(|hex| {
                *hex != engine.game.board_state.robber_pos && board_hexes.contains(hex)
            });
        }

        engine
            .game
            .builds
            .try_init_place(player_id, road, establishment)
            .expect("generated initial placement should be valid");
    }

    victim_hex.expect("victim settlement should touch a non-robber hex")
}

#[test]
fn event_batch_has_extra_inline_capacity() {
    let mut batch = EventBatch::new();

    for _ in 0..32 {
        batch.push(GameEvent::GameStarted);
    }

    assert!(!batch.spilled());
}

#[test]
fn reducer_moves_active_lifecycle_to_finished_result() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::GameFinished {
            result: crate::gameplay::game::run::GameResult::LimitReached { turns: 0 },
            stats: None,
        },
    )
    .unwrap();

    let EngineCore::Finished(finished) = lifecycle else {
        panic!("finished event should move active lifecycle to finished");
    };
    assert_eq!(
        finished.result,
        crate::gameplay::game::run::GameResult::LimitReached { turns: 0 }
    );
}

#[test]
fn reducer_replays_initial_placement_event() {
    let init = GameInitializationState::default();
    let placement = init
        .builds
        .query()
        .possible_initial_placements(&init.board, 0)
        .into_iter()
        .next()
        .expect("default board should have an initial placement");
    let (settlement, road) = placement.as_builds();
    let mut lifecycle = EngineCore::active(init.finish());

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::InitialPlacementBuilt {
            player_id: 0,
            settlement: settlement.vtx,
            road,
        },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.builds.by_player(0).settlements_count(), 1);
    assert_eq!(active.game.builds.by_player(0).roads_count(), 1);
}

#[test]
fn reducer_applies_explicit_resource_distribution_event() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let mut by_player = smallvec::SmallVec::new();
    by_player.push((0, one_brick()));

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::ResourcesDistributed { by_player },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.players.get(0).resources().brick, 1);
    assert_eq!(active.game.bank.resources.brick, 18);
}

#[test]
fn reducer_applies_explicit_resource_stolen_event() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    lifecycle
        .active_mut()
        .unwrap()
        .game
        .transfer_from_bank(Resource::Brick.into(), 1)
        .unwrap();

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::ResourceStolen {
            player_id: 0,
            robbed_id: 1,
            resource: Resource::Brick,
        },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.players.get(0).resources().brick, 1);
    assert_eq!(active.game.players.get(1).resources().brick, 0);
}

#[test]
fn reducer_applies_discard_robber_and_turn_events() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    lifecycle
        .active_mut()
        .unwrap()
        .game
        .transfer_from_bank(one_brick(), 0)
        .unwrap();
    let target_hex = Hex::new(1, 0);

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::PlayerDiscarded {
            player_id: 0,
            resources: one_brick(),
        },
    )
    .unwrap();
    reducer::reduce(
        &mut lifecycle,
        &GameEvent::RobberMoved {
            player_id: 0,
            hex: target_hex,
            robbed_id: None,
        },
    )
    .unwrap();
    reducer::reduce(
        &mut lifecycle,
        &GameEvent::TurnEnded {
            player_id: 0,
            turn_no: 0,
        },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.players.get(0).resources().brick, 0);
    assert_eq!(active.game.bank.resources.brick, 19);
    assert_eq!(active.game.board_state.robber_pos, target_hex);
    assert_eq!(active.game.turn.get_turn_index(), 1);
}

#[test]
fn reducer_applies_bank_trade_event_with_exact_exchange() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    lifecycle
        .active_mut()
        .unwrap()
        .game
        .transfer_from_bank(
            ResourceCollection {
                brick: 4,
                ..ResourceCollection::ZERO
            },
            0,
        )
        .unwrap();

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::Traded {
            player_id: 0,
            trade: BankTrade {
                kind: BankTradeKind::BankGeneric,
                give: Resource::Brick,
                take: Resource::Wood,
            },
        },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.players.get(0).resources().brick, 0);
    assert_eq!(active.game.players.get(0).resources().wood, 1);
}

#[test]
fn decider_start_emits_game_started_and_initial_decision() {
    let lifecycle = EngineCore::active(GameInitializationState::default().finish());

    let events = decider::decide(&lifecycle, GameInput::Start);

    assert!(matches!(events.as_slice(), [
        GameEvent::GameStarted,
        GameEvent::DecisionOpened(decision),
    ] if decision.player_id == 0 && matches!(decision.kind, DecisionKind::InitPlacement)));
    assert!(!events.spilled());
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
fn start_updates_reducer_lifecycle_mirror() {
    let (engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);

    let active = engine
        .lifecycle()
        .as_active()
        .expect("started engine should have active lifecycle");

    assert!(matches!(active.phase, GamePhase::InitialPlacement));
    assert!(active.pending.get(decision.id).is_some());
    assert_eq!(active.next_decision_id, decision.id.0 + 1);
}

#[test]
fn start_outputs_share_one_transaction_id() {
    let (_engine, outputs) = started_engine();
    let tx_ids = outputs
        .iter()
        .filter_map(|output| match output {
            GameOutput::Event(record) => Some(record.tx_id),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(tx_ids.len(), 1);
    assert_eq!(tx_ids.first().copied(), Some(1));
}

#[test]
fn start_emits_domain_decision_opened_event() {
    let (_engine, outputs) = started_engine();

    assert!(outputs.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::DecisionOpened(decision))
                if decision.player_id == 0 && matches!(decision.kind, DecisionKind::InitPlacement)
        )
    }));
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
            command: PlayerCommand::MoveRobbers(crate::gameplay::game::action::MoveRobbersAction(
                Hex::new(0, 0),
            )),
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
fn wrong_player_rejection_emits_domain_command_rejected_event() {
    let (mut engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 1,
            decision_id: decision.id,
            command: PlayerCommand::MoveRobbers(crate::gameplay::game::action::MoveRobbersAction(
                Hex::new(0, 0),
            )),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::CommandRejected {
                player_id: 1,
                decision_id: Some(id),
                reason: CommandRejectionReason::WrongPlayer { expected: 0 },
                counts_toward_limit: false,
            }) if *id == decision.id
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
fn buying_dev_card_emits_private_drawn_card_event() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    engine.test_give_resources(
        0,
        ResourceCollection {
            wheat: 1,
            sheep: 1,
            ore: 1,
            ..ResourceCollection::ZERO
        },
    );
    engine.game.bank.dev_cards = vec![DevCardKind::VictoryPoint];
    let decision = engine.open_decision_for_test(0, DecisionKind::RegularAction);
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: decision.id,
            command: PlayerCommand::Regular(
                crate::gameplay::game::action::RegularAction::BuyDevCard,
            ),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::DevCardDrawn {
                player_id: 0,
                card: DevCardKind::VictoryPoint,
            })
        )
    }));
}

#[test]
fn moving_robber_emits_stolen_resource_event() {
    let (mut engine, _outputs) = started_engine();
    let victim_hex = add_two_initial_settlements(&mut engine);
    engine.test_give_resources(1, Resource::Brick.into());
    let decision = engine.open_decision_for_test(0, DecisionKind::MoveRobber);
    let mut sink = VecOutputSink::default();

    engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: decision.id,
            command: PlayerCommand::MoveRobbers(crate::gameplay::game::action::MoveRobbersAction(
                victim_hex,
            )),
        },
        &mut sink,
    );

    assert!(sink.into_vec().iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::ResourceStolen {
                player_id: 0,
                robbed_id: 1,
                resource: Resource::Brick,
            })
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
                output_event(output),
                Some(GameEvent::TradeResponseUpdated { .. })
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
            output_event(output),
            Some(GameEvent::TradeResponseUpdated {
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
            output_event(output),
            Some(GameEvent::TradeOfferAdded {
                session_id,
                player_id: 1,
                ..
            }) if *session_id == session
        )
    }));
    assert!(outputs.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::TradeResponseUpdated {
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
            output_event(output),
            Some(GameEvent::TradeCompleted {
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
            output_event(output),
            Some(GameEvent::TradeCancelled {
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
            GameOutput::Event(record) => match &record.event {
                GameEvent::TradeOfferAdded { offer_id, .. } => Some(*offer_id),
                _ => None,
            },
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
    let decision = engine.open_decision_for_test(0, DecisionKind::RegularAction);
    engine.test_mark_ended();
    let mut sink = VecOutputSink::default();

    let status = engine.apply(
        GameInput::Submit {
            player_id: 0,
            decision_id: decision.id,
            command: PlayerCommand::Regular(crate::gameplay::game::action::RegularAction::EndMove),
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
