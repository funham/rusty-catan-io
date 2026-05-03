use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use catan_core::{
    agent::{
        action::{
            ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction,
            MoveRobbersAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
        },
        agent::PlayerRuntime,
    },
    gameplay::{
        game::{
            event::{GameEvent, PlayerNotification},
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
    pub fn new(id: PlayerId, terminal: SharedTerminalUi) -> Self {
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

impl PlayerRuntime for CliAgent {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn init_stage_action(&mut self, context: PlayerDecisionContext<'_>) -> InitStageAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Initial placement", &context);
        InitStageAction {
            establishment_position: TerminalUi::read_intersection("settlement (h1 h2 h3): "),
            road: Road {
                pos: TerminalUi::read_path("road (h1 h2): "),
            },
        }
    }

    fn init_action(&mut self, context: PlayerDecisionContext<'_>) -> InitAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Before dice", &context);
        InitAction::RollDice
    }

    fn after_dice_action(&mut self, context: PlayerDecisionContext<'_>) -> PostDiceAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("After dice", &context);
        PostDiceAction::RegularAction(TerminalUi::read_regular_action())
    }

    fn after_dev_card_action(&mut self, _context: PlayerDecisionContext<'_>) -> PostDevCardAction {
        PostDevCardAction::RollDice
    }

    fn regular_action(&mut self, context: PlayerDecisionContext<'_>) -> RegularAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Action", &context);
        TerminalUi::read_regular_action()
    }

    fn move_robbers(&mut self, context: PlayerDecisionContext<'_>) -> MoveRobbersAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Move robber", &context);
        MoveRobbersAction(TerminalUi::read_hex("robber hex: "))
    }

    fn choose_player_to_rob(
        &mut self,
        context: PlayerDecisionContext<'_>,
        _robber_pos: Hex,
    ) -> ChoosePlayerToRobAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Choose player to rob", &context);
        ChoosePlayerToRobAction(TerminalUi::read_player_id("player id: "))
    }

    fn answer_trade(&mut self, _context: PlayerDecisionContext<'_>) -> TradeAnswer {
        TradeAnswer::Decline
    }

    fn drop_half(&mut self, context: PlayerDecisionContext<'_>) -> DropHalfAction {
        let _guard = self.terminal.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::print_decision_context("Discard half", &context);
        DropHalfAction(TerminalUi::read_resource_collection(
            "drop brick wood wheat sheep ore: ",
        ))
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

    pub fn parse_regular_action(line: &str) -> Option<RegularAction> {
        let line = line.trim();
        if line == "end" || line.is_empty() {
            return Some(RegularAction::EndMove);
        }
        if line == "buy dev" || line == "buy-dev" {
            return Some(RegularAction::BuyDevCard);
        }
        if let Some(build) = Self::parse_build(line) {
            return Some(RegularAction::Build(build));
        }
        if let Some(trade) = Self::parse_bank_trade(line) {
            return Some(RegularAction::TradeWithBank(trade));
        }
        None
    }

    fn read_regular_action() -> RegularAction {
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
                Some(Build::Road(Road { pos: path }))
            }
            ["build", "settlement", h1, h2, h3] => {
                let pos = Intersection::try_from([
                    HexIndex::spiral_to_hex(h1.parse().ok()?),
                    HexIndex::spiral_to_hex(h2.parse().ok()?),
                    HexIndex::spiral_to_hex(h3.parse().ok()?),
                ])
                .ok()?;
                Some(Build::Establishment(Establishment {
                    pos,
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
                    pos,
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
