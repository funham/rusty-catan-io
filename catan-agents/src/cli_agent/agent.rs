use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use catan_core::{
    agent::{
        Agent, AgentRequest, AgentResponse,
        action::{
            FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceAnswer, TradeAction,
        },
    },
    gameplay::{
        game::state::Perspective,
        primitives::{
            build::{Build, Establishment, EstablishmentType, Road},
            dev_card::{UsableDevCardCollection, UsableDevCardKind},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind},
        },
    },
    topology::{Hex, HexIndex, Intersection, Path, repr::Dual},
};

use crate::cli_agent::ascii;

#[derive(Debug, Default)]
struct TerminalUi;

#[derive(Clone, Debug, Default)]
pub struct SharedTerminalUi {
    inner: Arc<Mutex<TerminalUi>>,
}

#[derive(Clone, Debug)]
pub struct CliAgent {
    terminal: SharedTerminalUi,
}

impl CliAgent {
    pub fn new(terminal: SharedTerminalUi) -> Self {
        Self { terminal }
    }
}

impl Agent for CliAgent {
    fn respond(&mut self, request: AgentRequest) -> AgentResponse {
        self.terminal.prompt(request)
    }
}

impl SharedTerminalUi {
    pub fn prompt(&self, request: AgentRequest) -> AgentResponse {
        let _guard = self.inner.lock().expect("terminal mutex poisoned");
        TerminalUi::prompt_locked(request)
    }
}

impl TerminalUi {
    fn prompt_locked(request: AgentRequest) -> AgentResponse {
        match request {
            AgentRequest::Init(perspective) => {
                Self::print_perspective("Init", &perspective);
                loop {
                    let line = Self::read_line("command [throw-dice]: ");
                    match line.as_str() {
                        "throw-dice" | "roll" | "" => {
                            return AgentResponse::Init(InitialAction::RollDice);
                        }
                        _ => println!("supported commands: throw-dice"),
                    }
                }
            }
            AgentRequest::AfterDevCard(perspective) => {
                Self::print_perspective("AfterDevCard", &perspective);
                AgentResponse::AfterDevCard(PostDevCardAction::ThrowDice)
            }
            AgentRequest::AfterDiceThrow(perspective) => {
                Self::print_perspective("AfterDiceThrow", &perspective);
                AgentResponse::AfterDice(Self::read_post_dice_action())
            }
            AgentRequest::Rest(perspective) => {
                Self::print_perspective("Rest", &perspective);
                AgentResponse::Rest(Self::read_rest_action())
            }
            AgentRequest::RobHex(perspective) => {
                Self::print_perspective("RobHex", &perspective);
                AgentResponse::RobHex(Self::read_hex("robber hex (#hex): "))
            }
            AgentRequest::RobPlayer(perspective) => {
                Self::print_perspective("RobPlayer", &perspective);
                AgentResponse::RobPlayer(Self::read_player_id("robbed player id: "))
            }
            AgentRequest::Initialization(perspective) => {
                Self::print_perspective("Initialization", &perspective);
                let establishment = Establishment {
                    pos: Self::read_intersection("settlement (h1, h2, h3): "),
                    stage: EstablishmentType::Settlement,
                };
                let road = Road {
                    pos: Self::read_path("road (h1, h2): "),
                };
                AgentResponse::Initialization {
                    establishment,
                    road,
                }
            }
            AgentRequest::AnswerTrade { perspective, trade } => {
                Self::print_perspective("AnswerTrade", &perspective);
                println!("trade offer give={:?} take={:?}", trade.give, trade.take);
                let line = Self::read_line("answer [accept/decline]: ");
                let action = match line.as_str() {
                    "accept" | "yes" | "y" => TradeAction::Accepted,
                    _ => TradeAction::Declined,
                };
                AgentResponse::AnswerTrade(action)
            }
            AgentRequest::DropHalf(perspective) => {
                Self::print_perspective("DropHalf", &perspective);
                println!("enter five counts in order: brick wood wheat sheep ore");
                AgentResponse::DropHalf(Self::read_resource_collection("drop counts: "))
            }
        }
    }

    fn print_perspective(label: &str, perspective: &Perspective) {
        let mut renderer = ascii::field_render::FieldRenderer::new();
        renderer.draw_perspective(perspective);
        renderer.render();

        // TODO: colorize
        println!("\n== {label} ==");
        println!("player id: {}", perspective.player_id);
        println!("resources: {:?}", perspective.player_view.resources);

        let print_dev_card_collection = |label: &str, deck: UsableDevCardCollection| {
            println!(
                "{}: | Kn: {} | YoP: {} | RB: {} | M: {} |",
                label,
                deck[UsableDevCardKind::Knight],
                deck[UsableDevCardKind::YearOfPlenty],
                deck[UsableDevCardKind::RoadBuild],
                deck[UsableDevCardKind::Monopoly]
            );
        };

        println!("dev cards:");
        println!("-------------------------------------------");
        print_dev_card_collection("Active: ", perspective.player_view.dev_cards.active);
        print_dev_card_collection("Queued: ", perspective.player_view.dev_cards.queued);
        print_dev_card_collection("Used  : ", perspective.player_view.dev_cards.used);
        println!("-------------------------------------------");

        println!("robber: {:?}", perspective.field.robber_pos);
        for player in &perspective.other_players {
            println!(
                "opponent {} => resource cards: {}, active dev: {}, queued dev: {}",
                player.player_id,
                player.public_data.resource_card_count,
                player.public_data.dev_cards.active,
                player.public_data.dev_cards.queued,
            );
        }
    }

    fn read_post_dice_action() -> PostDiceAnswer {
        loop {
            let line = Self::read_line(
                "command [end | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );
            if line == "end" || line.is_empty() {
                return PostDiceAnswer::EndMove;
            }

            if let Some(build) = Self::parse_build(&line) {
                return PostDiceAnswer::Build(build);
            }

            if let Some(trade) = Self::parse_bank_trade(&line) {
                return PostDiceAnswer::TradeWithBank(trade);
            }

            println!("could not parse action");
        }
    }

    fn read_rest_action() -> FinalStateAnswer {
        loop {
            let line = Self::read_line(
                "command [end | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );
            if line == "end" || line.is_empty() {
                return FinalStateAnswer::EndMove;
            }

            if let Some(build) = Self::parse_build(&line) {
                return FinalStateAnswer::Build(build);
            }

            if let Some(trade) = Self::parse_bank_trade(&line) {
                return FinalStateAnswer::TradeWithBank(trade);
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
                    "common" => BankTradeKind::Common,
                    "port-3" => BankTradeKind::PortUniversal,
                    "port-2" => BankTradeKind::PortSpecial,
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
                    return ResourceCollection::new(
                        parts[0], parts[1], parts[2], parts[3], parts[4],
                    );
                }
                _ => println!("expected five unsigned integers"),
            }
        }
    }

    fn read_hex(prompt: &str) -> Hex {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>();
            match parts {
                Ok(parts) if parts.len() == 1 => return HexIndex::spiral_to_hex(parts[0] as usize),
                _ => println!("expected spiral hex 0-based index (usize)"),
            }
        }
    }

    fn read_path(prompt: &str) -> Path {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>();
            match parts {
                Ok(parts) if parts.len() == 2 => {
                    let result_canon = Path::try_from((
                        HexIndex::spiral_to_hex(parts[0] as usize),
                        HexIndex::spiral_to_hex(parts[1] as usize),
                    ));

                    let result_dual = Path::<Dual>::try_from((
                        HexIndex::spiral_to_hex(parts[0] as usize),
                        HexIndex::spiral_to_hex(parts[1] as usize),
                    ));

                    match (result_canon, result_dual) {
                        (Ok(path), _) => {
                            return path;
                        }
                        (_, Ok(path)) => {
                            return path.canon();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            println!("expected adjacent hex pair: h1 h2 or a dual representation");
        }
    }

    fn read_intersection(prompt: &str) -> Intersection {
        loop {
            let line = Self::read_line(prompt);
            let parts = line
                .split_whitespace()
                .map(str::parse::<i32>)
                .collect::<Result<Vec<_>, _>>();
            match parts {
                Ok(parts) if parts.len() == 3 => {
                    let result = Intersection::try_from([
                        HexIndex::spiral_to_hex(parts[0] as usize),
                        HexIndex::spiral_to_hex(parts[1] as usize),
                        HexIndex::spiral_to_hex(parts[2] as usize),
                    ]);
                    match result {
                        Ok(intersection) => return intersection,
                        Err(_) => {
                            log::error!("hexes are not adjacent");
                        }
                    }
                    if let Ok(intersection) = result {
                        return intersection;
                    }
                }
                _ => {}
            }
            println!("expected adjacent hex triplet: h1 h2 h3");
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
