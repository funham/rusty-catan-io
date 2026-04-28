use std::{
    io::{self, BufRead, Write},
    os::unix::net::UnixStream,
    path::Path as FsPath,
};

use catan_agents::remote_agent::{
    CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli, UiModel, read_frame,
    write_frame,
};
use catan_core::{
    agent::action::{
        ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction, MoveRobbersAction,
        PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
    },
    gameplay::primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        dev_card::DevCardUsage,
        player::PlayerId,
        resource::{Resource, ResourceCollection},
        trade::{BankTrade, BankTradeKind},
    },
    topology::{Hex, HexIndex, Intersection, Path as BoardPath, repr::Dual},
};

pub fn run(socket: &FsPath, _role: &str) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;
    match read_frame::<HostToCli>(&mut stream)
        .map_err(|err| format!("failed to read hello: {err}"))?
    {
        HostToCli::Hello { role } => {
            println!("connected as {role:?}");
            write_frame(&mut stream, &CliToHost::Ready)
                .map_err(|err| format!("failed to send ready: {err}"))?;
        }
        other => return Err(format!("expected hello, got {other:?}")),
    }

    loop {
        let msg = match read_frame::<HostToCli>(&mut stream) {
            Ok(msg) => msg,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(
                    "host closed the CLI socket without a shutdown message; check the host terminal for a panic or startup error"
                        .to_owned(),
                );
            }
            Err(err) => return Err(format!("failed to read host frame: {err}")),
        };
        match msg {
            HostToCli::Hello { .. } => {}
            HostToCli::Shutdown { reason } => {
                println!("shutdown: {reason}");
                return Ok(());
            }
            HostToCli::Event { event, view } => {
                print_model(&view);
                println!("event: {event:?}");
            }
            HostToCli::DecisionRequest(request) => {
                let response = handle_decision(request);
                write_frame(&mut stream, &CliToHost::DecisionResponse(response))
                    .map_err(|err| format!("failed to send decision response: {err}"))?;
            }
        }
    }
}

fn handle_decision(request: DecisionRequestFrame) -> DecisionResponseFrame {
    match request {
        DecisionRequestFrame::InitStage(model) => {
            print_model(&model);
            DecisionResponseFrame::InitStage(InitStageAction {
                establishment_position: read_intersection("settlement (h1 h2 h3): "),
                road: Road {
                    pos: read_path("road (h1 h2): "),
                },
            })
        }
        DecisionRequestFrame::InitAction(model) => {
            print_model(&model);
            DecisionResponseFrame::InitAction(read_init_action())
        }
        DecisionRequestFrame::PostDice(model) => {
            print_model(&model);
            DecisionResponseFrame::PostDice(read_post_dice_action())
        }
        DecisionRequestFrame::PostDevCard(model) => {
            print_model(&model);
            DecisionResponseFrame::PostDevCard(PostDevCardAction::RollDice)
        }
        DecisionRequestFrame::Regular(model) => {
            print_model(&model);
            DecisionResponseFrame::Regular(read_regular_action())
        }
        DecisionRequestFrame::MoveRobbers(model) => {
            print_model(&model);
            DecisionResponseFrame::MoveRobbers(MoveRobbersAction(read_hex("robber hex: ")))
        }
        DecisionRequestFrame::ChoosePlayerToRob(model) => {
            print_model(&model);
            DecisionResponseFrame::ChoosePlayerToRob(ChoosePlayerToRobAction(read_player_id(
                "robbed player id: ",
            )))
        }
        DecisionRequestFrame::AnswerTrade(model) => {
            print_model(&model);
            let answer = read_line("answer trade [y/N]: ");
            let answer = match answer.as_str() {
                "y" | "yes" => TradeAnswer::Accepted,
                _ => TradeAnswer::Declined,
            };
            DecisionResponseFrame::AnswerTrade(answer)
        }
        DecisionRequestFrame::DropHalf(model) => {
            print_model(&model);
            DecisionResponseFrame::DropHalf(DropHalfAction(read_resource_collection(
                "drop brick wood wheat sheep ore: ",
            )))
        }
    }
}

fn print_model(model: &UiModel) {
    print!("\x1b[2J\x1b[H");
    println!("turn: {:?}", model.public.board_state.robber_pos);
    println!("actor: {:?}", model.actor);
    if let Some(private) = &model.private {
        println!(
            "you: p{} resources {}",
            private.player_id, private.resources
        );
        println!("dev cards: {:?}", private.dev_cards);
    }
    println!("players:");
    for player in &model.public.players {
        println!(
            "  p{} resources {:?} active_dev={} queued_dev={} vp={:?}",
            player.player_id,
            player.resources,
            player.active_dev_cards,
            player.queued_dev_cards,
            player.victory_points,
        );
    }
    println!(
        "longest road: {:?}; largest army: {:?}",
        model.public.longest_road_owner, model.public.largest_army_owner
    );
    println!(
        "commands: roll | end | buy dev | build road h1 h2 | build settlement h1 h2 h3 | build city h1 h2 h3 | bank-trade give take common"
    );
    println!(
        "dev cards: use knight hex [player|none] | use yop res1 res2 | use monopoly res | use roadbuild h1 h2 h3 h4"
    );
}

fn read_init_action() -> InitAction {
    loop {
        let line = read_line("action [roll]: ");
        let line = line.trim();
        if line.is_empty() || line == "roll" {
            return InitAction::RollDice;
        }
        if let Some(usage) = parse_dev_card_usage(line) {
            return InitAction::UseDevCard(usage);
        }
        println!("could not parse action");
    }
}

fn read_post_dice_action() -> PostDiceAction {
    loop {
        let line = read_line("action: ");
        if let Some(usage) = parse_dev_card_usage(&line) {
            return PostDiceAction::UseDevCard(usage);
        }
        if let Some(action) = parse_regular_action(&line) {
            return PostDiceAction::RegularAction(action);
        }
        println!("could not parse action");
    }
}

fn read_regular_action() -> RegularAction {
    loop {
        let line = read_line("action: ");
        if let Some(action) = parse_regular_action(&line) {
            return action;
        }
        println!("could not parse action");
    }
}

fn parse_regular_action(line: &str) -> Option<RegularAction> {
    let line = line.trim();
    if line == "end" || line.is_empty() {
        return Some(RegularAction::EndMove);
    }
    if line == "buy dev" || line == "buy-dev" {
        return Some(RegularAction::BuyDevCard);
    }
    if let Some(build) = parse_build(line) {
        return Some(RegularAction::Build(build));
    }
    if let Some(trade) = parse_bank_trade(line) {
        return Some(RegularAction::TradeWithBank(trade));
    }
    None
}

fn parse_build(line: &str) -> Option<Build> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["build", "road", h1, h2] => Some(Build::Road(Road {
            pos: path_from_tokens(h1, h2)?,
        })),
        ["build", "settlement", h1, h2, h3] => Some(Build::Establishment(Establishment {
            pos: intersection_from_tokens(h1, h2, h3)?,
            stage: EstablishmentType::Settlement,
        })),
        ["build", "city", h1, h2, h3] => Some(Build::Establishment(Establishment {
            pos: intersection_from_tokens(h1, h2, h3)?,
            stage: EstablishmentType::City,
        })),
        _ => None,
    }
}

fn parse_bank_trade(line: &str) -> Option<BankTrade> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["bank-trade", give, take, kind] => Some(BankTrade {
            give: parse_resource(give)?,
            take: parse_resource(take)?,
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

fn parse_dev_card_usage(line: &str) -> Option<DevCardUsage> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["use", "knight", hex] => Some(DevCardUsage::Knight {
            rob_hex: HexIndex::spiral_to_hex(hex.parse().ok()?),
            robbed_id: None,
        }),
        ["use", "knight", hex, "none"] => Some(DevCardUsage::Knight {
            rob_hex: HexIndex::spiral_to_hex(hex.parse().ok()?),
            robbed_id: None,
        }),
        ["use", "knight", hex, robbed_id] => Some(DevCardUsage::Knight {
            rob_hex: HexIndex::spiral_to_hex(hex.parse().ok()?),
            robbed_id: Some(robbed_id.parse().ok()?),
        }),
        ["use", "yop", first, second] | ["use", "year-of-plenty", first, second] => {
            Some(DevCardUsage::YearOfPlenty([
                parse_resource(first)?,
                parse_resource(second)?,
            ]))
        }
        ["use", "monopoly", resource] => Some(DevCardUsage::Monopoly(parse_resource(resource)?)),
        ["use", "roadbuild", h1, h2, h3, h4] | ["use", "road-build", h1, h2, h3, h4] => {
            Some(DevCardUsage::RoadBuild([
                path_from_tokens(h1, h2)?,
                path_from_tokens(h3, h4)?,
            ]))
        }
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

fn path_from_tokens(h1: &str, h2: &str) -> Option<BoardPath> {
    let h1 = HexIndex::spiral_to_hex(h1.parse().ok()?);
    let h2 = HexIndex::spiral_to_hex(h2.parse().ok()?);
    BoardPath::try_from((h1, h2))
        .or_else(|_| BoardPath::<Dual>::try_from((h1, h2)).map(|path| path.canon()))
        .ok()
}

fn intersection_from_tokens(h1: &str, h2: &str, h3: &str) -> Option<Intersection> {
    Intersection::try_from([
        HexIndex::spiral_to_hex(h1.parse().ok()?),
        HexIndex::spiral_to_hex(h2.parse().ok()?),
        HexIndex::spiral_to_hex(h3.parse().ok()?),
    ])
    .ok()
}

fn read_resource_collection(prompt: &str) -> ResourceCollection {
    loop {
        let line = read_line(prompt);
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
        let line = read_line(prompt);
        if let Ok(index) = line.parse::<usize>() {
            return HexIndex::spiral_to_hex(index);
        }
        println!("expected spiral hex index");
    }
}

fn read_path(prompt: &str) -> BoardPath {
    loop {
        let line = read_line(prompt);
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if let [h1, h2] = parts.as_slice()
            && let Some(path) = path_from_tokens(h1, h2)
        {
            return path;
        }
        println!("expected adjacent hex pair");
    }
}

fn read_intersection(prompt: &str) -> Intersection {
    loop {
        let line = read_line(prompt);
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if let [h1, h2, h3] = parts.as_slice()
            && let Some(intersection) = intersection_from_tokens(h1, h2, h3)
        {
            return intersection;
        }
        println!("expected adjacent hex triplet");
    }
}

fn read_player_id(prompt: &str) -> PlayerId {
    loop {
        let line = read_line(prompt);
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
