//! Host session orchestration for the CLI child.
//!
//! Owns the Unix socket lifecycle, handshake, host-frame loop, decision dispatch,
//! shutdown handling, and request/response logging.

use std::{io, os::unix::net::UnixStream, path::Path as FsPath, time::Duration};

use catan_agents::remote_agent::{
    CliRole, CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli,
    NonblockingFrameReader, UiModel, read_frame, ui_model_summary, write_frame,
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
    tui::{CliUi, CliViewMode, ControlInput},
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
    let view_mode = view_mode_for_role(&role);
    let snapshot_writer = match &role {
        role if role.includes_exact_snapshot_state() => Some(
            SnapshotWriter::new()
                .map_err(|err| format!("failed to initialize snapshots: {err}"))?,
        ),
        _ => None,
    };
    let mut ui = CliUi::new(view_mode).map_err(|err| format!("failed to initialize TUI: {err}"))?;
    ui.set_message(format!("connected as {role:?}"))
        .map_err(|err| format!("failed to draw TUI: {err}"))?;
    write_frame(&mut stream, &CliToHost::Ready)
        .map_err(|err| format!("failed to send ready: {err}"))?;
    log::trace!("Sent ready message to host");

    if role.is_observer() {
        return run_observer_session(stream, ui, snapshot_writer);
    }

    run_player_session(stream, ui, view_mode)
}

fn view_mode_for_role(role: &CliRole) -> CliViewMode {
    if role.includes_exact_snapshot_state() {
        CliViewMode::Snapshot
    } else {
        CliViewMode::Normal
    }
}

fn run_player_session(
    mut stream: UnixStream,
    mut ui: CliUi,
    view_mode: CliViewMode,
) -> Result<(), String> {
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
                if process_host_event(&mut ui, view_mode, &event, &view, None)? {
                    return Ok(());
                }
            }
            HostToCli::DecisionRequest(request) => {
                if view_mode == CliViewMode::Snapshot {
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

fn run_observer_session(
    mut stream: UnixStream,
    mut ui: CliUi,
    snapshot_writer: Option<SnapshotWriter>,
) -> Result<(), String> {
    stream
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set snapshot observer socket nonblocking: {err}"))?;
    let mut reader = NonblockingFrameReader::<HostToCli>::default();
    let mut state = ObserverSessionState {
        latest: SessionViewState::message("connected as observer"),
        event_count: 0,
        snapshot_writer,
    };
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
                    return handle_shutdown(&mut ui, reason);
                }
                HostToCli::Event { event, view } => {
                    state.event_count += 1;
                    log::trace!(
                        target: "catan_runtime::cli_child::observer_flow",
                        "decode event_count={} event={:?} {}",
                        state.event_count,
                        event,
                        ui_model_summary(&view)
                    );
                    let view_mode = ui.view_mode();
                    if process_host_event(
                        &mut ui,
                        view_mode,
                        &event,
                        &view,
                        Some(state.event_count),
                    )? {
                        return Ok(());
                    }
                    state.latest = SessionViewState::view(view, format!("event: {event:?}"));
                }
                HostToCli::DecisionRequest(request) => {
                    let message =
                        format!("observer received unexpected decision request: {request:?}");
                    send_child_error(&mut stream, message.clone())?;
                    state.latest.message = message;
                    draw_latest_or_message(&mut ui, &state.latest, state.event_count)?;
                }
            }
        }

        let can_save_snapshot = matches!(ui.view_mode(), CliViewMode::Snapshot);
        handle_control_input(
            &mut ui,
            &mut state.latest,
            state.snapshot_writer.as_mut(),
            can_save_snapshot,
            state.event_count,
        )?;

        if !received_message {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

struct ObserverSessionState {
    latest: SessionViewState,
    event_count: u64,
    snapshot_writer: Option<SnapshotWriter>,
}

struct SessionViewState {
    view: Option<UiModel>,
    message: String,
}

impl SessionViewState {
    fn message(message: impl Into<String>) -> Self {
        Self {
            view: None,
            message: message.into(),
        }
    }

    fn view(view: UiModel, message: impl Into<String>) -> Self {
        Self {
            view: Some(view),
            message: message.into(),
        }
    }
}

fn process_host_event(
    ui: &mut CliUi,
    view_mode: CliViewMode,
    event: &catan_core::gameplay::game::event::GameEvent,
    view: &UiModel,
    observer_event_count: Option<u64>,
) -> Result<bool, String> {
    log::trace!(
        target: "catan_runtime::cli_child::session",
        "processing event: {:?}",
        event
    );
    if let (
        CliViewMode::Normal,
        catan_core::gameplay::game::event::GameEvent::GameEnded {
            winner_id,
            turn_no,
            stats,
        },
    ) = (view_mode, event)
    {
        ui.show_game_ended(view, *winner_id, *turn_no, stats)
            .map_err(|err| format!("failed to draw game ended screen: {err}"))?;
        return Ok(true);
    }

    let message = format!("event: {event:?}");
    match observer_event_count {
        Some(event_count) => {
            log::trace!(
                target: "catan_runtime::cli_child::observer_flow",
                "display event_count={} event={:?} {}",
                event_count,
                event,
                ui_model_summary(view)
            );
            ui.show_observer_model(view, message, event_count)
        }
        None => ui.show_model(view, message),
    }
    .map_err(|err| format!("failed to draw TUI: {err}"))?;
    Ok(false)
}

fn handle_control_input(
    ui: &mut CliUi,
    latest: &mut SessionViewState,
    snapshot_writer: Option<&mut SnapshotWriter>,
    can_save_snapshot: bool,
    event_count: u64,
) -> Result<(), String> {
    match ui
        .poll_control_input()
        .map_err(|err| format!("failed to read control input: {err}"))?
    {
        Some(ControlInput::SaveSnapshot) if can_save_snapshot => {
            latest.message = match latest.view.as_ref() {
                Some(view) => save_snapshot(snapshot_writer, view),
                None => "no snapshot state available".to_owned(),
            };
            draw_latest_or_message(ui, latest, event_count)
        }
        Some(ControlInput::SaveSnapshot) => Ok(()),
        Some(ControlInput::Redraw) => draw_latest_or_message(ui, latest, event_count),
        None => Ok(()),
    }
}

fn draw_latest_or_message(
    ui: &mut CliUi,
    latest: &SessionViewState,
    event_count: u64,
) -> Result<(), String> {
    match latest.view.as_ref() {
        Some(view) => ui
            .show_observer_model(view, latest.message.clone(), event_count)
            .map_err(|err| format!("failed to draw TUI: {err}")),
        None => ui
            .set_message(latest.message.clone())
            .map_err(|err| format!("failed to draw TUI: {err}")),
    }
}

fn handle_shutdown(ui: &mut CliUi, reason: String) -> Result<(), String> {
    log::trace!("Shutdown received with reason: {}", reason);
    ui.set_message(format!("shutdown: {reason}"))
        .map_err(|err| format!("failed to draw TUI: {err}"))?;
    Ok(())
}

fn send_child_error(stream: &mut UnixStream, message: String) -> Result<(), String> {
    log::warn!("{message}");
    write_frame(stream, &CliToHost::Error { message })
        .map_err(|err| format!("failed to send observer error: {err}"))
}

fn save_snapshot(writer: Option<&mut SnapshotWriter>, view: &UiModel) -> String {
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
