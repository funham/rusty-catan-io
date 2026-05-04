//! Host session orchestration for the CLI child.
//!
//! Owns the Unix socket lifecycle, handshake, host-frame loop, decision dispatch,
//! shutdown handling, and request/response logging.

use std::{
    io::{self, Read},
    os::unix::net::UnixStream,
    path::Path as FsPath,
    time::Duration,
};

use catan_agents::remote_agent::{
    CliRole, CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli, read_frame,
    ui_model_summary, write_frame,
};
use catan_core::agent::action::{
    ChoosePlayerToRobAction, DropHalfAction, InitStageAction, MoveRobbersAction, PostDevCardAction,
    TradeAnswer,
};

use super::{
    input::{
        read_hex, read_init_action, read_initial_road, read_initial_settlement,
        read_post_dice_action, read_regular_action, read_resource_collection, read_robbed_player,
    },
    logging::init_socket_logger,
    snapshot::SnapshotWriter,
    tui::{CliDisplayMode, CliUi, SnapshotInput},
};

pub fn run(socket: &FsPath, log_socket: &FsPath, _role: &str) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;
    let log_stream = UnixStream::connect(log_socket).map_err(|err| {
        format!(
            "failed to connect log socket {}: {err}",
            log_socket.display()
        )
    })?;
    init_socket_logger(log_stream);
    log::trace!("Starting CLI with socket: {}", socket.display());
    let role = match read_frame::<HostToCli>(&mut stream)
        .map_err(|err| format!("failed to read hello: {err}"))?
    {
        HostToCli::Hello { role } => {
            log::trace!("Connected as role: {:?}", role);
            role
        }
        other => return Err(format!("expected hello, got {other:?}")),
    };
    let display_mode = match &role {
        CliRole::SnapshotObserver => CliDisplayMode::Snapshot,
        _ => CliDisplayMode::Normal,
    };
    let mut snapshot_writer = match &role {
        CliRole::SnapshotObserver => Some(
            SnapshotWriter::new()
                .map_err(|err| format!("failed to initialize snapshots: {err}"))?,
        ),
        _ => None,
    };
    let mut ui =
        CliUi::new(display_mode).map_err(|err| format!("failed to initialize TUI: {err}"))?;
    ui.set_message(format!("connected as {role:?}"))
        .map_err(|err| format!("failed to draw TUI: {err}"))?;
    write_frame(&mut stream, &CliToHost::Ready)
        .map_err(|err| format!("failed to send ready: {err}"))?;
    log::trace!("Sent ready message to host");

    if is_observer_role(&role) {
        return run_observer_loop(stream, ui, snapshot_writer);
    }

    loop {
        let msg = match read_frame::<HostToCli>(&mut stream) {
            Ok(msg) => msg,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                log::error!("Host closed socket unexpectedly");
                return Err(
                    "host closed the CLI socket without a shutdown message; check the host terminal for a panic or startup error"
                        .to_owned(),
                );
            }
            Err(err) => return Err(format!("failed to read host frame: {err}")),
        };
        log::trace!(target: "catan_runtime::cli_child::session", "received message from host");
        match msg {
            HostToCli::Hello { .. } => {
                log::trace!("Ignoring duplicate hello message");
            }
            HostToCli::Shutdown { reason } => {
                log::trace!("Shutdown received with reason: {}", reason);
                ui.set_message(format!("shutdown: {reason}"))
                    .map_err(|err| format!("failed to draw TUI: {err}"))?;
                return Ok(());
            }
            HostToCli::Event { event, view } => {
                log::trace!("Processing event: {:?}", event);
                if let (
                    CliDisplayMode::Normal,
                    catan_core::gameplay::game::event::GameEvent::GameEnded {
                        winner_id,
                        turn_no,
                        stats,
                    },
                ) = (display_mode, &event)
                {
                    ui.show_game_ended(&view, *winner_id, *turn_no, stats)
                        .map_err(|err| format!("failed to draw game ended screen: {err}"))?;
                    return Ok(());
                }
                ui.show_model(&view, format!("event: {event:?}"))
                    .map_err(|err| format!("failed to draw TUI: {err}"))?;
                if ui
                    .poll_snapshot_input()
                    .map_err(|err| format!("failed to read snapshot key: {err}"))?
                    == Some(SnapshotInput::Snapshot)
                {
                    let message = save_snapshot(snapshot_writer.as_mut(), &view);
                    ui.show_model(&view, message)
                        .map_err(|err| format!("failed to draw TUI: {err}"))?;
                }
            }
            HostToCli::DecisionRequest(request) => {
                if display_mode == CliDisplayMode::Snapshot {
                    let message = format!(
                        "snapshot observer received unexpected decision request: {request:?}"
                    );
                    log::warn!("{message}");
                    write_frame(&mut stream, &CliToHost::Error { message })
                        .map_err(|err| format!("failed to send observer error: {err}"))?;
                    continue;
                }
                log::trace!(
                    target: "catan_runtime::cli_child::session",
                    "processing decision request id={} kind={}",
                    request.request_id(),
                    request.kind()
                );
                let response = handle_decision(&mut ui, request)
                    .map_err(|err| format!("failed to handle decision: {err}"))?;
                log::trace!("Sending decision response: {:?}", response);
                write_frame(&mut stream, &CliToHost::DecisionResponse(response))
                    .map_err(|err| format!("failed to send decision response: {err}"))?;
            }
        }
    }
}

fn is_observer_role(role: &CliRole) -> bool {
    matches!(
        role,
        CliRole::Spectator
            | CliRole::PlayerObserver { .. }
            | CliRole::Omniscient
            | CliRole::SnapshotObserver
    )
}

fn run_observer_loop(
    mut stream: UnixStream,
    mut ui: CliUi,
    mut snapshot_writer: Option<SnapshotWriter>,
) -> Result<(), String> {
    stream
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set snapshot observer socket nonblocking: {err}"))?;
    let mut reader = NonblockingFrameReader::default();
    let mut latest_view = None;
    let mut latest_message = "connected as SnapshotObserver".to_owned();
    let mut event_count = 0;
    loop {
        let mut received_message = false;
        while let Some(msg) = reader
            .poll(&mut stream)
            .map_err(|err| format!("failed to read host frame: {err}"))?
        {
            received_message = true;
            match msg {
                HostToCli::Hello { .. } => {
                    log::trace!("Ignoring duplicate hello message");
                }
                HostToCli::Shutdown { reason } => {
                    latest_message = format!("shutdown: {reason}");
                    ui.set_message(latest_message.clone())
                        .map_err(|err| format!("failed to draw TUI: {err}"))?;
                    return Ok(());
                }
                HostToCli::Event { event, view } => {
                    event_count += 1;
                    let summary = ui_model_summary(&view);
                    log::trace!(
                        target: "catan_runtime::cli_child::observer_flow",
                        "decode event_count={} event={:?} {}",
                        event_count,
                        event,
                        summary
                    );
                    if let (
                        CliDisplayMode::Normal,
                        catan_core::gameplay::game::event::GameEvent::GameEnded {
                            winner_id,
                            turn_no,
                            stats,
                        },
                    ) = (ui.display_mode(), &event)
                    {
                        ui.show_game_ended(&view, *winner_id, *turn_no, stats)
                            .map_err(|err| format!("failed to draw game ended screen: {err}"))?;
                        return Ok(());
                    }
                    latest_message = format!("event: {event:?}");
                    log::trace!(
                        target: "catan_runtime::cli_child::observer_flow",
                        "display event_count={} event={:?} {}",
                        event_count,
                        event,
                        summary
                    );
                    ui.show_observer_model(&view, latest_message.clone(), event_count)
                        .map_err(|err| format!("failed to draw TUI: {err}"))?;
                    latest_view = Some(view);
                }
                HostToCli::DecisionRequest(request) => {
                    latest_message = format!(
                        "snapshot observer received unexpected decision request: {request:?}"
                    );
                    log::warn!("{latest_message}");
                    match latest_view.as_ref() {
                        Some(view) => ui
                            .show_observer_model(view, latest_message.clone(), event_count)
                            .map_err(|err| format!("failed to draw TUI: {err}"))?,
                        None => ui
                            .set_message(latest_message.clone())
                            .map_err(|err| format!("failed to draw TUI: {err}"))?,
                    }
                }
            }
        }

        match ui
            .poll_snapshot_input()
            .map_err(|err| format!("failed to read snapshot key: {err}"))?
        {
            Some(SnapshotInput::Snapshot) => {
                latest_message = match latest_view.as_ref() {
                    Some(view) => save_snapshot(snapshot_writer.as_mut(), view),
                    None => "no snapshot state available".to_owned(),
                };
                match latest_view.as_ref() {
                    Some(view) => ui
                        .show_observer_model(view, latest_message.clone(), event_count)
                        .map_err(|err| format!("failed to draw TUI: {err}"))?,
                    None => ui
                        .set_message(latest_message.clone())
                        .map_err(|err| format!("failed to draw TUI: {err}"))?,
                }
            }
            Some(SnapshotInput::Redraw) => match latest_view.as_ref() {
                Some(view) => ui
                    .show_observer_model(view, latest_message.clone(), event_count)
                    .map_err(|err| format!("failed to draw TUI: {err}"))?,
                None => ui
                    .set_message(latest_message.clone())
                    .map_err(|err| format!("failed to draw TUI: {err}"))?,
            },
            None => {}
        }

        if !received_message {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[derive(Default)]
struct NonblockingFrameReader {
    buffer: Vec<u8>,
}

impl NonblockingFrameReader {
    fn poll(&mut self, stream: &mut UnixStream) -> io::Result<Option<HostToCli>> {
        let mut chunk = [0; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "host closed snapshot observer socket",
                    ));
                }
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }

        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes(
            self.buffer[..4]
                .try_into()
                .expect("slice length checked above"),
        ) as usize;
        const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;
        if len > MAX_FRAME_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame exceeds maximum length",
            ));
        }
        let frame_len = 4 + len;
        if self.buffer.len() < frame_len {
            return Ok(None);
        }

        let payload = self.buffer[4..frame_len].to_vec();
        self.buffer.drain(..frame_len);
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(io::Error::other)
    }
}

fn save_snapshot(
    writer: Option<&mut SnapshotWriter>,
    view: &catan_agents::remote_agent::UiModel,
) -> String {
    let Some(writer) = writer else {
        return "snapshot writer is not available".to_owned();
    };
    let Some(state) = &view.snapshot_state else {
        return "no snapshot state available".to_owned();
    };
    match writer.write(state) {
        Ok(path) => format!("snapshot saved: {}", path.display()),
        Err(err) => format!("snapshot failed: {err}"),
    }
}

fn handle_decision(
    ui: &mut CliUi,
    request: DecisionRequestFrame,
) -> io::Result<DecisionResponseFrame> {
    log::trace!(
        target: "catan_runtime::cli_child::session",
        "handling decision request id={} kind={}",
        request.request_id(),
        request.kind()
    );
    match request {
        DecisionRequestFrame::InitStage(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing InitStage decision id={}",
                envelope.request_id
            );
            let settlement = read_initial_settlement(ui, &envelope, "settlement: ")?;
            log::trace!("Selected settlement: {:?}", settlement);
            let road = read_initial_road(ui, &envelope, settlement, "road: ")?;
            log::trace!("Selected road: {:?}", road);
            Ok(DecisionResponseFrame::InitStage(InitStageAction {
                establishment_position: settlement,
                road,
            }))
        }
        DecisionRequestFrame::InitAction(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing InitAction decision id={}",
                envelope.request_id
            );
            let action = read_init_action(ui, &envelope)?;
            log::trace!("Init action result: {:?}", action);
            Ok(DecisionResponseFrame::InitAction(action))
        }
        DecisionRequestFrame::PostDice(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing PostDice decision id={}",
                envelope.request_id
            );
            let action = read_post_dice_action(ui, &envelope)?;
            log::trace!("Post-dice action result: {:?}", action);
            Ok(DecisionResponseFrame::PostDice(action))
        }
        DecisionRequestFrame::PostDevCard(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing PostDevCard decision id={} (automatically rolling dice)",
                envelope.request_id
            );
            ui.show_model(&envelope.view, "dev card resolved; rolling dice".to_owned())?;
            Ok(DecisionResponseFrame::PostDevCard(
                PostDevCardAction::RollDice,
            ))
        }
        DecisionRequestFrame::Regular(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing Regular decision id={}",
                envelope.request_id
            );
            let action = read_regular_action(ui, &envelope)?;
            log::trace!("Regular action result: {:?}", action);
            Ok(DecisionResponseFrame::Regular(action))
        }
        DecisionRequestFrame::MoveRobbers(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing MoveRobbers decision id={}",
                envelope.request_id
            );
            let hex = read_hex(ui, &envelope, "robber hex: ")?;
            log::trace!("Selected robber hex: {:?}", hex);
            Ok(DecisionResponseFrame::MoveRobbers(MoveRobbersAction(hex)))
        }
        DecisionRequestFrame::ChoosePlayerToRob(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing ChoosePlayerToRob decision id={} robber_pos={:?}",
                envelope.request_id,
                envelope.legal.robber_pos
            );
            let player_id = read_robbed_player(ui, &envelope, "robbed player: ")?;
            log::trace!("Selected player to rob: {}", player_id);
            Ok(DecisionResponseFrame::ChoosePlayerToRob(
                ChoosePlayerToRobAction(player_id),
            ))
        }
        DecisionRequestFrame::AnswerTrade(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing AnswerTrade decision id={}",
                envelope.request_id
            );
            let answer = ui.prompt(&envelope.view, "answer trade [y/N]: ")?;
            let answer = match answer.as_str() {
                "y" | "yes" => {
                    log::trace!("Trade accepted");
                    TradeAnswer::Accept
                }
                _ => {
                    log::trace!("Trade declined");
                    TradeAnswer::Decline
                }
            };
            Ok(DecisionResponseFrame::AnswerTrade(answer))
        }
        DecisionRequestFrame::DropHalf(envelope) => {
            log::trace!(
                target: "catan_runtime::cli_child::session",
                "processing DropHalf decision id={}",
                envelope.request_id
            );
            let resources =
                read_resource_collection(ui, &envelope.view, "drop brick wood wheat sheep ore: ")?;
            log::trace!("Resources to drop: {:?}", resources);
            Ok(DecisionResponseFrame::DropHalf(DropHalfAction(resources)))
        }
    }
}
