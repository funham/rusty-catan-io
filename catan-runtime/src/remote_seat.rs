use std::{io, os::unix::net::UnixStream};

use catan_agents::remote_agent::{
    CliRole, CliToHost, HostToCli, LegalDecisionOptions, UiModel, read_frame, write_frame,
};
use catan_core::gameplay::{
    game::{
        decision::DecisionKind,
        event::ObserverNotificationContext,
        output::GameOutput,
        view::{ContextFactory, PlayerDecisionContext},
    },
    primitives::player::PlayerId,
};

use crate::sync_host::{
    ObserverFrame, OutputObserver, Seat, SeatCommand, SeatCommandBuffer, SeatFrame,
};

pub struct RemoteCliSeat {
    player_id: PlayerId,
    stream: UnixStream,
}

impl RemoteCliSeat {
    pub fn new(player_id: PlayerId, mut stream: UnixStream) -> io::Result<Self> {
        write_frame(
            &mut stream,
            &HostToCli::Hello {
                role: CliRole::Player { player_id },
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
            && decision.player_id != self.player_id
        {
            return;
        }

        let (output, view, legal) = player_frame(frame.output, &frame.view);
        if write_frame(
            &mut self.stream,
            &HostToCli::Output {
                output: output.clone(),
                view,
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
        if decision.player_id != self.player_id {
            return;
        }

        loop {
            match read_frame::<CliToHost>(&mut self.stream) {
                Ok(CliToHost::SubmitCommand {
                    player_id,
                    decision_id,
                    command,
                }) => {
                    commands.push(SeatCommand {
                        player_id,
                        decision_id,
                        command,
                    });
                    return;
                }
                Ok(CliToHost::Error { message }) => {
                    log::warn!(target: "catan_runtime::remote_seat", "remote CLI error: {message}");
                    return;
                }
                Ok(CliToHost::Log {
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
    role: CliRole,
    stream: UnixStream,
}

impl RemoteCliOutputObserver {
    pub fn new(role: CliRole, mut stream: UnixStream) -> io::Result<Self> {
        if !role.is_observer() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote CLI output observer requires an observer role",
            ));
        }
        write_frame(&mut stream, &HostToCli::Hello { role: role.clone() })?;
        expect_ready(&mut stream)?;
        Ok(Self { role, stream })
    }
}

impl OutputObserver for RemoteCliOutputObserver {
    fn on_output(&mut self, frame: ObserverFrame<'_>) {
        let view = observer_model(&self.role, &frame.factory);
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Output {
                output: frame.output.clone(),
                view,
                legal: LegalDecisionOptions::default(),
            },
        );
    }
}

fn player_frame(
    output: &GameOutput,
    context: &PlayerDecisionContext<'_>,
) -> (GameOutput, UiModel, LegalDecisionOptions) {
    let robber_pos = match output {
        GameOutput::DecisionOpened(decision) => match decision.kind {
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(robber_pos),
            _ => None,
        },
        _ => None,
    };
    (
        output.clone(),
        UiModel::from_decision(context),
        LegalDecisionOptions::from_context(context, robber_pos),
    )
}

fn observer_model(role: &CliRole, factory: &ContextFactory<'_>) -> UiModel {
    match role {
        CliRole::Spectator => UiModel::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        ),
        CliRole::PlayerObserver { player_id } => UiModel::from_observer(
            ObserverNotificationContext::Player {
                public: factory.public_view(factory.visibility.player_policy(*player_id)),
                private: factory.private_view(*player_id),
            },
            false,
        ),
        CliRole::Omniscient | CliRole::SnapshotObserver => UiModel::from_observer(
            ObserverNotificationContext::Omniscient {
                public: factory.spectator_public_view(),
                full: factory.omniscient_view(),
            },
            role.includes_exact_snapshot_state(),
        ),
        CliRole::Player { .. } => UiModel::from_observer(
            ObserverNotificationContext::Spectator {
                public: factory.spectator_public_view(),
            },
            false,
        ),
    }
}

fn expect_ready(stream: &mut UnixStream) -> io::Result<()> {
    match read_frame::<CliToHost>(stream)? {
        CliToHost::Ready => Ok(()),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected ready, got {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use catan_agents::remote_agent::{CliToHost, HostToCli, read_frame, write_frame};
    use catan_core::gameplay::game::{
        decision::{DecisionKind, DecisionLifetime, OpenDecision},
        output::GameOutput,
    };

    use super::*;

    #[test]
    fn remote_cli_seat_queues_submit_command_from_child() {
        let (host_stream, mut child_stream) = UnixStream::pair().unwrap();
        let child = std::thread::spawn(move || {
            assert!(matches!(
                read_frame::<HostToCli>(&mut child_stream).unwrap(),
                HostToCli::Hello {
                    role: CliRole::Player { player_id: 0 }
                }
            ));
            write_frame(&mut child_stream, &CliToHost::Ready).unwrap();
            let HostToCli::Output { output, .. } =
                read_frame::<HostToCli>(&mut child_stream).unwrap()
            else {
                panic!("expected output frame");
            };
            let GameOutput::DecisionOpened(decision) = output else {
                panic!("expected decision output");
            };
            write_frame(
                &mut child_stream,
                &CliToHost::SubmitCommand {
                    player_id: decision.player_id,
                    decision_id: decision.id,
                    command: catan_core::gameplay::game::input::PlayerCommand::Regular(
                        catan_core::gameplay::game::action::RegularAction::EndMove,
                    ),
                },
            )
            .unwrap();
        });

        let mut seat = RemoteCliSeat::new(0, host_stream).unwrap();
        let mut commands = SeatCommandBuffer::default();
        let init = catan_core::gameplay::game::init::GameInitializationState::default();
        let state = init.finish();
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = catan_core::gameplay::game::view::VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let view = factory.player_decision_context(0, None);
        let output = GameOutput::DecisionOpened(OpenDecision {
            id: catan_core::gameplay::game::decision::DecisionId(7),
            player_id: 0,
            kind: DecisionKind::RegularAction,
            lifetime: DecisionLifetime::OneShot,
        });

        seat.on_frame(
            SeatFrame {
                player_id: 0,
                output: &output,
                view,
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
                read_frame::<HostToCli>(&mut child_stream).unwrap(),
                HostToCli::Hello {
                    role: CliRole::Player { player_id: 0 }
                }
            ));
            write_frame(&mut child_stream, &CliToHost::Ready).unwrap();
            read_frame::<HostToCli>(&mut child_stream)
        });

        let mut seat = RemoteCliSeat::new(0, host_stream).unwrap();
        let mut commands = SeatCommandBuffer::default();
        let init = catan_core::gameplay::game::init::GameInitializationState::default();
        let state = init.finish();
        let index = catan_core::gameplay::game::index::GameIndex::rebuild(&state);
        let visibility = catan_core::gameplay::game::view::VisibilityConfig::default();
        let factory = ContextFactory {
            state: &state,
            index: &index,
            visibility: &visibility,
        };
        let view = factory.player_decision_context(0, None);
        let output = GameOutput::DecisionOpened(OpenDecision {
            id: catan_core::gameplay::game::decision::DecisionId(8),
            player_id: 1,
            kind: DecisionKind::RegularAction,
            lifetime: DecisionLifetime::OneShot,
        });

        seat.on_frame(
            SeatFrame {
                player_id: 0,
                output: &output,
                view,
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
}
