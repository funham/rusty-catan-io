use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use catan_core::{
    gameplay::{
        game::{
            command::{
                ChooseRobbedPlayerCommand, DropHalfCommand, InitCommand, InitialPlacementCommand,
                MoveRobberCommand, PostDevCardCommand, PostDiceCommand, RegularCommand,
                TradeAnswer,
            },
            decision::{DecisionKind, OpenDecision},
            event::{GameEvent, PlayerNotification},
            input::{PlayerCommand, TradeCommand, TradeResponseCommand},
            view::{PlayerDecisionContext, PlayerNotificationContext},
        },
        primitives::{
            build::{Build, Establishment, EstablishmentType, Road},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind},
        },
    },
    topology::{Hex, HexIndex, Intersection, Path, repr::Dual},
};

use crate::bot::{BotPolicy, unsupported_decision_command};

#[derive(Debug, Default)]
struct TerminalUi;

#[derive(Clone, Debug, Default)]
pub struct SharedTerminalUi {
    inner: Arc<Mutex<TerminalUi>>,
}

#[derive(Clone, Debug)]
pub struct CliAgent {
    player_id: PlayerId,
    terminal: SharedTerminalUi,
}

impl CliAgent {
    pub fn new(id: impl Into<PlayerId>, terminal: SharedTerminalUi) -> Self {
        let id = id.into();
        Self {
            player_id: id,
            terminal,
        }
    }
}

impl PlayerNotification for CliAgent {
    fn on_event(&mut self, event: &GameEvent, context: PlayerNotificationContext<'_>) {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_notification(event, &context);
    }
}

impl CliAgent {
    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitialPlacementCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Initial placement", &context);
        loop {
            if let Some(action) = InitialPlacementCommand::try_new(
                TerminalUi::read_intersection("settlement (h1 h2 h3): "),
                TerminalUi::read_path("road (h1 h2): "),
            ) {
                break action;
            }

            log::warn!("incorrect initial stage placement")
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Before dice", &context);
        InitCommand::RollDice
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("After dice", &context);
        PostDiceCommand::RegularCommand(TerminalUi::read_regular_action())
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardCommand {
        PostDevCardCommand::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Action", &context);
        TerminalUi::read_regular_action()
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobberCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Move robber", &context);
        MoveRobberCommand(TerminalUi::read_hex("robber hex: "))
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        _robber_pos: Hex,
    ) -> ChooseRobbedPlayerCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Choose player to rob", &context);
        ChooseRobbedPlayerCommand(TerminalUi::read_player_id("player id: "))
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Decline
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfCommand {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Discard half", &context);
        DropHalfCommand(TerminalUi::read_resource_collection(
            "drop brick wood wheat sheep ore: ",
        ))
    }
}

impl BotPolicy for CliAgent {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn command_for(
        &mut self,
        decision: &OpenDecision,
        context: PlayerDecisionContext<'_>,
    ) -> Option<PlayerCommand> {
        match decision.kind {
            DecisionKind::InitPlacement => Some(PlayerCommand::InitialPlacement(
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
            DecisionKind::MoveRobber => {
                Some(PlayerCommand::MoveRobbers(self.move_robbers(context)))
            }
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
            DecisionKind::TradeOwnerAction { .. } => unsupported_decision_command(decision.kind),
        }
    }
}

impl TerminalUi {
    fn print_notification(event: &GameEvent, _context: &PlayerNotificationContext<'_>) {
        println!("event: {event:?}");
    }

    fn print_decision_context(label: &str, context: &PlayerDecisionContext<'_>) {
        let mut renderer = crate::cli_agent::ui::field_render::FieldRenderer::new();
        renderer.draw_context(&context.public);
        renderer.render();

        println!("\n== {label} ==");
        println!("player: {}", context.actor);
        println!("resources: {}", context.private.resources);
        println!("robber: {:?}", context.public.board_state.robber_pos);
        for player in &context.public.players {
            println!("player {} => {:?}", player.player_id, player.resources);
        }
    }

    pub fn parse_regular_action(line: &str) -> Option<RegularCommand> {
        let line = line.trim();
        if line == "end" || line.is_empty() {
            return Some(RegularCommand::EndMove);
        }
        if line == "buy dev" || line == "buy-dev" {
            return Some(RegularCommand::BuyDevCard);
        }
        if let Some(build) = Self::parse_build(line) {
            return Some(RegularCommand::Build(build));
        }
        if let Some(trade) = Self::parse_bank_trade(line) {
            return Some(RegularCommand::TradeWithBank(trade));
        }
        None
    }

    fn read_regular_action() -> RegularCommand {
        loop {
            let line = Self::read_line(
                "command [end | buy dev | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );
            if let Some(action) = Self::parse_regular_action(&line) {
                return action;
            }
            println!("could not parse action");
        }
    }

    fn parse_build(line: &str) -> Option<Build> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["build", "road", h1, h2] => {
                let path = Path::try_from((
                    HexIndex::spiral_to_hex(h1.parse().ok()?),
                    HexIndex::spiral_to_hex(h2.parse().ok()?),
                ))
                .ok()?;
                Some(Build::Road(Road { path }))
            }
            ["build", "settlement", h1, h2, h3] => {
                let pos = Intersection::try_from([
                    HexIndex::spiral_to_hex(h1.parse().ok()?),
                    HexIndex::spiral_to_hex(h2.parse().ok()?),
                    HexIndex::spiral_to_hex(h3.parse().ok()?),
                ])
                .ok()?;
                Some(Build::Establishment(Establishment {
                    vtx: pos,
                    stage: EstablishmentType::Settlement,
                }))
            }
            ["build", "city", h1, h2, h3] => {
                let pos = Intersection::try_from([
                    HexIndex::spiral_to_hex(h1.parse().ok()?),
                    HexIndex::spiral_to_hex(h2.parse().ok()?),
                    HexIndex::spiral_to_hex(h3.parse().ok()?),
                ])
                .ok()?;
                Some(Build::Establishment(Establishment {
                    vtx: pos,
                    stage: EstablishmentType::City,
                }))
            }
            _ => None,
        }
    }

    fn parse_bank_trade(line: &str) -> Option<BankTrade> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["bank-trade", give, take, kind] => Some(BankTrade {
                give: Self::parse_resource(give)?,
                take: Self::parse_resource(take)?,
                kind: match *kind {
                    "common" => BankTradeKind::BankGeneric,
                    "port-3" => BankTradeKind::PortGeneric,
                    "port-2" => BankTradeKind::PortSpecific,
                    _ => return None,
                },
            }),
            _ => None,
        }
    }

    fn parse_resource(token: &str) -> Option<Resource> {
        match token {
            "brick" => Some(Resource::Brick),
            "wood" => Some(Resource::Wood),
            "wheat" => Some(Resource::Wheat),
            "sheep" => Some(Resource::Sheep),
            "ore" => Some(Resource::Ore),
            _ => None,
        }
    }

    fn read_resource_collection(prompt: &str) -> ResourceCollection {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<u16>)
                .collect::<Result<Vec<_>, _>>();
            match parts {
                Ok(parts) if parts.len() == 5 => {
                    return ResourceCollection {
                        brick: parts[0],
                        wood: parts[1],
                        wheat: parts[2],
                        sheep: parts[3],
                        ore: parts[4],
                    };
                }
                _ => println!("expected five unsigned integers"),
            }
        }
    }

    fn read_hex(prompt: &str) -> Hex {
        loop {
            let line = Self::read_line(prompt);
            if let Ok(index) = line.parse::<usize>() {
                return HexIndex::spiral_to_hex(index);
            }
            println!("expected spiral hex 0-based index");
        }
    }

    fn read_path(prompt: &str) -> Path {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<usize>)
                .collect::<Result<Vec<_>, _>>();
            if let Ok(parts) = parts
                && parts.len() == 2
            {
                let h1 = HexIndex::spiral_to_hex(parts[0]);
                let h2 = HexIndex::spiral_to_hex(parts[1]);
                if let Ok(path) = Path::try_from((h1, h2)) {
                    return path;
                }
                if let Ok(path) = Path::<Dual>::try_from((h1, h2)) {
                    return path.canon();
                }
            }
            println!("expected adjacent hex pair");
        }
    }

    fn read_intersection(prompt: &str) -> Intersection {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<usize>)
                .collect::<Result<Vec<_>, _>>();
            if let Ok(parts) = parts
                && parts.len() == 3
                && let Ok(intersection) = Intersection::try_from([
                    HexIndex::spiral_to_hex(parts[0]),
                    HexIndex::spiral_to_hex(parts[1]),
                    HexIndex::spiral_to_hex(parts[2]),
                ])
            {
                return intersection;
            }
            println!("expected adjacent hex triplet");
        }
    }

    fn read_player_id(prompt: &str) -> PlayerId {
        loop {
            let line = Self::read_line(prompt);
            if let Ok(id) = line.parse() {
                return id;
            }
            println!("expected unsigned integer");
        }
    }

    fn read_line(prompt: &str) -> String {
        print!("{prompt}");
        io::stdout().flush().expect("failed to flush stdout");

        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .expect("failed to read line");
        line.trim().to_owned()
    }
}
