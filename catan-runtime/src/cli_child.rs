use std::{
    collections::BTreeSet,
    io::{self, Stdout, Write},
    os::unix::net::UnixStream,
    path::Path as FsPath,
    sync::{Arc, Mutex},
};

use catan_agents::remote_agent::{
    CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli, RemoteLogLevel, UiModel,
    read_frame, write_frame,
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
use catan_render::{
    adapters::ratatui::canvas_lines,
    field::{FieldOverlay, FieldPreview, FieldRenderer, FieldSelection, SelectionStatus},
    model::{RenderBoard, RenderGameView, RenderPlayerBuilds},
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

#[derive(Clone)]
struct SocketLogWriter {
    stream: Arc<Mutex<UnixStream>>,
}

impl Write for SocketLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let raw = String::from_utf8_lossy(buf);
        let (level, target, message) = parse_socket_log_line(&raw);
        let mut stream = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("CLI log socket mutex poisoned"))?;
        write_frame(
            &mut *stream,
            &CliToHost::Log {
                level,
                target,
                message,
            },
        )?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut stream = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("CLI log socket mutex poisoned"))?;
        stream.flush()
    }
}

fn init_socket_logger(stream: UnixStream) {
    let writer = SocketLogWriter {
        stream: Arc::new(Mutex::new(stream)),
    };
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.target(env_logger::Target::Pipe(Box::new(writer)));
    builder.format(|buf, record| {
        writeln!(
            buf,
            "{}\t{}\t{}",
            record.level(),
            record.target(),
            record.args()
        )
    });
    if let Err(err) = builder.try_init() {
        eprintln!("failed to initialize CLI socket logger: {err}");
    }
}

fn parse_socket_log_line(raw: &str) -> (RemoteLogLevel, String, String) {
    let raw = raw.trim_end_matches(['\r', '\n']);
    let mut parts = raw.splitn(3, '\t');
    let level = parts
        .next()
        .and_then(parse_remote_log_level)
        .unwrap_or(RemoteLogLevel::Info);
    let target = parts
        .next()
        .unwrap_or("catan_runtime::cli_child")
        .to_owned();
    let message = parts.next().unwrap_or(raw).to_owned();
    (level, target, message)
}

fn parse_remote_log_level(raw: &str) -> Option<RemoteLogLevel> {
    match raw {
        "ERROR" => Some(RemoteLogLevel::Error),
        "WARN" => Some(RemoteLogLevel::Warn),
        "INFO" => Some(RemoteLogLevel::Info),
        "DEBUG" => Some(RemoteLogLevel::Debug),
        "TRACE" => Some(RemoteLogLevel::Trace),
        _ => None,
    }
}

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
        log::trace!("Received message from host: {}:{}", file!(), line!());
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
                ui.show_model(&view, format!("event: {event:?}"))
                    .map_err(|err| format!("failed to draw TUI: {err}"))?;
            }
            HostToCli::DecisionRequest(request) => {
                log::trace!("Processing decision request: {}:{}", file!(), line!());
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
    log::trace!("Handling decision request: {}:{}", file!(), line!());
    match request {
        DecisionRequestFrame::InitStage(model) => {
            log::trace!("Processing InitStage decision");
            let settlement = read_initial_settlement(ui, &model, "settlement: ")?;
            log::trace!("Selected settlement: {:?}", settlement);
            let road = read_initial_road(ui, &model, settlement, "road: ")?;
            log::trace!("Selected road: {:?}", road);
            Ok(DecisionResponseFrame::InitStage(InitStageAction {
                establishment_position: settlement,
                road,
            }))
        }
        DecisionRequestFrame::InitAction(model) => {
            log::trace!("Processing InitAction decision");
            let action = read_init_action(ui, &model)?;
            log::trace!("Init action result: {:?}", action);
            Ok(DecisionResponseFrame::InitAction(action))
        }
        DecisionRequestFrame::PostDice(model) => {
            log::trace!("Processing PostDice decision");
            let action = read_post_dice_action(ui, &model)?;
            log::trace!("Post-dice action result: {:?}", action);
            Ok(DecisionResponseFrame::PostDice(action))
        }
        DecisionRequestFrame::PostDevCard(model) => {
            log::trace!("Processing PostDevCard decision (automatically rolling dice)");
            ui.show_model(&model, "dev card resolved; rolling dice".to_owned())?;
            Ok(DecisionResponseFrame::PostDevCard(
                PostDevCardAction::RollDice,
            ))
        }
        DecisionRequestFrame::Regular(model) => {
            log::trace!("Processing Regular decision");
            let action = read_regular_action(ui, &model)?;
            log::trace!("Regular action result: {:?}", action);
            Ok(DecisionResponseFrame::Regular(action))
        }
        DecisionRequestFrame::MoveRobbers(model) => {
            log::trace!("Processing MoveRobbers decision");
            let hex = read_hex(ui, &model, "robber hex: ")?;
            log::trace!("Selected robber hex: {:?}", hex);
            Ok(DecisionResponseFrame::MoveRobbers(MoveRobbersAction(hex)))
        }
        DecisionRequestFrame::ChoosePlayerToRob(model) => {
            log::trace!("Processing ChoosePlayerToRob decision");
            let player_id = read_player_id(ui, &model, "robbed player id: ")?;
            log::trace!("Selected player to rob: {}", player_id);
            Ok(DecisionResponseFrame::ChoosePlayerToRob(
                ChoosePlayerToRobAction(player_id),
            ))
        }
        DecisionRequestFrame::AnswerTrade(model) => {
            log::trace!("Processing AnswerTrade decision");
            let answer = ui.prompt(&model, "answer trade [y/N]: ")?;
            let answer = match answer.as_str() {
                "y" | "yes" => {
                    log::trace!("Trade accepted");
                    TradeAnswer::Accepted
                }
                _ => {
                    log::trace!("Trade declined");
                    TradeAnswer::Declined
                }
            };
            Ok(DecisionResponseFrame::AnswerTrade(answer))
        }
        DecisionRequestFrame::DropHalf(model) => {
            log::trace!("Processing DropHalf decision");
            let resources =
                read_resource_collection(ui, &model, "drop brick wood wheat sheep ore: ")?;
            log::trace!("Resources to drop: {:?}", resources);
            Ok(DecisionResponseFrame::DropHalf(DropHalfAction(resources)))
        }
    }
}

struct CliUi {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    message: String,
    overlay: FieldOverlay,
}

impl CliUi {
    fn new() -> io::Result<Self> {
        log::trace!("Initializing CLI UI");
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        log::trace!("CLI UI initialized successfully");
        Ok(Self {
            terminal,
            message: "waiting for host".to_owned(),
            overlay: FieldOverlay::default(),
        })
    }

    fn set_message(&mut self, message: String) -> io::Result<()> {
        log::trace!("Setting UI message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.draw(None, "", "")
    }

    fn show_model(&mut self, model: &UiModel, message: String) -> io::Result<()> {
        log::trace!("Showing model with message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.draw(Some(model), "", "")
    }

    fn prompt(&mut self, model: &UiModel, prompt: &str) -> io::Result<String> {
        log::trace!("Prompting user: {}", prompt);
        let mut input = String::new();
        self.message = "enter command".to_owned();
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        loop {
            self.draw(Some(model), prompt, &input)?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        log::trace!("User input: {}", input);
                        return Ok(input.trim().to_owned());
                    }
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

    fn select_hex_where(
        &mut self,
        model: &UiModel,
        prompt: &str,
        is_available: impl Fn(Hex) -> bool,
    ) -> io::Result<Hex> {
        log::trace!("Selecting hex with prompt: {}", prompt);
        let board_hexes = board_hex_set(model);
        let mut selected = Hex::new(0, 0);
        if !board_hexes.contains(&selected) {
            selected = *board_hexes.iter().next().ok_or_else(|| {
                log::error!("No board hexes available for selection");
                io::Error::new(io::ErrorKind::InvalidInput, "selector has no board hexes")
            })?;
        }

        self.message = "select hex with arrows; enter confirms".to_owned();
        loop {
            self.overlay.selected = Some(FieldSelection::Hex(selected));
            self.overlay.status = selection_status(is_available(selected));
            self.draw(Some(model), prompt, &hex_label(selected))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        if !is_available(selected) {
                            log::warn!("Selected hex {:?} is unavailable", selected);
                            self.message = "unavailable hex".to_owned();
                            self.overlay.status = SelectionStatus::Unavailable;
                            continue;
                        }
                        log::trace!("Hex selected: {:?}", selected);
                        self.overlay.selected = None;
                        self.overlay.status = SelectionStatus::Neutral;
                        return Ok(selected);
                    }
                    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                        let old = selected;
                        selected = move_hex_by_key(selected, key.code, &board_hexes);
                        if old != selected {
                            log::trace!("Moved hex from {:?} to {:?}", old, selected);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn select_intersection_where(
        &mut self,
        model: &UiModel,
        prompt: &str,
        is_available: impl Fn(Intersection) -> bool,
    ) -> io::Result<Intersection> {
        log::trace!("Selecting intersection with prompt: {}", prompt);
        let board_hexes = board_hex_set(model);
        let mut hex = Hex::new(0, 0);
        if !board_hexes.contains(&hex) {
            hex = *board_hexes.iter().next().ok_or_else(|| {
                log::error!("No board hexes available for intersection selection");
                io::Error::new(io::ErrorKind::InvalidInput, "selector has no board hexes")
            })?;
        }

        self.message = "stage 1: select hex; enter chooses surrounding vertex".to_owned();
        loop {
            loop {
                self.overlay.selected = Some(FieldSelection::Hex(hex));
                self.draw(
                    Some(model),
                    prompt,
                    &format!("{}; enter for vertices", hex_label(hex)),
                )?;
                if let CrosstermEvent::Key(key) = event::read()?
                    && key.kind == KeyEventKind::Press
                {
                    match key.code {
                        KeyCode::Enter => break,
                        KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                            let old = hex;
                            hex = move_hex_by_key(hex, key.code, &board_hexes);
                            if old != hex {
                                log::trace!("Moved hex from {:?} to {:?}", old, hex);
                            }
                        }
                        _ => {}
                    }
                }
            }

            let intersections = hex.vertices_arr();
            let mut selected = 0;
            self.message = "stage 2: cycle vertices with arrows/tab; esc returns to hex".to_owned();
            loop {
                let intersection = intersections[selected];
                self.overlay.selected = Some(FieldSelection::Intersection(intersection));
                self.overlay.status = selection_status(is_available(intersection));
                self.draw(Some(model), prompt, &intersection_label(intersection))?;
                if let CrosstermEvent::Key(key) = event::read()?
                    && key.kind == KeyEventKind::Press
                {
                    match key.code {
                        KeyCode::Enter => {
                            if !is_available(intersection) {
                                log::warn!(
                                    "Selected intersection {:?} is unavailable",
                                    intersection
                                );
                                self.message = "unavailable intersection".to_owned();
                                self.overlay.status = SelectionStatus::Unavailable;
                                continue;
                            }
                            log::trace!("Intersection selected: {:?}", intersection);
                            self.overlay.selected = None;
                            self.overlay.status = SelectionStatus::Neutral;
                            return Ok(intersection);
                        }
                        KeyCode::Esc => {
                            log::trace!("Returning to hex selection stage");
                            self.message =
                                "stage 1: select hex; enter chooses surrounding vertex".to_owned();
                            self.overlay.status = SelectionStatus::Neutral;
                            break;
                        }
                        KeyCode::Left | KeyCode::Down | KeyCode::Tab => {
                            selected = (selected + 1) % intersections.len();
                            log::trace!("Cycled to intersection index {}", selected);
                        }
                        KeyCode::Right | KeyCode::Up | KeyCode::BackTab => {
                            selected = selected.checked_sub(1).unwrap_or(intersections.len() - 1);
                            log::trace!("Cycled to intersection index {}", selected);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn select_initial_road(
        &mut self,
        model: &UiModel,
        settlement_pos: Intersection,
        prompt: &str,
    ) -> io::Result<Road> {
        log::trace!("Selecting initial road for settlement {:?}", settlement_pos);
        let roads = initial_roads_for_settlement(model, settlement_pos);
        if roads.is_empty() {
            log::error!("No legal initial roads for settlement {:?}", settlement_pos);
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "selected settlement has no legal adjacent initial roads",
            ));
        }

        let actor = model.actor.unwrap_or_default();
        self.overlay.preview = vec![FieldPreview::Establishment {
            player_id: actor,
            establishment: Establishment {
                pos: settlement_pos,
                stage: EstablishmentType::Settlement,
            },
        }];
        self.message = "cycle adjacent initial roads with arrows/tab; enter confirms".to_owned();

        let mut selected = 0;
        loop {
            let road = roads[selected];
            self.overlay.selected = Some(FieldSelection::Path(road.pos));
            self.overlay.status = SelectionStatus::Available;
            self.draw(Some(model), prompt, &path_label(road.pos))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        log::trace!("Road selected: {:?}", road);
                        self.overlay.selected = None;
                        self.overlay.status = SelectionStatus::Neutral;
                        self.overlay.preview.clear();
                        return Ok(road);
                    }
                    KeyCode::Left | KeyCode::Down | KeyCode::Tab => {
                        selected = selected.checked_sub(1).unwrap_or(roads.len() - 1);
                    }
                    KeyCode::Right | KeyCode::Up | KeyCode::BackTab => {
                        selected = (selected + 1) % roads.len();
                    }
                    _ => {}
                }
            }
        }
    }

    fn draw(&mut self, model: Option<&UiModel>, prompt: &str, input: &str) -> io::Result<()> {
        let message = self.message.clone();
        let overlay = self.overlay.clone();
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

                    let field = Paragraph::new(field_lines(model, &overlay))
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
        log::trace!("Dropping CLI UI, cleaning up terminal");
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn model_lines(model: &UiModel) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(format!(
        "actor#: {:?} | robber: {:?} | longest road: {:?} | largest army: {:?}",
        match model.actor {
            Some(id) => format!("{}", id),
            None => "None".to_owned(),
        },
        model.public.board_state.robber_pos.index().to_spiral(),
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
        lines.push(Line::from(format!("your dev cards: {}", private.dev_cards)));
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
    lines.push(Line::from("commands: roll | end | buy dev | build road [h1] [h2] | build settlement [h1] [h2] [h3] | build city [h1] [h2] [h3]"));
    lines.push(Line::from(
        "trades: bank-trade [give] [take] [G4 | G3 | S2]",
    ));
    lines.push(Line::from("dev cards: use knight hex [player|none] | use yop res1 res2 | use monopoly res | use roadbuild h1 h2 h3 h4"));
    lines
}

fn field_lines(model: &UiModel, overlay: &FieldOverlay) -> Vec<Line<'static>> {
    let mut renderer = FieldRenderer::new();
    renderer.draw_game(&render_game_view(model));
    renderer.draw_overlay(overlay);
    canvas_lines(renderer.canvas())
}

fn read_init_action(ui: &mut CliUi, model: &UiModel) -> io::Result<InitAction> {
    log::trace!("Reading init action");
    loop {
        let line = ui.prompt(model, "action [roll]: ")?;
        let line = line.trim();
        if line.is_empty() || line == "roll" {
            log::trace!("Init action: RollDice");
            return Ok(InitAction::RollDice);
        }
        if let Some(usage) = parse_dev_card_usage(line) {
            log::trace!("Init action: UseDevCard({:?})", usage);
            return Ok(InitAction::UseDevCard(usage));
        }
        log::warn!("Could not parse init action: {}", line);
        ui.set_message("could not parse action".to_owned())?;
    }
}

fn read_post_dice_action(ui: &mut CliUi, model: &UiModel) -> io::Result<PostDiceAction> {
    log::trace!("Reading post-dice action");
    loop {
        let line = ui.prompt(model, "action: ")?;
        if let Some(usage) = parse_dev_card_usage(&line) {
            log::trace!("Post-dice action: UseDevCard({:?})", usage);
            return Ok(PostDiceAction::UseDevCard(usage));
        }
        if let Some(action) = parse_regular_action(&line) {
            log::trace!("Post-dice action: RegularAction({:?})", action);
            return Ok(PostDiceAction::RegularAction(action));
        }
        log::warn!("Could not parse post-dice action: {}", line);
        ui.set_message("could not parse action".to_owned())?;
    }
}

fn read_regular_action(ui: &mut CliUi, model: &UiModel) -> io::Result<RegularAction> {
    log::trace!("Reading regular action");
    loop {
        let line = ui.prompt(model, "action: ")?;
        if let Some(action) = parse_regular_action(&line) {
            log::trace!("Regular action: {:?}", action);
            return Ok(action);
        }
        log::warn!("Could not parse regular action: {}", line);
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
                "G4" => BankTradeKind::BankGeneric,
                "G3" => BankTradeKind::PortGeneric,
                "S2" => BankTradeKind::PortSpecific,
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
    match token.to_lowercase().as_str() {
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
    log::trace!("Reading resource collection");
    loop {
        let line = ui.prompt(model, prompt)?;
        let parts = line
            .split_whitespace()
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>();
        match parts {
            Ok(parts) if parts.len() == 5 => {
                let resources = ResourceCollection {
                    brick: parts[0],
                    wood: parts[1],
                    wheat: parts[2],
                    sheep: parts[3],
                    ore: parts[4],
                };
                log::trace!("Resource collection read: {:?}", resources);
                return Ok(resources);
            }
            _ => {
                log::warn!("Invalid resource collection input: {}", line);
                ui.set_message("expected five unsigned integers".to_owned())?
            }
        }
    }
}

fn read_initial_settlement(
    ui: &mut CliUi,
    model: &UiModel,
    prompt: &str,
) -> io::Result<Intersection> {
    let legal = legal_initial_settlements(model);
    ui.select_intersection_where(model, prompt, |intersection| legal.contains(&intersection))
}

fn read_initial_road(
    ui: &mut CliUi,
    model: &UiModel,
    settlement: Intersection,
    prompt: &str,
) -> io::Result<Road> {
    ui.select_initial_road(model, settlement, prompt)
}

fn read_hex(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<Hex> {
    let robber_pos = model.public.board_state.robber_pos;
    log::trace!("Reading hex (robber at {:?})", robber_pos);
    ui.select_hex_where(model, prompt, |hex| hex != robber_pos)
}

fn read_player_id(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<PlayerId> {
    log::trace!("Reading player ID");
    loop {
        let line = ui.prompt(model, prompt)?;
        if let Ok(id) = line.parse() {
            log::trace!("Player ID read: {}", id);
            return Ok(id);
        }
        log::warn!("Invalid player ID input: {}", line);
        ui.set_message("expected unsigned integer".to_owned())?;
    }
}

fn hex_label(hex: Hex) -> String {
    format!("hex {} (q={}, r={})", hex.index().to_spiral(), hex.q, hex.r)
}

fn path_label(path: BoardPath) -> String {
    let (a, b) = path.as_pair();
    format!(
        "road {} {} ({},{} -> {},{})",
        a.index().to_spiral(),
        b.index().to_spiral(),
        a.q,
        a.r,
        b.q,
        b.r
    )
}

fn intersection_label(intersection: Intersection) -> String {
    let label = intersection
        .as_set()
        .into_iter()
        .map(|hex| hex.index().to_spiral().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    format!("intersection {label}")
}

fn selection_status(is_available: bool) -> SelectionStatus {
    if is_available {
        SelectionStatus::Available
    } else {
        SelectionStatus::Unavailable
    }
}

fn move_hex_by_key(current: Hex, key: KeyCode, board_hexes: &BTreeSet<Hex>) -> Hex {
    let next = match key {
        KeyCode::Left => Hex::new(current.q - 1, current.r),
        KeyCode::Right => Hex::new(current.q + 1, current.r),
        KeyCode::Up => Hex::new(current.q, current.r - 1),
        KeyCode::Down => Hex::new(current.q, current.r + 1),
        _ => current,
    };

    if board_hexes.contains(&next) {
        next
    } else {
        current
    }
}

fn board_hexes(model: &UiModel) -> Vec<Hex> {
    (0..model.public.board.tiles.len())
        .map(HexIndex::spiral_to_hex)
        .collect()
}

fn board_hex_set(model: &UiModel) -> BTreeSet<Hex> {
    board_hexes(model).into_iter().collect()
}

fn board_path_set(model: &UiModel) -> BTreeSet<BoardPath> {
    board_hexes(model)
        .into_iter()
        .flat_map(|hex| hex.paths_arr())
        .collect()
}

fn board_intersection_set(model: &UiModel) -> BTreeSet<Intersection> {
    board_hexes(model)
        .into_iter()
        .flat_map(|hex| hex.vertices_arr())
        .collect()
}

fn occupied_roads(model: &UiModel) -> BTreeSet<BoardPath> {
    model
        .public
        .builds
        .iter()
        .flat_map(|builds| builds.roads.iter().map(|road| road.pos))
        .collect()
}

fn occupied_intersections(model: &UiModel) -> BTreeSet<Intersection> {
    model
        .public
        .builds
        .iter()
        .flat_map(|builds| builds.establishments.iter().map(|est| est.pos))
        .collect()
}

fn settlement_deadzone(model: &UiModel) -> BTreeSet<Intersection> {
    occupied_intersections(model)
        .into_iter()
        .flat_map(|intersection| {
            intersection
                .neighbors()
                .into_iter()
                .chain(std::iter::once(intersection))
        })
        .collect()
}

fn legal_initial_settlements(model: &UiModel) -> BTreeSet<Intersection> {
    let deadzone = settlement_deadzone(model);
    let settlements = board_intersection_set(model)
        .into_iter()
        .filter(|intersection| !deadzone.contains(intersection))
        .filter(|intersection| !initial_roads_for_settlement(model, *intersection).is_empty())
        .collect::<BTreeSet<_>>();
    settlements
}

fn initial_roads_for_settlement(model: &UiModel, settlement: Intersection) -> Vec<Road> {
    let valid_paths = board_path_set(model);
    let occupied = occupied_roads(model);
    let roads = settlement
        .paths()
        .into_iter()
        .filter(|path| valid_paths.contains(path))
        .filter(|path| !occupied.contains(path))
        .map(|pos| Road { pos })
        .collect::<Vec<_>>();

    roads
}

fn render_game_view(model: &UiModel) -> RenderGameView {
    RenderGameView {
        board: RenderBoard {
            n_players: model.public.board.n_players,
            field_radius: model.public.board.field_radius,
            tiles: model.public.board.tiles.clone(),
            ports: model.public.board.ports.clone(),
        },
        board_state: model.public.board_state,
        builds: model
            .public
            .builds
            .iter()
            .map(|builds| RenderPlayerBuilds {
                player_id: builds.player_id,
                establishments: builds.establishments.clone(),
                roads: builds.roads.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catan_agents::remote_agent::UiModel;
    use catan_core::gameplay::game::{
        index::GameIndex,
        init::GameInitializationState,
        state::GameState,
        view::{ContextFactory, VisibilityConfig},
    };

    fn hex_set(hexes: impl IntoIterator<Item = Hex>) -> BTreeSet<Hex> {
        hexes.into_iter().collect()
    }

    fn model_from_state(state: &GameState) -> UiModel {
        let index = GameIndex::rebuild(state);
        let visibility = VisibilityConfig::default();
        let factory = ContextFactory {
            state,
            index: &index,
            visibility: &visibility,
        };
        UiModel::from_decision(&factory.player_decision_context(0, None))
    }

    #[test]
    fn hex_arrow_navigation_changes_q_and_r_coordinates() {
        let board = hex_set([
            Hex::new(0, 0),
            Hex::new(1, 0),
            Hex::new(-1, 0),
            Hex::new(0, -1),
            Hex::new(0, 1),
        ]);

        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Right, &board),
            Hex::new(1, 0)
        );
        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Left, &board),
            Hex::new(-1, 0)
        );
        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Up, &board),
            Hex::new(0, -1)
        );
        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Down, &board),
            Hex::new(0, 1)
        );
    }

    #[test]
    fn hex_arrow_navigation_stays_on_board() {
        let board = hex_set([Hex::new(0, 0)]);

        assert_eq!(
            move_hex_by_key(Hex::new(0, 0), KeyCode::Right, &board),
            Hex::new(0, 0)
        );
    }

    #[test]
    fn initial_settlement_legality_excludes_existing_deadzone() {
        let mut init = GameInitializationState::default();
        let (settlement, road) = init
            .builds
            .query()
            .possible_initial_placements(&init.board, 0)
            .into_iter()
            .next()
            .expect("default board should have an initial placement");
        init.builds
            .try_init_place(0, road, settlement)
            .expect("generated initial placement should be valid");

        let state = init.finish();
        let model = model_from_state(&state);
        let legal = legal_initial_settlements(&model);

        assert!(!legal.contains(&settlement.pos));
        for neighbor in settlement.pos.neighbors() {
            assert!(!legal.contains(&neighbor));
        }
    }

    #[test]
    fn initial_roads_are_adjacent_on_board_and_unoccupied() {
        let init = GameInitializationState::default();
        let model = model_from_state(&init.finish());
        let settlement = *legal_initial_settlements(&model)
            .iter()
            .next()
            .expect("default board should have legal settlements");

        let roads = initial_roads_for_settlement(&model, settlement);
        assert!(!roads.is_empty());
        assert!(roads.len() <= 3);
        assert!(roads.iter().all(|road| {
            road.pos
                .intersections_iter()
                .any(|intersection| intersection == settlement)
        }));
    }

    #[test]
    fn socket_log_line_preserves_level_target_and_message() {
        let (level, target, message) =
            parse_socket_log_line("TRACE\tcatan_runtime::cli_child\tselected road\n");

        assert_eq!(level, RemoteLogLevel::Trace);
        assert_eq!(target, "catan_runtime::cli_child");
        assert_eq!(message, "selected road");
    }
}
