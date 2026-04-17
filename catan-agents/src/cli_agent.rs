use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use catan_core::{
    agent::{
        action::{
            FinalStateAnswer, InitialAction, PostDevCardAction, PostDiceThrowAnswer, TradeAction,
        },
        Agent, AgentRequest, AgentResponse,
    },
    gameplay::{
        game::state::Perspective,
        primitives::{
            build::{Builds, City, Road, Settlement},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind},
        },
    },
    topology::{Hex, Intersection, Path},
};

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
                            return AgentResponse::Init(InitialAction::ThrowDice);
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
                AgentResponse::AfterDiceThrow(Self::read_post_dice_action())
            }
            AgentRequest::Rest(perspective) => {
                Self::print_perspective("Rest", &perspective);
                AgentResponse::Rest(Self::read_rest_action())
            }
            AgentRequest::RobHex(perspective) => {
                Self::print_perspective("RobHex", &perspective);
                AgentResponse::RobHex(Self::read_hex("robber hex (q r): "))
            }
            AgentRequest::RobPlayer(perspective) => {
                Self::print_perspective("RobPlayer", &perspective);
                AgentResponse::RobPlayer(Self::read_player_id("robbed player id: "))
            }
            AgentRequest::Initialization(perspective) => {
                Self::print_perspective("Initialization", &perspective);
                let settlement = Settlement {
                    pos: Self::read_intersection("settlement (q1 r1 q2 r2 q3 r3): "),
                };
                let road = Road {
                    pos: Self::read_path("road (q1 r1 q2 r2): "),
                };
                AgentResponse::Initialization { settlement, road }
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
        println!("\n== {label} ==");
        println!("player: {}", perspective.player_id);
        println!("resources: {:?}", perspective.player_view.resources);
        println!("dev cards: {:?}", perspective.player_view.dev_cards);
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

    fn read_post_dice_action() -> PostDiceThrowAnswer {
        loop {
            let line = Self::read_line(
                "command [end | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );
            if line == "end" || line.is_empty() {
                return PostDiceThrowAnswer::EndMove;
            }

            if let Some(build) = Self::parse_build(&line) {
                return PostDiceThrowAnswer::Build(build);
            }

            if let Some(trade) = Self::parse_bank_trade(&line) {
                return PostDiceThrowAnswer::TradeWithBank(trade);
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

    fn parse_build(line: &str) -> Option<Builds> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["build", "road", q1, r1, q2, r2] => {
                let path = Path::try_from((
                    Hex::new(q1.parse().ok()?, r1.parse().ok()?),
                    Hex::new(q2.parse().ok()?, r2.parse().ok()?),
                ))
                .ok()?;
                Some(Builds::Road(Road { pos: path }))
            }
            ["build", "settlement", q1, r1, q2, r2, q3, r3] => {
                let pos = Intersection::try_from([
                    Hex::new(q1.parse().ok()?, r1.parse().ok()?),
                    Hex::new(q2.parse().ok()?, r2.parse().ok()?),
                    Hex::new(q3.parse().ok()?, r3.parse().ok()?),
                ])
                .ok()?;
                Some(Builds::Settlement(Settlement { pos }))
            }
            ["build", "city", q1, r1, q2, r2, q3, r3] => {
                let pos = Intersection::try_from([
                    Hex::new(q1.parse().ok()?, r1.parse().ok()?),
                    Hex::new(q2.parse().ok()?, r2.parse().ok()?),
                    Hex::new(q3.parse().ok()?, r3.parse().ok()?),
                ])
                .ok()?;
                Some(Builds::City(City { pos }))
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
                    return ResourceCollection::new(parts[0], parts[1], parts[2], parts[3], parts[4]);
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
                Ok(parts) if parts.len() == 2 => return Hex::new(parts[0], parts[1]),
                _ => println!("expected: q r"),
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
                Ok(parts) if parts.len() == 4 => {
                    let result = Path::try_from((
                        Hex::new(parts[0], parts[1]),
                        Hex::new(parts[2], parts[3]),
                    ));
                    if let Ok(path) = result {
                        return path;
                    }
                }
                _ => {}
            }
            println!("expected adjacent hex pair: q1 r1 q2 r2");
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
                Ok(parts) if parts.len() == 6 => {
                    let result = Intersection::try_from([
                        Hex::new(parts[0], parts[1]),
                        Hex::new(parts[2], parts[3]),
                        Hex::new(parts[4], parts[5]),
                    ]);
                    if let Ok(intersection) = result {
                        return intersection;
                    }
                }
                _ => {}
            }
            println!("expected adjacent hex triplet: q1 r1 q2 r2 q3 r3");
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
