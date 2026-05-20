use super::GameEngine;
use crate::{
    gameplay::{
        game::{
            command::{InitCommand, RegularCommand},
            decider,
            decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
            event::{EventBatch, GameEvent},
            init::GameInitializationState,
            input::{
                DecisionRequest, DecisionResponse, DecisionToken, GameInput, PlayerCommand,
                TradeCommand, TradeResponseCommand,
            },
            lifecycle::EngineCore,
            output::{CommandRejectionReason, GameOutput},
            projector, reducer,
            run::{GameResult, RunOptions},
            trade::TradeScope,
        },
        primitives::{
            build::{Build, Road},
            dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceSet},
            trade::{BankTrade, BankTradeKind, PlayerTrade, PublicTradeOffer},
        },
    },
    math::dice::DiceRoll,
    topology::Hex,
};

const P0: PlayerId = PlayerId::new(0);
const P1: PlayerId = PlayerId::new(1);
const P2: PlayerId = PlayerId::new(2);
const P99: PlayerId = PlayerId::new(99);

fn one_brick() -> ResourceSet {
    ResourceSet {
        brick: 1,
        ..ResourceSet::EMPTY
    }
}

fn one_wood() -> ResourceSet {
    ResourceSet {
        wood: 1,
        ..ResourceSet::EMPTY
    }
}

fn started_engine() -> (GameEngine, Vec<GameOutput>) {
    let init = GameInitializationState::default();
    let mut engine = GameEngine::from_init(init, RunOptions::default());
    let outputs = start_outputs(&mut engine);
    (engine, outputs)
}

fn start_outputs(engine: &mut GameEngine) -> Vec<GameOutput> {
    let transition = engine.start().expect("start should reduce");
    projector::project_transaction(&transition.transaction)
}

fn submit(player_id: PlayerId, decision_id: DecisionId, command: PlayerCommand) -> GameInput {
    GameInput::Submit(DecisionResponse {
        token: DecisionToken {
            id: decision_id,
            player_id,
        },
        command,
    })
}

fn apply_outputs(
    engine: &mut GameEngine,
    input: GameInput,
) -> (Option<GameResult>, Vec<GameOutput>) {
    let transition = engine.apply(input).expect("submit should reduce");
    (
        transition.result,
        projector::project_transaction(&transition.transaction),
    )
}

fn apply_to_sink(
    engine: &mut GameEngine,
    input: GameInput,
    sink: &mut Vec<GameOutput>,
) -> Option<GameResult> {
    let (status, outputs) = apply_outputs(engine, input);
    for output in outputs {
        sink.push(output);
    }
    status
}

fn first_open_decision(outputs: &[GameOutput]) -> DecisionRequest {
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

fn output_events(outputs: &[GameOutput]) -> Vec<&GameEvent> {
    outputs.iter().filter_map(output_event).collect()
}

fn add_two_initial_settlements(engine: &mut GameEngine) -> Hex {
    engine.core.force_playing_for_tests();
    let mut victim_hex = None;

    for player_id in [P0, P1] {
        let state = engine.state();
        let (establishment, road) = state
            .builds
            .query()
            .possible_initial_placements(&state.board, player_id)
            .iter()
            .map(crate::gameplay::game::command::InitialPlacementCommand::as_builds)
            .next()
            .expect("default board should have initial placements");

        if player_id == P1 {
            let state = engine.state();
            let board_hexes = state.board.arrangement.hex_iter().collect::<Vec<_>>();
            victim_hex = establishment
                .vtx
                .as_set()
                .into_iter()
                .find(|hex| *hex != state.board_state.robber_pos && board_hexes.contains(hex));
        }

        engine
            .core
            .active_mut()
            .expect("test engine should be active")
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
            player_id: P0,
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
    by_player.push((P0, one_brick()));

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
fn reducer_applies_initial_resource_grant_event() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::InitialResourcesGranted {
            player_id: P0,
            resources: one_brick(),
        },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.players.get(0).resources().brick, 1);
    assert_eq!(active.game.bank.resources.brick, 18);
    assert_eq!(active.stats.resources_distributed, 0);
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
            player_id: P0,
            robbed_id: P1,
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
            player_id: P0,
            resources: one_brick(),
        },
    )
    .unwrap();
    reducer::reduce(
        &mut lifecycle,
        &GameEvent::RobberMoved {
            player_id: P0,
            hex: target_hex,
            robbed_id: None,
        },
    )
    .unwrap();
    reducer::reduce(
        &mut lifecycle,
        &GameEvent::TurnEnded {
            player_id: P0,
            turn_no: 0,
        },
    )
    .unwrap();

    let active = lifecycle.as_active().expect("lifecycle should stay active");
    assert_eq!(active.game.players.get(0).resources().brick, 0);
    assert_eq!(active.game.bank.resources.brick, 19);
    assert_eq!(active.game.board_state.robber_pos, target_hex);
    assert_eq!(active.game.turn.get_turn_index(), 1);
    assert_eq!(active.stats.regular_actions, 1);
}

#[test]
fn reducer_applies_bank_trade_event_with_exact_exchange() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    lifecycle
        .active_mut()
        .unwrap()
        .game
        .transfer_from_bank(
            ResourceSet {
                brick: 4,
                ..ResourceSet::EMPTY
            },
            0,
        )
        .unwrap();

    reducer::reduce(
        &mut lifecycle,
        &GameEvent::BankTradeCompleted {
            player_id: P0,
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
    let lifecycle = EngineCore::unstarted(GameInitializationState::default());

    let events = decider::decide(&lifecycle, GameInput::Start);

    assert!(matches!(events.as_slice(), [
        GameEvent::GameStarted,
        GameEvent::DecisionOpened(decision),
    ] if decision.player_id() == 0 && matches!(decision.kind(), DecisionKind::InitialPlacement)));
    assert!(!events.spilled());
}

#[test]
fn decider_end_move_emits_turn_transition_events() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active.pending.push(decision.clone());

    let events = decider::decide(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(RegularCommand::EndMove),
        ),
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::TurnEnded {
                player_id: P0,
                turn_no: 0,
            },
            GameEvent::TurnStarted {
                player_id: P1,
                turn_no: 1,
            },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P1,
                kind: DecisionKind::InitCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id() && id.0 == 8
    ));
}

#[test]
fn decider_valid_bank_trade_emits_trade_and_reopens_regular_decision() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active
        .game
        .transfer_from_bank(
            ResourceSet {
                brick: 4,
                ..ResourceSet::EMPTY
            },
            P0,
        )
        .unwrap();
    active.pending.push(decision.clone());
    let trade = BankTrade {
        kind: BankTradeKind::BankGeneric,
        give: Resource::Brick,
        take: Resource::Wood,
    };

    let events = decider::decide(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(RegularCommand::TradeWithBank(trade)),
        ),
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::BankTradeCompleted {
                player_id: P0,
                trade: emitted,
            },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id()
            && emitted.kind == trade.kind
            && emitted.give == trade.give
            && emitted.take == trade.take
            && id.0 == 8
    ));
}

#[test]
fn decider_valid_build_emits_build_and_reopens_regular_decision() {
    let init = GameInitializationState::default();
    let mut game = init.finish();
    let first_placement = game
        .builds
        .query()
        .possible_initial_placements(&game.board, P0)
        .into_iter()
        .next()
        .expect("default board should have an initial placement");
    let (settlement, road) = first_placement.as_builds();
    game.builds
        .try_init_place(P0, road, settlement)
        .expect("initial placement should be valid");
    game.transfer_from_bank(crate::constants::costs::ROAD, P0)
        .unwrap();
    let build = game
        .board
        .paths()
        .iter()
        .copied()
        .map(|path| Build::Road(Road { path }))
        .find(|build| {
            let mut candidate = game.clone();
            candidate.build(P0, *build).is_ok()
        })
        .expect("player should have a valid road extension");
    let mut lifecycle = EngineCore::active(game);
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active.pending.push(decision.clone());

    let events = decider::decide(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(RegularCommand::Build(build)),
        ),
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::Built {
                player_id: P0,
                build: emitted,
            },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id()
            && matches!((emitted, build), (Build::Road(left), Build::Road(right)) if left.path == right.path)
            && id.0 == 8
    ));
}

#[test]
fn decider_roll_dice_harvest_emits_complete_facts() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::InitCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active.pending.push(decision.clone());

    let events = decider::decide_with_context(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::InitCommand(crate::gameplay::game::command::InitCommand::RollDice),
        ),
        decider::DecisionContext {
            max_turns: None,
            max_invalid_actions: None,
            dice_roll: Some(DiceRoll::eight()),
            stolen_resource: None,
        },
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::DiceRolled {
                player_id: P0,
                value,
            },
            GameEvent::ResourcesDistributed { .. },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P0,
                kind: DecisionKind::PostDiceCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id() && *value == DiceRoll::eight() && id.0 == 8
    ));
}

#[test]
fn decider_roll_dice_seven_opens_discard_or_robber_decision() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::InitCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active.pending.push(decision.clone());

    let events = decider::decide_with_context(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::InitCommand(InitCommand::RollDice),
        ),
        decider::DecisionContext {
            max_turns: None,
            max_invalid_actions: None,
            dice_roll: Some(DiceRoll::seven()),
            stolen_resource: None,
        },
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::DiceRolled {
                player_id: P0,
                value,
            },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P0,
                kind: DecisionKind::MoveRobber,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id() && *value == DiceRoll::seven() && id.0 == 8
    ));
}

#[test]
fn decider_buy_dev_card_emits_private_draw_fact_and_reopens_regular_decision() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::RegularCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active
        .game
        .transfer_from_bank(crate::constants::costs::DEV_CARD, P0)
        .unwrap();
    active.game.bank.dev_cards = vec![DevCardKind::VictoryPoint];
    active.pending.push(decision.clone());

    let events = decider::decide(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(RegularCommand::BuyDevCard),
        ),
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::DevCardBought { player_id: P0 },
            GameEvent::DevCardDrawn {
                player_id: P0,
                card: DevCardKind::VictoryPoint,
            },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id() && id.0 == 8
    ));
}

#[test]
fn decider_use_dev_card_emits_usage_and_post_dev_card_decision() {
    let mut lifecycle = EngineCore::active(GameInitializationState::default().finish());
    let decision = OpenDecision {
        id: crate::gameplay::game::decision::DecisionId(7),
        player_id: P0,
        kind: DecisionKind::InitCommand,
        lifetime: DecisionLifetime::OneShot,
    };
    let active = lifecycle.active_mut().expect("lifecycle should be active");
    active.next_decision_id = 8;
    active
        .game
        .players
        .get_mut(P0)
        .dev_cards_add(DevCardKind::Usable(UsableDevCard::YearOfPlenty));
    active.game.players.get_mut(P0).dev_cards_reset_queue();
    active.pending.push(decision.clone());
    let usage = DevCardUsage::YearOfPlenty([Resource::Brick, Resource::Wood]);

    let events = decider::decide(
        &lifecycle,
        submit(
            P0,
            decision.id(),
            PlayerCommand::InitCommand(crate::gameplay::game::command::InitCommand::UseDevCard(
                usage.clone(),
            )),
        ),
    );

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::DevCardUsed {
                player_id: P0,
                usage: emitted,
            },
            GameEvent::DecisionOpened(OpenDecision {
                id,
                player_id: P0,
                kind: DecisionKind::PostDevCardCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        ] if *decision_id == decision.id() && *emitted == usage && id.0 == 8
    ));
}

#[test]
fn start_opens_one_shot_init_decision() {
    let (_engine, outputs) = started_engine();

    let decision = first_open_decision(&outputs);

    assert_eq!(decision.player_id(), 0);
    assert!(matches!(decision.kind(), DecisionKind::InitialPlacement));
}

#[test]
fn start_updates_reducer_lifecycle_mirror() {
    let (engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);

    let active = engine
        .lifecycle()
        .as_setup()
        .expect("started engine should have setup lifecycle");

    assert!(active.pending.get(decision.id()).is_some());
    assert_eq!(active.next_decision_id, decision.id().0 + 1);
}

#[test]
fn start_event_outputs_preserve_domain_event_order() {
    let (_engine, outputs) = started_engine();
    let events = outputs
        .iter()
        .filter_map(|output| match output {
            GameOutput::Event(record) => Some(&record.event),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::GameStarted,
            GameEvent::DecisionOpened(OpenDecision {
                player_id: P0,
                kind: DecisionKind::InitialPlacement,
                ..
            }),
        ]
    ));
}

#[test]
fn start_emits_domain_decision_opened_event() {
    let (_engine, outputs) = started_engine();

    assert!(outputs.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::DecisionOpened(decision))
                if decision.player_id() == 0 && matches!(decision.kind(), DecisionKind::InitialPlacement)
        )
    }));
}

#[test]
fn characterization_start_event_order_is_stable() {
    let (_engine, outputs) = started_engine();
    let events = output_events(&outputs);

    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::GameStarted,
            GameEvent::DecisionOpened(OpenDecision {
                player_id: P0,
                kind: DecisionKind::InitialPlacement,
                ..
            }),
        ]
    ));
}

#[test]
fn characterization_bank_trade_event_follows_decision_close() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    engine.test_give_resources(
        0,
        ResourceSet {
            brick: 4,
            ..ResourceSet::EMPTY
        },
    );
    let decision = engine.open_decision_for_test(0, DecisionKind::RegularCommand);
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(RegularCommand::TradeWithBank(BankTrade {
                kind: BankTradeKind::BankGeneric,
                give: Resource::Brick,
                take: Resource::Wood,
            })),
        ),
        &mut sink,
    );

    let events = output_events(sink.as_slice());
    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id },
            GameEvent::BankTradeCompleted { player_id: P0, .. },
            GameEvent::DecisionOpened(OpenDecision {
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                ..
            }),
        ] if *decision_id == decision.id()
    ));
}

#[test]
fn characterization_trade_commit_event_order_closes_session_then_reopens_regular_decision() {
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
    let peer = engine.open_decision_for_test(1, DecisionKind::TradeResponse { session });
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            owner.id,
            PlayerCommand::Trade(TradeCommand::Commit { offer_id: offer }),
        ),
        &mut sink,
    );

    let events = output_events(sink.as_slice());
    assert!(matches!(
        events.as_slice(),
        [
            GameEvent::DecisionClosed { decision_id: owner_closed },
            GameEvent::DecisionClosed { decision_id: peer_closed },
            GameEvent::TradeCompleted { session_id, proposer_id: P0, peer_id: P1, .. },
            GameEvent::DecisionOpened(OpenDecision {
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                ..
            }),
        ] if *owner_closed == owner.id && *peer_closed == peer.id && *session_id == session
    ));
    assert!(sink.as_slice().iter().any(|output| {
        matches!(output, GameOutput::DecisionClosed { decision_id } if *decision_id == owner.id)
    }));
    assert!(sink.as_slice().iter().any(|output| {
        matches!(output, GameOutput::DecisionClosed { decision_id } if *decision_id == peer.id)
    }));
}

#[test]
fn wrong_player_is_rejected_without_closing_decision() {
    let (mut engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);
    let mut sink = Vec::new();

    let status = apply_to_sink(
        &mut engine,
        submit(
            P1,
            decision.id(),
            PlayerCommand::MoveRobber(crate::gameplay::game::command::MoveRobberCommand(Hex::new(
                0, 0,
            ))),
        ),
        &mut sink,
    );

    assert!(status.is_none());
    assert!(sink.iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                token,
                reason: CommandRejectionReason::WrongPlayer { expected: P0 },
            } if token.player_id == P1 && token.id == decision.id()
        )
    }));
}

#[test]
fn wrong_player_rejection_emits_domain_command_rejected_event() {
    let (mut engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P1,
            decision.id(),
            PlayerCommand::MoveRobber(crate::gameplay::game::command::MoveRobberCommand(Hex::new(
                0, 0,
            ))),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::CommandRejected {
                token,
                reason: CommandRejectionReason::WrongPlayer { expected: P0 },
                counts_toward_limit: false,
            }) if token.player_id == P1 && token.id == decision.id()
        )
    }));
}

#[test]
fn stale_decision_is_rejected_after_one_shot_closes() {
    let (mut engine, outputs) = started_engine();
    let decision = first_open_decision(&outputs);
    let placement = engine
        .legal_initial_placements(P0)
        .into_iter()
        .next()
        .expect("default board should have an initial placement");
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            decision.id(),
            PlayerCommand::InitialPlacement(placement),
        ),
        &mut sink,
    );
    let mut stale_sink = Vec::new();
    apply_to_sink(
        &mut engine,
        submit(
            P0,
            decision.id(),
            PlayerCommand::InitialPlacement(placement),
        ),
        &mut stale_sink,
    );

    assert!(stale_sink.iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                token,
                reason: CommandRejectionReason::StaleDecision,
            } if token.player_id == P0 && token.id == decision.id()
        )
    }));
}

#[test]
fn buying_dev_card_emits_private_drawn_card_event() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    engine.test_give_resources(
        0,
        ResourceSet {
            wheat: 1,
            sheep: 1,
            ore: 1,
            ..ResourceSet::EMPTY
        },
    );
    engine
        .core
        .active_mut()
        .expect("test engine should be active")
        .game
        .bank
        .dev_cards = vec![DevCardKind::VictoryPoint];
    let decision = engine.open_decision_for_test(0, DecisionKind::RegularCommand);
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(crate::gameplay::game::command::RegularCommand::BuyDevCard),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::DevCardDrawn {
                player_id: P0,
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
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            decision.id(),
            PlayerCommand::MoveRobber(crate::gameplay::game::command::MoveRobberCommand(
                victim_hex,
            )),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::ResourceStolen {
                player_id: P0,
                robbed_id: P1,
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

    let mut sink = Vec::new();
    let owner_decision = engine.open_decision_for_test(0, DecisionKind::RegularCommand);
    apply_to_sink(
        &mut engine,
        submit(
            P0,
            owner_decision.id(),
            PlayerCommand::Trade(TradeCommand::Propose {
                scope: TradeScope::Public,
                offer: PublicTradeOffer {
                    give: one_brick(),
                    take: one_wood(),
                },
            }),
        ),
        &mut sink,
    );

    let response_decision = sink
        .as_slice()
        .iter()
        .find_map(|output| match output {
            GameOutput::DecisionOpened(decision)
                if decision.player_id() == 1
                    && matches!(decision.kind(), DecisionKind::TradeResponse { .. }) =>
            {
                Some(decision.clone())
            }
            _ => None,
        })
        .expect("trade should open response decision for peer");

    let mut update_sink = Vec::new();
    apply_to_sink(
        &mut engine,
        submit(
            P1,
            response_decision.id(),
            PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Accept {
                offer_id: 0.into(),
            })),
        ),
        &mut update_sink,
    );
    apply_to_sink(
        &mut engine,
        submit(
            P1,
            response_decision.id(),
            PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Reject)),
        ),
        &mut update_sink,
    );

    let updates = update_sink
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
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            owner.id,
            PlayerCommand::Trade(TradeCommand::Commit { offer_id: offer }),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                token,
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            } if token.player_id == P0
        )
    }));
}

#[test]
fn same_resource_on_both_sides_is_rejected() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let owner = engine.open_decision_for_test(0, DecisionKind::RegularCommand);
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            owner.id,
            PlayerCommand::Trade(TradeCommand::Propose {
                scope: TradeScope::Public,
                offer: PublicTradeOffer {
                    give: one_brick(),
                    take: one_brick(),
                },
            }),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
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
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P1,
            response.id,
            PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Reject)),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: P1,
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
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P1,
            response.id,
            PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Counter {
                offer: PlayerTrade {
                    give: one_wood(),
                    take: one_brick(),
                },
            })),
        ),
        &mut sink,
    );

    let outputs = sink;
    assert!(outputs.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::TradeOfferAdded {
                session_id,
                player_id: P1,
                ..
            }) if *session_id == session
        )
    }));
    assert!(outputs.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::TradeResponseUpdated {
                session_id,
                player_id: P1,
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
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            owner.id,
            PlayerCommand::Trade(TradeCommand::Commit { offer_id: offer }),
        ),
        &mut sink,
    );

    assert_eq!(*engine.state().players.get(0).resources(), one_wood());
    assert_eq!(*engine.state().players.get(1).resources(), one_brick());
    assert!(sink.iter().any(|output| {
        matches!(
            output_event(output),
            Some(GameEvent::TradeCompleted {
                session_id,
                proposer_id: P0,
                peer_id: P1,
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
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(P0, owner.id, PlayerCommand::Trade(TradeCommand::Cancel)),
        &mut sink,
    );

    let outputs = sink;
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
                proposer_id: P0,
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
    let mut counter_sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P1,
            countering_player.id,
            PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Counter {
                offer: PlayerTrade {
                    give: one_wood(),
                    take: one_brick(),
                },
            })),
        ),
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
    let mut accept_sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P2,
            other_player.id,
            PlayerCommand::Trade(TradeCommand::Respond(TradeResponseCommand::Accept {
                offer_id: counter_offer_id,
            })),
        ),
        &mut accept_sink,
    );

    assert!(accept_sink.iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                token,
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            } if token.player_id == P2
        )
    }));
}

#[test]
fn targeted_trade_rejects_invalid_target() {
    let (mut engine, _outputs) = started_engine();
    engine.test_force_regular_action_phase(0);
    let owner = engine.open_decision_for_test(0, DecisionKind::RegularCommand);
    let mut sink = Vec::new();

    apply_to_sink(
        &mut engine,
        submit(
            P0,
            owner.id,
            PlayerCommand::Trade(TradeCommand::Propose {
                scope: TradeScope::Targeted(P99),
                offer: PublicTradeOffer {
                    give: one_brick(),
                    take: one_wood(),
                },
            }),
        ),
        &mut sink,
    );

    assert!(sink.iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                token,
                reason: CommandRejectionReason::IllegalCommand(_),
                ..
            } if token.player_id == P0
        )
    }));
}

#[test]
fn submit_after_game_end_is_rejected_without_mutation() {
    let (mut engine, _outputs) = started_engine();
    let decision = engine.open_decision_for_test(0, DecisionKind::RegularCommand);
    engine.test_mark_ended();
    let mut sink = Vec::new();

    let status = apply_to_sink(
        &mut engine,
        submit(
            P0,
            decision.id(),
            PlayerCommand::Regular(crate::gameplay::game::command::RegularCommand::EndMove),
        ),
        &mut sink,
    );

    assert!(matches!(status, Some(GameResult::Interrupted { .. })));
    assert!(sink.iter().any(|output| {
        matches!(
            output,
            GameOutput::CommandRejected {
                token,
                reason: CommandRejectionReason::GameEnded,
                ..
            } if token.player_id == P0
        )
    }));
}
