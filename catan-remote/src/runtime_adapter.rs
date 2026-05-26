use std::{io, os::unix::net::UnixStream};

use catan_core::gameplay::game::projection::GameProjection;
use catan_core::gameplay::{
    game::{
        decision::DecisionKind,
        event::ObserverNotificationContext,
        output::GameOutput,
        view::{ContextFactory, PlayerDecisionContext},
    },
    primitives::player::PlayerId,
};

use catan_runtime::snapshot::SnapshotStore;
use catan_runtime::sync_host::{
    ObserverFrame, OutputObserver, Seat, SeatCommand, SeatCommandBuffer, SeatFrame,
};

use crate::{
    frame::{NonblockingFrameReader, read_frame, write_frame},
    protocol::{ClientMessage, HostMessage, LegalDecisionOptions, RemoteRole},
};

pub struct RemoteCliSeat {
    player_id: PlayerId,
    stream: UnixStream,
}

impl RemoteCliSeat {
    pub fn new(player_id: impl Into<PlayerId>, mut stream: UnixStream) -> io::Result<Self> {
        let player_id = player_id.into();
        write_frame(
            &mut stream,
            &HostMessage::Hello {
                role: RemoteRole::Player { player_id },
            },
        )?;
        expect_ready(&mut stream)?;
        Ok(Self { player_id, stream })
    }
}

impl Seat for RemoteCliSeat {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn on_frame(&mut self, frame: SeatFrame<'_>, commands: &mut SeatCommandBuffer) {
        if let GameOutput::DecisionOpened(decision) = frame.output
            && decision.player_id() != self.player_id
        {
            return;
        }

        let (output, view, legal) =
            player_frame(frame.output, &frame.view, frame.dev_card_used_this_turn);
        if write_frame(
            &mut self.stream,
            &HostMessage::Output {
                output: output.clone(),
                view: Box::new(view),
                legal,
            },
        )
        .is_err()
        {
            return;
        }

        let GameOutput::DecisionOpened(decision) = output else {
            return;
        };
        if decision.player_id() != self.player_id {
            return;
        }

        loop {
            match read_frame::<ClientMessage>(&mut self.stream) {
                Ok(ClientMessage::SubmitCommand {
                    player_id,
                    decision_id: _,
                    command,
                }) => {
                    if player_id == self.player_id
                        && let Some(response) = decision.respond_command(command)
                    {
                        commands.push(SeatCommand { response });
                    }
                    return;
                }
                Ok(ClientMessage::Error { message }) => {
                    log::warn!(target: "catan_runtime::remote_seat", "remote CLI error: {message}");
                    return;
                }
                Ok(ClientMessage::Log {
                    level,
                    target,
                    message,
                }) => {
                    let level = log::Level::from(level);
                    for line in message.lines().filter(|line| !line.trim().is_empty()) {
                        log::log!(target: &target, level, "{line}");
                    }
                }
                Ok(other) => {
                    log::warn!(target: "catan_runtime::remote_seat", "unexpected CLI frame: {other:?}");
                    return;
                }
                Err(err) => {
                    log::warn!(target: "catan_runtime::remote_seat", "failed to read CLI command: {err}");
                    return;
                }
            }
        }
    }
}

pub struct RemoteCliOutputObserver {
    role: RemoteRole,
    stream: UnixStream,
    reader: NonblockingFrameReader<ClientMessage>,
    snapshot_store: Option<SnapshotStore>,
}

impl RemoteCliOutputObserver {
    pub fn new(role: RemoteRole, mut stream: UnixStream) -> io::Result<Self> {
        if !role.is_observer() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote CLI output observer requires an observer role",
            ));
        }
        write_frame(&mut stream, &HostMessage::Hello { role: role.clone() })?;
        expect_ready(&mut stream)?;
        let snapshot_store = if role.includes_exact_snapshot_state() {
            Some(SnapshotStore::new()?)
        } else {
            None
        };
        Ok(Self {
            role,
            stream,
            reader: NonblockingFrameReader::default(),
            snapshot_store,
        })
    }
}

impl OutputObserver for RemoteCliOutputObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        let view = observer_model(&self.role, frame.factory);
        let _ = write_frame(
            &mut self.stream,
            &HostMessage::Output {
                output: frame.output.clone(),
                view: Box::new(view),
                legal: LegalDecisionOptions::default(),
            },
        );
        self.handle_control_messages(frame);
    }
}

impl RemoteCliOutputObserver {
    fn handle_control_messages(&mut self, frame: ObserverFrame<'_>) {
        if let Err(err) = self.stream.set_nonblocking(true) {
            log::warn!(target: "catan_runtime::remote_seat", "failed to poll observer control frames: {err}");
            return;
        }
        let mut messages = Vec::new();
        loop {
            match self.reader.poll(&mut self.stream) {
                Ok(Some(message)) => messages.push(message),
                Ok(None) => break,
                Err(err) => {
                    log::warn!(target: "catan_runtime::remote_seat", "failed to read observer control frame: {err}");
                    break;
                }
            };
        }
        if let Err(err) = self.stream.set_nonblocking(false) {
            log::warn!(target: "catan_runtime::remote_seat", "failed to restore observer stream blocking mode: {err}");
            return;
        }
        for message in messages {
            match message {
                ClientMessage::SaveSnapshot => self.save_snapshot(frame.engine),
                ClientMessage::Log {
                    level,
                    target,
                    message,
                } => {
                    let level = log::Level::from(level);
                    for line in message.lines().filter(|line| !line.trim().is_empty()) {
                        log::log!(target: &target, level, "{line}");
                    }
                }
                ClientMessage::Error { message } => {
                    log::warn!(target: "catan_runtime::remote_seat", "remote observer error: {message}");
                }
                other => {
                    log::warn!(target: "catan_runtime::remote_seat", "unexpected observer control frame: {other:?}");
                }
            }
        }
    }

    fn save_snapshot(&mut self, engine: &catan_core::gameplay::game::engine::GameEngine) {
        let Some(store) = self.snapshot_store.as_mut() else {
            let _ = write_frame(
                &mut self.stream,
                &HostMessage::SnapshotFailed {
                    reason: "snapshot store is not available for this observer".to_owned(),
                },
            );
            return;
        };
        match store.write_checkpoint(engine) {
            Ok(path) => {
                let _ = write_frame(
                    &mut self.stream,
                    &HostMessage::SnapshotSaved {
                        path: path.display().to_string(),
                    },
                );
            }
            Err(err) => {
                let _ = write_frame(
                    &mut self.stream,
                    &HostMessage::SnapshotFailed {
                        reason: err.to_string(),
                    },
                );
            }
        }
    }
}

fn player_frame(
    output: &GameOutput,
    context: &PlayerDecisionContext<'_>,
    dev_card_used_this_turn: bool,
) -> (GameOutput, GameProjection, LegalDecisionOptions) {
    let robber_pos = match output {
        GameOutput::DecisionOpened(decision) => match decision.kind() {
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(robber_pos),
            _ => None,
        },
        _ => None,
    };
    let mut legal = LegalDecisionOptions::from_context(context, robber_pos);
    legal.dev_card_used_this_turn = dev_card_used_this_turn;
    (
        output.clone(),
        GameProjection::from_decision(context),
        legal,
    )
}

fn observer_model(role: &RemoteRole, factory: &ContextFactory<'_>) -> GameProjection {
    match role {
        RemoteRole::Spectator => GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        ),
        RemoteRole::PlayerObserver { player_id } => GameProjection::from_observer(
            ObserverNotificationContext::Player {
                public: factory.public_view(factory.visibility.player_policy(*player_id)),
                private: factory.private_view(*player_id),
            },
            false,
        ),
        RemoteRole::Omniscient | RemoteRole::SnapshotObserver => GameProjection::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            role.includes_exact_snapshot_state(),
        ),
        RemoteRole::Player { .. } => GameProjection::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        ),
    }
}

fn expect_ready(stream: &mut UnixStream) -> io::Result<()> {
    match read_frame::<ClientMessage>(stream)? {
        ClientMessage::Ready => Ok(()),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected ready, got {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use crate::{
        frame::{read_frame, write_frame},
        protocol::{ClientMessage, HostMessage},
    };
    use catan_core::gameplay::game::{
        decision::{DecisionId, DecisionKind, DecisionLifetime, OpenDecision},
        input::DecisionToken,
        output::{CommandRejectionReason, GameOutput},
        run::RunOptions,
        state::SetupGameState,
    };

    use super::*;

    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);

    #[test]
    fn remote_cli_seat_queues_submit_command_from_child() {
        let (host_stream, mut child_stream) = UnixStream::pair().unwrap();
        let child = std::thread::spawn(move || {
            assert!(matches!(
                read_frame::<HostMessage>(&mut child_stream).unwrap(),
                HostMessage::Hello {
                    role: RemoteRole::Player { player_id: P0 }
                }
            ));
            write_frame(&mut child_stream, &ClientMessage::Ready).unwrap();
            let HostMessage::Output { output, .. } =
                read_frame::<HostMessage>(&mut child_stream).unwrap()
            else {
                panic!("expected output frame");
            };
            let GameOutput::DecisionOpened(decision) = output else {
                panic!("expected decision output");
            };
            write_frame(
                &mut child_stream,
                &ClientMessage::SubmitCommand {
                    player_id: decision.player_id(),
                    decision_id: decision.id(),
                    command: catan_core::gameplay::game::input::PlayerCommand::Regular(
                        catan_core::gameplay::game::command::RegularCommand::EndMove,
                    ),
                },
            )
            .unwrap();
        });

        let mut seat = RemoteCliSeat::new(0, host_stream).unwrap();
        let mut commands = SeatCommandBuffer::default();
        let init = catan_core::gameplay::game::state::SetupGameState::default();
        let state = init.finish();
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = catan_core::gameplay::game::view::VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let view = factory.player_decision_context(0, None);
        let output = GameOutput::DecisionOpened(
            catan_core::gameplay::game::input::DecisionRequest::from_open_decision(&OpenDecision {
                id: catan_core::gameplay::game::decision::DecisionId(7),
                player_id: P0,
                kind: DecisionKind::RegularCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        );

        seat.on_frame(
            SeatFrame {
                player_id: P0,
                output: &output,
                view,
                dev_card_used_this_turn: false,
            },
            &mut commands,
        );

        assert_eq!(commands.len(), 1);
        child.join().unwrap();
    }

    #[test]
    fn remote_cli_seat_does_not_send_other_player_decisions() {
        let (host_stream, mut child_stream) = UnixStream::pair().unwrap();
        child_stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        let child = std::thread::spawn(move || {
            assert!(matches!(
                read_frame::<HostMessage>(&mut child_stream).unwrap(),
                HostMessage::Hello {
                    role: RemoteRole::Player { player_id: P0 }
                }
            ));
            write_frame(&mut child_stream, &ClientMessage::Ready).unwrap();
            read_frame::<HostMessage>(&mut child_stream)
        });

        let mut seat = RemoteCliSeat::new(0, host_stream).unwrap();
        let mut commands = SeatCommandBuffer::default();
        let init = catan_core::gameplay::game::state::SetupGameState::default();
        let state = init.finish();
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = catan_core::gameplay::game::view::VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
            trade_sessions: &[],
        };
        let view = factory.player_decision_context(0, None);
        let output = GameOutput::DecisionOpened(
            catan_core::gameplay::game::input::DecisionRequest::from_open_decision(&OpenDecision {
                id: catan_core::gameplay::game::decision::DecisionId(8),
                player_id: P1,
                kind: DecisionKind::RegularCommand,
                lifetime: DecisionLifetime::OneShot,
            }),
        );

        seat.on_frame(
            SeatFrame {
                player_id: P0,
                output: &output,
                view,
                dev_card_used_this_turn: false,
            },
            &mut commands,
        );

        assert!(commands.is_empty());
        assert!(matches!(
            child.join().unwrap(),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock
                || err.kind() == std::io::ErrorKind::TimedOut
        ));
    }

    #[test]
    fn remote_output_observer_streams_large_frames_when_reader_lags() {
        let (host_stream, mut child_stream) = UnixStream::pair().unwrap();
        let child = std::thread::spawn(move || -> std::io::Result<()> {
            assert!(matches!(
                read_frame::<HostMessage>(&mut child_stream)?,
                HostMessage::Hello {
                    role: RemoteRole::SnapshotObserver
                }
            ));
            write_frame(&mut child_stream, &ClientMessage::Ready)?;
            std::thread::sleep(std::time::Duration::from_millis(50));

            for _ in 0..2 {
                let frame = read_frame::<HostMessage>(&mut child_stream)?;
                assert!(matches!(frame, HostMessage::Output { .. }));
            }
            Ok(())
        });
        let mut observer =
            RemoteCliOutputObserver::new(RemoteRole::SnapshotObserver, host_stream).unwrap();
        let engine = catan_core::gameplay::game::engine::GameEngine::from_init(
            SetupGameState::default(),
            RunOptions::default(),
        );
        let visibility = catan_core::gameplay::game::view::VisibilityConfig::default();
        let factory = ContextFactory {
            state: engine.table(),
            index: engine.index(),
            visibility: &visibility,
            trade_sessions: &[],
        };
        let output = GameOutput::CommandRejected {
            token: DecisionToken {
                id: DecisionId(0),
                player_id: P0,
            },
            reason: CommandRejectionReason::IllegalCommand("x".repeat(2 * 1024 * 1024)),
        };

        for _ in 0..2 {
            observer.on_output(ObserverFrame {
                output: &output,
                factory: &factory,
                engine: &engine,
            });
        }

        drop(observer);
        child.join().unwrap().unwrap();
    }
}
