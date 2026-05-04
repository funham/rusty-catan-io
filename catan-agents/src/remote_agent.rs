mod client;
mod model;
mod protocol;

pub use client::{RemoteCliAgent, RemoteCliObserver};
pub use model::{
    UiBoard, UiModel, UiOmniscient, UiPlayerBuilds, UiPrivatePlayer, UiPublicBank,
    UiPublicBankResources, UiPublicGame, UiPublicPlayer, UiPublicPlayerResources, ui_model_summary,
};
pub use protocol::{
    CliRole, CliToHost, DecisionRequestEnvelope, DecisionRequestFrame, DecisionResponseFrame,
    HostToCli, LegalBuildOptions, LegalDecisionOptions, MAX_FRAME_LEN, NonblockingFrameReader,
    RemoteLogLevel, read_frame, write_frame,
};

#[cfg(test)]
mod tests {
    use super::{
        CliRole, CliToHost, HostToCli, LegalDecisionOptions, NonblockingFrameReader,
        RemoteCliObserver, RemoteLogLevel, UiBoard, UiModel, read_frame, write_frame,
    };
    use catan_core::gameplay::{
        game::{
            event::{GameEvent, GameObserver, ObserverKind, ObserverNotificationContext},
            index::GameIndex,
            init::GameInitializationState,
            view::{ContextFactory, SearchFactory, VisibilityConfig},
        },
        primitives::{
            dev_card::{DevCardKind, DevCardUsage, UsableDevCard},
            resource::{Resource, ResourceCollection},
        },
    };

    #[test]
    fn frame_round_trip() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"hello").unwrap();
        let value: String = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(value, "hello");
    }

    #[test]
    fn nonblocking_frame_reader_round_trip() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"hello").unwrap();
        let mut reader = NonblockingFrameReader::<String>::default();
        let value = reader.poll(&mut bytes.as_slice()).unwrap();
        assert_eq!(value.as_deref(), Some("hello"));
    }

    #[test]
    fn log_frame_round_trip() {
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &CliToHost::Log {
                level: RemoteLogLevel::Trace,
                target: "child_target".to_owned(),
                message: "child log".to_owned(),
            },
        )
        .unwrap();
        let value: CliToHost = read_frame(&mut bytes.as_slice()).unwrap();
        assert!(matches!(
            value,
            CliToHost::Log {
                level: RemoteLogLevel::Trace,
                target,
                message,
            } if target == "child_target" && message == "child log"
        ));
    }

    #[test]
    fn rejects_large_frame() {
        let bytes = ((super::MAX_FRAME_LEN + 1) as u32).to_be_bytes().to_vec();
        let err = read_frame::<String>(&mut bytes.as_slice()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn nonblocking_reader_rejects_large_frame() {
        let bytes = ((super::MAX_FRAME_LEN + 1) as u32).to_be_bytes().to_vec();
        let mut reader = NonblockingFrameReader::<String>::default();
        let err = reader.poll(&mut bytes.as_slice()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn cli_role_helpers_describe_observer_and_snapshot_behavior() {
        assert!(!CliRole::Player { player_id: 0 }.is_observer());
        assert!(CliRole::SnapshotObserver.is_observer());
        assert_eq!(
            CliRole::PlayerObserver { player_id: 2 }.observer_kind(),
            Some(ObserverKind::Player(2))
        );
        assert_eq!(
            CliRole::SnapshotObserver.observer_kind(),
            Some(ObserverKind::Omniscient)
        );
        assert!(CliRole::SnapshotObserver.includes_exact_snapshot_state());
        assert!(!CliRole::Omniscient.includes_exact_snapshot_state());
        assert_eq!(CliRole::SnapshotObserver.label(), "snapshot-observer");
        assert_eq!(CliRole::SnapshotObserver.socket_abbrev(), "snap");
    }

    #[test]
    fn ui_board_serializes_to_json() {
        let state = GameInitializationState::default().finish();
        let board = UiBoard::from_board(&state.board);
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let model = UiModel::from_decision(&factory.player_decision_context(0, None));
        let msg = HostToCli::Hello {
            role: CliRole::Spectator,
        };

        serde_json::to_vec(&board).unwrap();
        serde_json::to_vec(&model).unwrap();
        serde_json::to_vec(&msg).unwrap();
    }

    #[test]
    fn legal_options_attach_regular_trades_and_dev_card_usages() {
        let mut state = GameInitializationState::default().finish();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 4,
                    wheat: 1,
                    sheep: 1,
                    ore: 1,
                    ..ResourceCollection::ZERO
                },
                0,
            )
            .expect("bank should fund test player");
        state
            .players
            .get_mut(0)
            .dev_cards_add(DevCardKind::Usable(UsableDevCard::YearOfPlenty));
        state.players.get_mut(0).dev_cards_reset_queue();

        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);
        let legal = LegalDecisionOptions::from_context(&context, None);

        assert!(!legal.initial_placements.is_empty());
        assert!(!legal.regular_actions.is_empty());
        assert!(
            legal
                .bank_trades
                .iter()
                .any(|trade| trade.give == Resource::Brick)
        );
        assert!(
            legal
                .dev_card_usages
                .iter()
                .any(|usage| { matches!(usage, DevCardUsage::YearOfPlenty([_, _])) })
        );
    }

    #[test]
    fn legal_options_attach_explicit_robber_context() {
        let mut init = GameInitializationState::default();
        let mut victim_hex = None;
        for player_id in 0..2 {
            let (settlement, road) = init
                .builds
                .query()
                .possible_initial_placements(&init.board, player_id)
                .into_iter()
                .next()
                .expect("default board should have initial placements");
            if player_id == 1 {
                let board_hexes = init.board.arrangement.hex_iter().collect::<Vec<_>>();
                victim_hex =
                    settlement.pos.as_set().into_iter().find(|hex| {
                        *hex != init.board_state.robber_pos && board_hexes.contains(hex)
                    });
            }
            init.builds
                .try_init_place(player_id, road, settlement)
                .expect("generated initial placement should be valid");
        }
        let mut state = init.finish();
        state
            .transfer_from_bank(Resource::Brick.into(), 1)
            .expect("bank should fund victim");
        let victim_hex = victim_hex.expect("victim should touch a non-robber hex");

        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let search = Some(SearchFactory::new(&state, visibility.player_policy(0), 0));
        let context = factory.player_decision_context(0, search);
        let legal = LegalDecisionOptions::from_context(&context, Some(victim_hex));

        assert_eq!(legal.robber_pos, Some(victim_hex));
        assert!(legal.rob_targets.contains(&1));
        assert!(!legal.robber_hexes.contains(&state.board_state.robber_pos));
    }

    #[test]
    fn snapshot_observer_model_includes_exact_state_only_when_requested() {
        let state = GameInitializationState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };

        let normal = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            false,
        );
        let snapshot = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );

        assert!(normal.omniscient.is_some());
        assert!(normal.snapshot_state.is_none());
        assert!(snapshot.snapshot_state.is_some());
        assert_eq!(
            snapshot.snapshot_state.unwrap().bank.dev_cards,
            state.bank.dev_cards
        );
    }

    #[test]
    fn snapshot_observer_event_frame_writes_with_exact_state() {
        let state = GameInitializationState::default().finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let model = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );
        let mut bytes = Vec::new();

        write_frame(
            &mut bytes,
            &HostToCli::Event {
                event: GameEvent::GameStarted,
                view: model,
            },
        )
        .unwrap();

        assert!(!bytes.is_empty());
    }

    #[test]
    fn snapshot_observer_event_frame_writes_with_built_exact_state() {
        let mut init = GameInitializationState::default();
        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .next()
            .expect("default board should have an initial placement");
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");

        let state = init.finish();
        let index = GameIndex::rebuild(&state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let model = UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            true,
        );
        let mut bytes = Vec::new();

        write_frame(
            &mut bytes,
            &HostToCli::Event {
                event: GameEvent::InitialPlacementBuilt {
                    player_id: 0,
                    settlement: settlement.pos,
                    road,
                },
                view: model,
            },
        )
        .unwrap();

        assert!(!bytes.is_empty());
    }

    #[test]
    fn remote_observer_streams_multiple_event_frames_without_responses() {
        let (host, mut child) = std::os::unix::net::UnixStream::pair().unwrap();
        child
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let writer = std::thread::spawn(move || {
            let mut observer =
                RemoteCliObserver::from_connected_role(CliRole::SnapshotObserver, host);

            let mut state = GameInitializationState::default().finish();
            let first_index = GameIndex::rebuild(&state);
            let visibility = VisibilityConfig::default();
            let first_factory = ContextFactory {
                state: &state,
                index: &first_index,
                visibility: &visibility,
            };
            observer.on_event(
                &GameEvent::GameStarted,
                ObserverNotificationContext::Omniscient {
                    public: first_factory.spectator_public_view(),
                    full: first_factory.omniscient_view(),
                },
            );

            state
                .transfer_from_bank(Resource::Brick.into(), 0)
                .expect("bank should fund player");
            let second_index = GameIndex::rebuild(&state);
            let second_factory = ContextFactory {
                state: &state,
                index: &second_index,
                visibility: &visibility,
            };
            observer.on_event(
                &GameEvent::ResourcesDistributed,
                ObserverNotificationContext::Omniscient {
                    public: second_factory.spectator_public_view(),
                    full: second_factory.omniscient_view(),
                },
            );
        });

        let first = read_frame::<HostToCli>(&mut child).unwrap();
        let second = read_frame::<HostToCli>(&mut child).unwrap();
        assert!(matches!(
            first,
            HostToCli::Event {
                event: GameEvent::GameStarted,
                ..
            }
        ));
        assert!(matches!(
            second,
            HostToCli::Event {
                event: GameEvent::ResourcesDistributed,
                ..
            }
        ));
        if let HostToCli::Event { view, .. } = second {
            assert_eq!(
                view.snapshot_state
                    .expect("snapshot role should include exact state")
                    .players
                    .get(0)
                    .resources()
                    .total(),
                1
            );
        }
        writer.join().unwrap();
    }
}
