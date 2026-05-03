//! Host session orchestration for the CLI child.
//!
//! Owns the Unix socket lifecycle, handshake, host-frame loop, decision dispatch,
//! shutdown handling, and request/response logging.

use std::{io, os::unix::net::UnixStream, path::Path as FsPath};

use catan_agents::remote_agent::{
    CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli, read_frame, write_frame,
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
    tui::CliUi,
};

pub fn run(socket: &FsPath, _role: &str) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;
    init_socket_logger(
        stream
            .try_clone()
            .map_err(|err| format!("failed to clone CLI socket for logging: {err}"))?,
    );
    log::trace!("Starting CLI with socket: {}", socket.display());
    let mut ui = CliUi::new().map_err(|err| format!("failed to initialize TUI: {err}"))?;
    match read_frame::<HostToCli>(&mut stream)
        .map_err(|err| format!("failed to read hello: {err}"))?
    {
        HostToCli::Hello { role } => {
            log::trace!("Connected as role: {:?}", role);
            ui.set_message(format!("connected as {role:?}"))
                .map_err(|err| format!("failed to draw TUI: {err}"))?;
            write_frame(&mut stream, &CliToHost::Ready)
                .map_err(|err| format!("failed to send ready: {err}"))?;
            log::trace!("Sent ready message to host");
        }
        other => return Err(format!("expected hello, got {other:?}")),
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
                if let catan_core::gameplay::game::event::GameEvent::GameEnded {
                    winner_id,
                    turn_no,
                    stats,
                } = event
                {
                    ui.show_game_ended(&view, winner_id, turn_no, &stats)
                        .map_err(|err| format!("failed to draw game ended screen: {err}"))?;
                    return Ok(());
                }
                ui.show_model(&view, format!("event: {event:?}"))
                    .map_err(|err| format!("failed to draw TUI: {err}"))?;
            }
            HostToCli::DecisionRequest(request) => {
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
