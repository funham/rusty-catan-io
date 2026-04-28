use std::{
    collections::BTreeSet,
    io::{self, Stdout},
    os::unix::net::UnixStream,
    path::Path as FsPath,
};

use catan_agents::{
    cli_agent::ui::field_render::FieldRenderer,
    remote_agent::{
        CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli, UiModel, read_frame,
        write_frame,
    },
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
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn run(socket: &FsPath, _role: &str) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;
    let mut ui = CliUi::new().map_err(|err| format!("failed to initialize TUI: {err}"))?;
    match read_frame::<HostToCli>(&mut stream)
        .map_err(|err| format!("failed to read hello: {err}"))?
    {
        HostToCli::Hello { role } => {
            ui.set_message(format!("connected as {role:?}"))
                .map_err(|err| format!("failed to draw TUI: {err}"))?;
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
                ui.set_message(format!("shutdown: {reason}"))
                    .map_err(|err| format!("failed to draw TUI: {err}"))?;
                return Ok(());
            }
            HostToCli::Event { event, view } => {
                ui.show_model(&view, format!("event: {event:?}"))
                    .map_err(|err| format!("failed to draw TUI: {err}"))?;
            }
            HostToCli::DecisionRequest(request) => {
                let response = handle_decision(&mut ui, request)
                    .map_err(|err| format!("failed to handle decision: {err}"))?;
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
    match request {
        DecisionRequestFrame::InitStage(model) => {
            Ok(DecisionResponseFrame::InitStage(InitStageAction {
                establishment_position: read_intersection(ui, &model, "settlement (h1 h2 h3): ")?,
                road: Road {
                    pos: read_path(ui, &model, "road (h1 h2): ")?,
                },
            }))
        }
        DecisionRequestFrame::InitAction(model) => Ok(DecisionResponseFrame::InitAction(
            read_init_action(ui, &model)?,
        )),
        DecisionRequestFrame::PostDice(model) => Ok(DecisionResponseFrame::PostDice(
            read_post_dice_action(ui, &model)?,
        )),
        DecisionRequestFrame::PostDevCard(model) => {
            ui.show_model(&model, "dev card resolved; rolling dice".to_owned())?;
            Ok(DecisionResponseFrame::PostDevCard(
                PostDevCardAction::RollDice,
            ))
        }
        DecisionRequestFrame::Regular(model) => Ok(DecisionResponseFrame::Regular(
            read_regular_action(ui, &model)?,
        )),
        DecisionRequestFrame::MoveRobbers(model) => Ok(DecisionResponseFrame::MoveRobbers(
            MoveRobbersAction(read_hex(ui, &model, "robber hex: ")?),
        )),
        DecisionRequestFrame::ChoosePlayerToRob(model) => {
            Ok(DecisionResponseFrame::ChoosePlayerToRob(
                ChoosePlayerToRobAction(read_player_id(ui, &model, "robbed player id: ")?),
            ))
        }
        DecisionRequestFrame::AnswerTrade(model) => {
            let answer = ui.prompt(&model, "answer trade [y/N]: ")?;
            let answer = match answer.as_str() {
                "y" | "yes" => TradeAnswer::Accepted,
                _ => TradeAnswer::Declined,
            };
            Ok(DecisionResponseFrame::AnswerTrade(answer))
        }
        DecisionRequestFrame::DropHalf(model) => {
            Ok(DecisionResponseFrame::DropHalf(DropHalfAction(
                read_resource_collection(ui, &model, "drop brick wood wheat sheep ore: ")?,
            )))
        }
    }
}

struct CliUi {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    message: String,
}

impl CliUi {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self {
            terminal,
            message: "waiting for host".to_owned(),
        })
    }

    fn set_message(&mut self, message: String) -> io::Result<()> {
        self.message = message;
        self.draw(None, "", "")
    }

    fn show_model(&mut self, model: &UiModel, message: String) -> io::Result<()> {
        self.message = message;
        self.draw(Some(model), "", "")
    }

    fn prompt(&mut self, model: &UiModel, prompt: &str) -> io::Result<String> {
        let mut input = String::new();
        self.message = "enter command".to_owned();
        loop {
            self.draw(Some(model), prompt, &input)?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => return Ok(input.trim().to_owned()),
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Esc => {
                        input.clear();
                    }
                    KeyCode::Char(c) => input.push(c),
                    _ => {}
                }
            }
        }
    }

    fn select<T: Copy>(
        &mut self,
        model: &UiModel,
        prompt: &str,
        choices: &[(String, T)],
    ) -> io::Result<T> {
        if choices.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "selector has no choices",
            ));
        }

        let mut selected = 0;
        self.message = "select with arrows/tab; enter confirms".to_owned();
        loop {
            self.draw(Some(model), prompt, &choices[selected].0)?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => return Ok(choices[selected].1),
                    KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
                        selected = (selected + 1) % choices.len();
                    }
                    KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
                        selected = selected.checked_sub(1).unwrap_or(choices.len() - 1);
                    }
                    KeyCode::Home => selected = 0,
                    KeyCode::End => selected = choices.len() - 1,
                    _ => {}
                }
            }
        }
    }

    fn draw(&mut self, model: Option<&UiModel>, prompt: &str, input: &str) -> io::Result<()> {
        let message = self.message.clone();
        self.terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(5),
                ])
                .split(frame.area());

            let title = Paragraph::new(Line::from(vec![
                Span::styled("rusty-catan", Style::default().fg(Color::Green)),
                Span::raw("  "),
                Span::raw(message.as_str()),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Status"));
            frame.render_widget(title, chunks[0]);

            match model {
                Some(model) => {
                    let body_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Length(68), Constraint::Min(30)])
                        .split(chunks[1]);

                    let field = Paragraph::new(field_lines(model))
                        .block(Block::default().borders(Borders::ALL).title("Field"));
                    frame.render_widget(field, body_chunks[0]);

                    let details = Paragraph::new(model_lines(model))
                        .wrap(Wrap { trim: false })
                        .block(Block::default().borders(Borders::ALL).title("Game"));
                    frame.render_widget(details, body_chunks[1]);
                }
                None => {
                    let body = Paragraph::new(vec![Line::from("waiting for game state")])
                        .wrap(Wrap { trim: false })
                        .block(Block::default().borders(Borders::ALL).title("Game"));
                    frame.render_widget(body, chunks[1]);
                }
            }

            let input = Paragraph::new(vec![
                Line::from(prompt.to_owned()),
                Line::from(Span::styled(
                    input.to_owned(),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(
                    "Text: Esc clears, Enter submits. Select: arrows/tab move, Enter picks.",
                ),
            ])
            .block(Block::default().borders(Borders::ALL).title("Command"));
            frame.render_widget(input, chunks[2]);
        })?;
        Ok(())
    }
}

impl Drop for CliUi {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn model_lines(model: &UiModel) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(format!(
        "actor: {:?} | robber: {:?} | longest road: {:?} | largest army: {:?}",
        model.actor,
        model.public.board_state.robber_pos,
        model.public.longest_road_owner,
        model.public.largest_army_owner
    )));
    lines.push(Line::from(format!(
        "board: radius {} | tiles {} | dev cards in bank {}",
        model.public.board.field_radius,
        model.public.board.tiles.len(),
        model.public.bank.dev_card_count
    )));
    if let Some(private) = &model.private {
        lines.push(Line::from(format!(
            "you: p{} resources {}",
            private.player_id, private.resources
        )));
        lines.push(Line::from(format!(
            "your dev cards: {:?}",
            private.dev_cards
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("players:"));
    for player in &model.public.players {
        lines.push(Line::from(format!(
            "  p{} resources {:?} active_dev={} queued_dev={} vp={:?}",
            player.player_id,
            player.resources,
            player.active_dev_cards,
            player.queued_dev_cards,
            player.victory_points
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("commands: roll | end | buy dev | build road h1 h2 | build settlement h1 h2 h3 | build city h1 h2 h3"));
    lines.push(Line::from("trades: bank-trade give take common"));
    lines.push(Line::from("dev cards: use knight hex [player|none] | use yop res1 res2 | use monopoly res | use roadbuild h1 h2 h3 h4"));
    lines
}

fn field_lines(model: &UiModel) -> Vec<Line<'static>> {
    let mut renderer = FieldRenderer::new();
    renderer.draw_ui_public(&model.public);
    renderer.plain_lines().into_iter().map(Line::from).collect()
}

fn read_init_action(ui: &mut CliUi, model: &UiModel) -> io::Result<InitAction> {
    loop {
        let line = ui.prompt(model, "action [roll]: ")?;
        let line = line.trim();
        if line.is_empty() || line == "roll" {
            return Ok(InitAction::RollDice);
        }
        if let Some(usage) = parse_dev_card_usage(line) {
            return Ok(InitAction::UseDevCard(usage));
        }
        ui.set_message("could not parse action".to_owned())?;
    }
}

fn read_post_dice_action(ui: &mut CliUi, model: &UiModel) -> io::Result<PostDiceAction> {
    loop {
        let line = ui.prompt(model, "action: ")?;
        if let Some(usage) = parse_dev_card_usage(&line) {
            return Ok(PostDiceAction::UseDevCard(usage));
        }
        if let Some(action) = parse_regular_action(&line) {
            return Ok(PostDiceAction::RegularAction(action));
        }
        ui.set_message("could not parse action".to_owned())?;
    }
}

fn read_regular_action(ui: &mut CliUi, model: &UiModel) -> io::Result<RegularAction> {
    loop {
        let line = ui.prompt(model, "action: ")?;
        if let Some(action) = parse_regular_action(&line) {
            return Ok(action);
        }
        ui.set_message("could not parse action".to_owned())?;
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

fn read_resource_collection(
    ui: &mut CliUi,
    model: &UiModel,
    prompt: &str,
) -> io::Result<ResourceCollection> {
    loop {
        let line = ui.prompt(model, prompt)?;
        let parts = line
            .split_whitespace()
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>();
        match parts {
            Ok(parts) if parts.len() == 5 => {
                return Ok(ResourceCollection {
                    brick: parts[0],
                    wood: parts[1],
                    wheat: parts[2],
                    sheep: parts[3],
                    ore: parts[4],
                });
            }
            _ => ui.set_message("expected five unsigned integers".to_owned())?,
        }
    }
}

fn read_hex(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<Hex> {
    ui.select(model, prompt, &hex_choices(model))
}

fn read_path(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<BoardPath> {
    ui.select(model, prompt, &path_choices(model))
}

fn read_intersection(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<Intersection> {
    ui.select(model, prompt, &intersection_choices(model))
}

fn read_player_id(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<PlayerId> {
    loop {
        let line = ui.prompt(model, prompt)?;
        if let Ok(id) = line.parse() {
            return Ok(id);
        }
        ui.set_message("expected unsigned integer".to_owned())?;
    }
}

fn hex_choices(model: &UiModel) -> Vec<(String, Hex)> {
    board_hexes(model)
        .into_iter()
        .map(|hex| (format!("hex {}", hex.index().to_spiral()), hex))
        .collect()
}

fn path_choices(model: &UiModel) -> Vec<(String, BoardPath)> {
    board_hexes(model)
        .into_iter()
        .flat_map(|hex| hex.paths_arr())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|path| {
            let (a, b) = path.as_pair();
            (
                format!("road {} {}", a.index().to_spiral(), b.index().to_spiral()),
                path,
            )
        })
        .collect()
}

fn intersection_choices(model: &UiModel) -> Vec<(String, Intersection)> {
    board_hexes(model)
        .into_iter()
        .flat_map(|hex| hex.vertices_arr())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|intersection| {
            let label = intersection
                .as_set()
                .into_iter()
                .map(|hex| hex.index().to_spiral().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            (format!("intersection {label}"), intersection)
        })
        .collect()
}

fn board_hexes(model: &UiModel) -> Vec<Hex> {
    (0..model.public.board.tiles.len())
        .map(HexIndex::spiral_to_hex)
        .collect()
}
