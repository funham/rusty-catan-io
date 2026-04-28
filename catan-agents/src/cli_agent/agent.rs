use std::{
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};

use catan_core::{
    agent::{
        Agent,
        action::{InitAction, PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer},
    },
    gameplay::{
        game::{
            event::{GameEvent, PlayerContext, PlayerObserver},
            view::{GameView, PrivatePlayerData},
        },
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

use crate::cli_agent::ui;

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

impl PlayerObserver for CliAgent {
    fn player_id(&self) -> PlayerId {
        self.player_id
    }

    fn on_event(
        &mut self,
        event: &GameEvent,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) {
        let _ = event;
        let _ = context;

        todo!("implement PlayerObserver for cli agent")
    }
}

impl Agent for CliAgent {
    fn init_stage_action(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> catan_core::agent::action::InitStageAction {
        todo!()
    }

    fn init_action(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> InitAction {
        todo!()
    }

    fn after_dice_action(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> PostDiceAction {
        todo!()
    }

    fn after_dev_card_action(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> PostDevCardAction {
        todo!()
    }

    fn regular_action(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> RegularAction {
        todo!()
    }

    fn move_robbers(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> catan_core::agent::action::MoveRobbersAction {
        todo!()
    }

    fn choose_player_to_rob(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> catan_core::agent::action::ChoosePlayerToRobAction {
        todo!()
    }

    fn answer_trade(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> TradeAnswer {
        todo!()
    }

    fn drop_half(
        &mut self,
        context: &catan_core::gameplay::game::event::PlayerContext,
    ) -> catan_core::agent::action::DropHalfAction {
        todo!()
    }
}

impl SharedTerminalUi {
    // pub fn prompt(&self, request: AgentRequest) -> AgentAction {
    //     let _guard = self.inner.lock().expect("terminal mutex poisoned");
    //     TerminalUi::prompt_locked(request)
    // }
}

impl TerminalUi {
    // fn prompt_locked(request: AgentRequest) -> AgentAction {
    //     match request {
    //         AgentRequest::Init(context) => {
    //             Self::print_context("Init", &context);
    //             loop {
    //                 let line = Self::read_line("command [throw-dice]: ");
    //                 match line.as_str() {
    //                     "throw-dice" | "roll" | "" => {
    //                         return AgentAction::Init(InitAction::RollDice);
    //                     }
    //                     _ => println!("supported commands: throw-dice"),
    //                 }
    //             }
    //         }
    //         AgentRequest::AfterDevCard(context) => {
    //             Self::print_context("AfterDevCard", &context);
    //             AgentAction::AfterDevCard(PostDevCardAction::RollDice)
    //         }
    //         AgentRequest::AfterDiceThrow(context) => {
    //             Self::print_context("AfterDiceThrow", &context);
    //             AgentAction::AfterDice(Self::read_post_dice_action())
    //         }
    //         AgentRequest::Rest(context) => {
    //             Self::print_context("Rest", &context);
    //             AgentAction::Rest(Self::read_rest_action())
    //         }
    //         AgentRequest::RobHex(context) => {
    //             Self::print_context("RobHex", &context);
    //             AgentAction::RobHex(Self::read_hex("robber hex (#hex): "))
    //         }
    //         AgentRequest::RobPlayer(context) => {
    //             Self::print_context("RobPlayer", &context);
    //             AgentAction::RobPlayer(Self::read_player_id("robbed player id: "))
    //         }
    //         AgentRequest::Initialization(context) => {
    //             Self::print_context("Initialization", &context);
    //             let establishment = Establishment {
    //                 pos: Self::read_intersection("settlement (h1, h2, h3): "),
    //                 stage: EstablishmentType::Settlement,
    //             };
    //             let road = Road {
    //                 pos: Self::read_path("road (h1, h2): "),
    //             };
    //             AgentAction::Initialization {
    //                 establishment,
    //                 road,
    //             }
    //         }
    //         AgentRequest::AnswerTrade { context, trade } => {
    //             Self::print_context("AnswerTrade", &context);
    //             println!("trade offer give={:?} take={:?}", trade.give, trade.take);
    //             let line = Self::read_line("answer [accept/decline]: ");
    //             let action = match line.as_str() {
    //                 "accept" | "yes" | "y" => TradeAnswer::Accepted,
    //                 _ => TradeAnswer::Declined,
    //             };
    //             AgentAction::AnswerTrade(action)
    //         }
    //         AgentRequest::DropHalf(context) => {
    //             Self::print_context("DropHalf", &context);
    //             println!("enter five counts in order: brick wood wheat sheep ore");
    //             AgentAction::DropHalf(Self::read_resource_collection("drop counts: "))
    //         }
    //     }
    // }

    fn print_context(label: &str, context: &PlayerContext) {
        let mut renderer = ui::field_render::FieldRenderer::new();
        renderer.draw_context(context);
        renderer.render();

        // TODO: colorize
        println!("\n== {label} ==");
        println!("player: {:?}", context.player_data);
        println!("resources: {:?}", context.player_data.resources);

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
        print_dev_card_collection("Active: ", context.player_data.dev_cards.active);
        print_dev_card_collection("Queued: ", context.player_data.dev_cards.queued);
        print_dev_card_collection("Used  : ", context.player_data.dev_cards.used);
        println!("-------------------------------------------");

        println!("robber: {:?}", context.view.field.robber_pos);
        for (id, data) in context.view.players.iter().enumerate() {
            println!(
                "opponent {} => resource cards: {}, active dev: {}, queued dev: {}",
                id, data.resources, data.dev_cards.active, data.dev_cards.queued,
            );
        }
    }

    fn read_post_dice_action() -> PostDiceAction {
        loop {
            let line = Self::read_line(
                "command [end | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );

            if line == "end" || line.is_empty() {
                return PostDiceAction::RegularAction(RegularAction::EndMove);
            }

            if let Some(build) = Self::parse_build(&line) {
                return PostDiceAction::RegularAction(RegularAction::Build(build));
            }

            if let Some(trade) = Self::parse_bank_trade(&line) {
                return PostDiceAction::RegularAction(RegularAction::TradeWithBank(trade));
            }

            println!("could not parse action");
        }
    }

    fn read_rest_action() -> RegularAction {
        loop {
            let line = Self::read_line(
                "command [end | build road ... | build settlement ... | build city ... | bank-trade give take kind]: ",
            );
            if line == "end" || line.is_empty() {
                return RegularAction::EndMove;
            }

            if let Some(build) = Self::parse_build(&line) {
                return RegularAction::Build(build);
            }

            if let Some(trade) = Self::parse_bank_trade(&line) {
                return RegularAction::TradeWithBank(trade);
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
