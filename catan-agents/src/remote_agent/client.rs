use std::{io, os::unix::net::UnixStream};

use catan_core::{
    gameplay::{
        game::{
            command::{
                ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
                MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
                TradeAnswer,
            },
            decision::DecisionKind,
            event::{
                GameEvent, GameObserver, ObserverKind, ObserverNotificationContext,
                PlayerNotification,
            },
            input::DecisionRequest,
            input::{PlayerCommand, TradeCommand, TradeResponseCommand},
            view::{PlayerDecisionContext, PlayerNotificationContext},
        },
        primitives::player::PlayerId,
    },
    topology::Hex,
};

use super::{
    model::{UiModel, ui_model_summary},
    protocol::{
        CliRole, CliToHost, DecisionRequestEnvelope, DecisionRequestFrame, DecisionResponseFrame,
        HostToCli, LegalDecisionOptions, read_frame, write_frame,
    },
};

use crate::bot::{BotPolicy, unsupported_decision_command};

pub struct RemoteCliAgent {
    player_id: PlayerId,
    stream: UnixStream,
    next_request_id: u64,
}

impl RemoteCliAgent {
    pub fn new(player_id: impl Into<PlayerId>, mut stream: UnixStream) -> io::Result<Self> {
        let player_id = player_id.into();
        write_frame(
            &mut stream,
            &HostToCli::Hello {
                role: CliRole::Player { player_id },
            },
        )?;
        expect_ready(&mut stream)?;
        Ok(Self {
            player_id,
            stream,
            next_request_id: 0,
        })
    }

    fn request(&mut self, request: DecisionRequestFrame) -> DecisionResponseFrame {
        let request_id = request.request_id();
        let kind = request.kind();
        log::trace!(
            target: "catan_agents::remote_agent",
            "sending CLI decision request id={request_id} kind={kind}"
        );
        write_frame(&mut self.stream, &HostToCli::DecisionRequest(request))
            .expect("failed to write CLI decision request");
        loop {
            match read_frame::<CliToHost>(&mut self.stream).expect("failed to read CLI response") {
                CliToHost::DecisionResponse(response) => {
                    log::trace!(
                        target: "catan_agents::remote_agent",
                        "received CLI decision response id={request_id} kind={kind}"
                    );
                    return response;
                }
                CliToHost::Log {
                    level,
                    target,
                    message,
                } => {
                    let level = log::Level::from(level);
                    for line in message.lines().filter(|line| !line.trim().is_empty()) {
                        log::log!(target: &target, level, "{line}");
                    }
                }
                CliToHost::Error { message } => panic!("remote CLI error: {message}"),
                other => panic!("unexpected CLI frame on game socket: {other:?}"),
            }
        }
    }

    fn envelope(
        &mut self,
        context: &PlayerDecisionContext<'_>,
        robber_pos: Option<Hex>,
    ) -> DecisionRequestEnvelope {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        DecisionRequestEnvelope {
            request_id,
            view: UiModel::from_decision(context),
            legal: LegalDecisionOptions::from_context(context, robber_pos),
        }
    }
}

impl PlayerNotification for RemoteCliAgent {
    fn on_event(&mut self, event: &GameEvent, context: PlayerNotificationContext<'_>) {
        let model = UiModel::from_player_notification(&context);
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Event {
                event: event.clone(),
                view: model,
            },
        );
    }
}

impl RemoteCliAgent {
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitialPlacementCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::InitStage(envelope)) {
            DecisionResponseFrame::InitStage(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::InitCommand(envelope)) {
            DecisionResponseFrame::InitCommand(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::PostDice(envelope)) {
            DecisionResponseFrame::PostDice(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn after_dev_card_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDevCardCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::PostDevCard(envelope)) {
            DecisionResponseFrame::PostDevCard(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::Regular(envelope)) {
            DecisionResponseFrame::Regular(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn move_robber(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::MoveRobber(envelope)) {
            DecisionResponseFrame::MoveRobber(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        robber_pos: Hex,
    ) -> ChooseRobbedPlayerCommand {
        let envelope = self.envelope(&context, Some(robber_pos));
        match self.request(DecisionRequestFrame::ChoosePlayerToRob(envelope)) {
            DecisionResponseFrame::ChoosePlayerToRob(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn answer_trade(&mut self, context: PlayerDecisionContext<'_>) -> TradeAnswer {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::AnswerTrade(envelope)) {
            DecisionResponseFrame::AnswerTrade(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfCommand {
        let envelope = self.envelope(&context, None);
        match self.request(DecisionRequestFrame::DropHalf(envelope)) {
            DecisionResponseFrame::DropHalf(action) => action,
            other => panic!("unexpected CLI response: {other:?}"),
        }
    }
}

impl BotPolicy for RemoteCliAgent {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn command_for(
        &mut self,
        request: &DecisionRequest,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match request.kind() {
            DecisionKind::InitialPlacement => Some(PlayerCommand::InitialPlacement(
                self.init_stage_action(context),
            )),
            DecisionKind::InitCommand => {
                Some(PlayerCommand::InitCommand(self.init_action(context)))
            }
            DecisionKind::PostDiceCommand => {
                Some(PlayerCommand::PostDice(self.after_dice_action(context)))
            }
            DecisionKind::PostDevCardCommand => Some(PlayerCommand::PostDevCard(
                self.after_dev_card_action(context),
            )),
            DecisionKind::RegularCommand => {
                Some(PlayerCommand::Regular(self.regular_action(context)))
            }
            DecisionKind::MoveRobber => Some(PlayerCommand::MoveRobber(self.move_robber(context))),
            DecisionKind::ChooseRobbedPlayer { robber_pos } => Some(
                PlayerCommand::ChooseRobbedPlayer(self.choose_player_to_rob(context, robber_pos)),
            ),
            DecisionKind::DropHalf { .. } => Some(PlayerCommand::DropHalf(self.drop_half(context))),
            DecisionKind::TradeResponse { .. } => {
                Some(PlayerCommand::Trade(match self.answer_trade(context) {
                    TradeAnswer::Accept => return None,
                    TradeAnswer::Decline => TradeCommand::Respond(TradeResponseCommand::Reject),
                }))
            }
            DecisionKind::TradeOwnerAction { .. } => unsupported_decision_command(request),
        }
    }
}

impl Drop for RemoteCliAgent {
    fn drop(&mut self) {
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Shutdown {
                reason: "host dropped remote CLI agent".to_owned(),
            },
        );
    }
}

pub struct RemoteCliObserver {
    role: CliRole,
    stream: UnixStream,
}

impl RemoteCliObserver {
    pub fn new(kind: ObserverKind, stream: UnixStream) -> io::Result<Self> {
        let role = match kind {
            ObserverKind::Spectator => CliRole::Spectator,
            ObserverKind::Player(player_id) => CliRole::PlayerObserver { player_id },
            ObserverKind::Omniscient => CliRole::Omniscient,
        };
        Self::new_with_role(role, stream)
    }

    pub fn new_snapshot(stream: UnixStream) -> io::Result<Self> {
        Self::new_with_role(CliRole::SnapshotObserver, stream)
    }

    pub fn new_with_role(role: CliRole, mut stream: UnixStream) -> io::Result<Self> {
        if matches!(role, CliRole::Player { .. }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote CLI observer requires an observer role",
            ));
        }
        write_frame(&mut stream, &HostToCli::Hello { role: role.clone() })?;
        expect_ready(&mut stream)?;
        Ok(Self { role, stream })
    }

    #[cfg(test)]
    pub(crate) fn from_connected_role(role: CliRole, stream: UnixStream) -> Self {
        Self { role, stream }
    }
}

impl GameObserver for RemoteCliObserver {
    fn kind(&self) -> ObserverKind {
        self.role
            .observer_kind()
            .expect("RemoteCliObserver always stores an observer role")
    }

    fn on_event(&mut self, event: &GameEvent, context: ObserverNotificationContext<'_>) {
        let include_snapshot_state = self.role.includes_exact_snapshot_state();
        let model = UiModel::from_observer(context, include_snapshot_state);
        let summary = ui_model_summary(&model);
        let started = std::time::Instant::now();
        log::trace!(
            target: "catan_agents::remote_observer_flow",
            "send start role={} event={:?} exact_state={} {}",
            self.role.label(),
            event,
            include_snapshot_state,
            summary
        );
        if let Err(err) = write_frame(
            &mut self.stream,
            &HostToCli::Event {
                event: event.clone(),
                view: model,
            },
        ) {
            log::warn!(
                target: "catan_agents::remote_observer_flow",
                "send failed role={} event={:?} elapsed_ms={} err={err}",
                self.role.label(),
                event,
                started.elapsed().as_millis()
            );
        } else {
            log::trace!(
                target: "catan_agents::remote_observer_flow",
                "send done role={} event={:?} elapsed_ms={}",
                self.role.label(),
                event,
                started.elapsed().as_millis()
            );
        }
    }
}

impl Drop for RemoteCliObserver {
    fn drop(&mut self) {
        let _ = write_frame(
            &mut self.stream,
            &HostToCli::Shutdown {
                reason: "host dropped remote CLI observer".to_owned(),
            },
        );
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
