use std::{
    collections::BTreeSet,
    io::{self, Stdout, Write},
    os::unix::net::UnixStream,
    path::Path as FsPath,
    sync::{Arc, Mutex},
};

use catan_agents::remote_agent::{
    CliToHost, DecisionRequestFrame, DecisionResponseFrame, HostToCli, RemoteLogLevel, UiModel,
    UiPublicBankResources, UiPublicPlayerResources, read_frame, write_frame,
};
use catan_core::{
    agent::action::{
        ChoosePlayerToRobAction, DropHalfAction, InitAction, InitStageAction, MoveRobbersAction,
        PostDevCardAction, PostDiceAction, RegularAction, TradeAnswer,
    },
    gameplay::{
        game::event::GameEndPlayerStats,
        primitives::{
            PortKind,
            bank::DeckFullnessLevel,
            build::{Build, Establishment, EstablishmentType, Road},
            dev_card::{DevCardData, DevCardUsage, UsableDevCard},
            player::PlayerId,
            resource::{Resource, ResourceCollection},
            trade::{BankTrade, BankTradeKind},
        },
    },
    topology::{Hex, HexIndex, Intersection, Path as BoardPath, repr::Dual},
};
use catan_render::{
    adapters::ratatui::{canvas_lines, color as ratatui_color},
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
            ui.pending_robber_hex = Some(hex);
            log::trace!("Selected robber hex: {:?}", hex);
            Ok(DecisionResponseFrame::MoveRobbers(MoveRobbersAction(hex)))
        }
        DecisionRequestFrame::ChoosePlayerToRob(model) => {
            log::trace!("Processing ChoosePlayerToRob decision");
            let player_id = read_robbed_player(ui, &model, "robbed player: ")?;
            ui.pending_robber_hex = None;
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
                    TradeAnswer::Accept
                }
                _ => {
                    log::trace!("Trade declined");
                    TradeAnswer::Decline
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
    public_override: Option<Vec<Line<'static>>>,
    personal_override: Option<Vec<Line<'static>>>,
    pending_robber_hex: Option<Hex>,
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
            public_override: None,
            personal_override: None,
            pending_robber_hex: None,
        })
    }

    fn set_message(&mut self, message: String) -> io::Result<()> {
        log::trace!("Setting UI message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
        self.draw(None, "", "")
    }

    fn show_model(&mut self, model: &UiModel, message: String) -> io::Result<()> {
        log::trace!("Showing model with message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
        self.draw(Some(model), "", "")
    }

    fn show_game_ended(
        &mut self,
        model: &UiModel,
        winner_id: PlayerId,
        turn_no: u64,
        stats: &[GameEndPlayerStats],
    ) -> io::Result<()> {
        self.message = "game ended".to_owned();
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = Some(game_ended_lines(model, winner_id, turn_no, stats));
        self.personal_override = None;
        loop {
            self.draw(Some(model), "[press esc to quit]", "")?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && key.code == KeyCode::Esc
            {
                self.public_override = None;
                return Ok(());
            }
        }
    }

    fn prompt(&mut self, model: &UiModel, prompt: &str) -> io::Result<String> {
        log::trace!("Prompting user: {}", prompt);
        let mut input = String::new();
        self.message = "enter command".to_owned();
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
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

    fn select_build(
        &mut self,
        model: &UiModel,
        builds: Vec<Build>,
        prompt: &str,
    ) -> io::Result<Option<Build>> {
        if builds.is_empty() {
            self.message = "no legal placements".to_owned();
            return Ok(None);
        }

        let actor = model.actor.unwrap_or_default();
        let mut selected = 0;
        self.message = "cycle placements with arrows/tab; enter confirms; esc cancels".to_owned();
        loop {
            let build = builds[selected];
            self.overlay.selected = Some(match build {
                Build::Road(road) => FieldSelection::Path(road.pos),
                Build::Establishment(establishment) => {
                    FieldSelection::Intersection(establishment.pos)
                }
            });
            self.overlay.status = SelectionStatus::Available;
            self.overlay.preview = vec![match build {
                Build::Road(road) => FieldPreview::Road {
                    player_id: actor,
                    road,
                },
                Build::Establishment(establishment) => FieldPreview::Establishment {
                    player_id: actor,
                    establishment,
                },
            }];
            self.draw(Some(model), prompt, &build_label(build))?;

            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        self.overlay.selected = None;
                        self.overlay.status = SelectionStatus::Neutral;
                        self.overlay.preview.clear();
                        return Ok(Some(build));
                    }
                    KeyCode::Esc => {
                        self.overlay.selected = None;
                        self.overlay.status = SelectionStatus::Neutral;
                        self.overlay.preview.clear();
                        self.message = "selection cancelled".to_owned();
                        return Ok(None);
                    }
                    KeyCode::Left | KeyCode::Down | KeyCode::Tab => {
                        selected = selected.checked_sub(1).unwrap_or(builds.len() - 1);
                    }
                    KeyCode::Right | KeyCode::Up | KeyCode::BackTab => {
                        selected = (selected + 1) % builds.len();
                    }
                    _ => {}
                }
            }
        }
    }

    fn select_drop_cards(&mut self, model: &UiModel) -> io::Result<Option<ResourceCollection>> {
        let Some(private) = &model.private else {
            self.message = "no private resources".to_owned();
            return Ok(None);
        };

        let required = private.resources.total() / 2;
        let mut selected_resource = 0;
        let mut selected = ResourceCollection::ZERO;
        self.message =
            format!("select exactly {required} cards to drop; enter confirms; esc cancels");

        loop {
            self.personal_override = Some(drop_personal_lines(
                private.player_id,
                &private.resources,
                &private.dev_cards,
                &selected,
                required,
                selected_resource,
            ));
            self.draw(
                Some(model),
                "drop: ",
                &format!("selected {} of {}", selected.total(), required),
            )?;

            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                let resource = Resource::list()[selected_resource];
                match key.code {
                    KeyCode::Enter => {
                        if selected.total() == required {
                            self.personal_override = None;
                            return Ok(Some(selected));
                        }
                        self.message =
                            format!("selected {} cards; expected {}", selected.total(), required);
                    }
                    KeyCode::Esc => {
                        self.personal_override = None;
                        self.message = "drop cancelled".to_owned();
                        return Ok(None);
                    }
                    KeyCode::Left => {
                        selected_resource = selected_resource
                            .checked_sub(1)
                            .unwrap_or(Resource::list().len() - 1);
                    }
                    KeyCode::Right => {
                        selected_resource = (selected_resource + 1) % Resource::list().len();
                    }
                    KeyCode::Up => {
                        adjust_drop_selection(&private.resources, &mut selected, resource, 1);
                    }
                    KeyCode::Down => {
                        adjust_drop_selection(&private.resources, &mut selected, resource, -1);
                    }
                    _ => {}
                }
            }
        }
    }

    fn select_bank_trade(&mut self, model: &UiModel) -> io::Result<Option<BankTrade>> {
        let options = bank_trade_options(model);
        if options.is_empty() {
            self.message = "no available bank trades".to_owned();
            return Ok(None);
        }

        let mut selected = 0;
        self.message = "select bank trade with up/down; enter confirms; esc cancels".to_owned();
        loop {
            self.personal_override = Some(bank_trade_menu_lines(&options, selected));
            self.draw(
                Some(model),
                "bank-trade: ",
                &bank_trade_label(options[selected]),
            )?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        self.personal_override = None;
                        return Ok(Some(options[selected]));
                    }
                    KeyCode::Esc => {
                        self.personal_override = None;
                        self.message = "bank trade cancelled".to_owned();
                        return Ok(None);
                    }
                    KeyCode::Up => selected = selected.checked_sub(1).unwrap_or(options.len() - 1),
                    KeyCode::Down => selected = (selected + 1) % options.len(),
                    _ => {}
                }
            }
        }
    }

    fn select_resource(
        &mut self,
        model: &UiModel,
        prompt: &str,
        message: &str,
    ) -> io::Result<Option<Resource>> {
        let mut selected = 0;
        self.message = message.to_owned();
        loop {
            self.personal_override = Some(resource_picker_lines(selected));
            let resource = Resource::list()[selected];
            self.draw(Some(model), prompt, &format!("{resource:?}"))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        self.personal_override = None;
                        return Ok(Some(resource));
                    }
                    KeyCode::Esc => {
                        self.personal_override = None;
                        self.message = "resource selection cancelled".to_owned();
                        return Ok(None);
                    }
                    KeyCode::Left => {
                        selected = selected
                            .checked_sub(1)
                            .unwrap_or(Resource::list().len() - 1);
                    }
                    KeyCode::Right => {
                        selected = (selected + 1) % Resource::list().len();
                    }
                    _ => {}
                }
            }
        }
    }

    fn select_player(
        &mut self,
        model: &UiModel,
        candidates: &[PlayerId],
        prompt: &str,
    ) -> io::Result<Option<PlayerId>> {
        if candidates.is_empty() {
            return Ok(None);
        }
        if candidates.len() == 1 {
            return Ok(Some(candidates[0]));
        }

        let mut selected = 0;
        self.message = "select player with up/down; enter confirms; esc cancels".to_owned();
        loop {
            self.personal_override = Some(player_menu_lines(candidates, selected));
            self.draw(Some(model), prompt, &format!("p{}", candidates[selected]))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        self.personal_override = None;
                        return Ok(Some(candidates[selected]));
                    }
                    KeyCode::Esc => {
                        self.personal_override = None;
                        self.message = "player selection cancelled".to_owned();
                        return Ok(None);
                    }
                    KeyCode::Up => {
                        selected = selected.checked_sub(1).unwrap_or(candidates.len() - 1)
                    }
                    KeyCode::Down => selected = (selected + 1) % candidates.len(),
                    _ => {}
                }
            }
        }
    }

    fn draw(&mut self, model: Option<&UiModel>, prompt: &str, input: &str) -> io::Result<()> {
        let message = self.message.clone();
        let overlay = self.overlay.clone();
        let public_override = self.public_override.clone();
        let personal_override = self.personal_override.clone();
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
                        .constraints([Constraint::Min(68), Constraint::Length(42)])
                        .split(chunks[1]);

                    let field = Paragraph::new(field_lines(model, &overlay))
                        .block(Block::default().borders(Borders::ALL).title("Field"));
                    frame.render_widget(field, body_chunks[0]);

                    let info_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Min(6), Constraint::Length(15)])
                        .split(body_chunks[1]);

                    let has_public_override = public_override.is_some();
                    let public_lines = public_override.unwrap_or_else(|| public_model_lines(model));
                    let public_title = if has_public_override {
                        "Game Ended"
                    } else {
                        "Public"
                    };
                    let public = Paragraph::new(public_lines)
                        .wrap(Wrap { trim: false })
                        .block(Block::default().borders(Borders::ALL).title(public_title));
                    frame.render_widget(public, info_chunks[0]);

                    let personal = Paragraph::new(
                        personal_override.unwrap_or_else(|| personal_model_lines(model)),
                    )
                    .wrap(Wrap { trim: false })
                    .block(Block::default().borders(Borders::ALL).title("Personal"));
                    frame.render_widget(personal, info_chunks[1]);
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

fn public_model_lines(model: &UiModel) -> Vec<Line<'static>> {
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
        "board: radius {} | tiles {}",
        model.public.board.field_radius,
        model.public.board.tiles.len()
    )));
    lines.push(bank_resources_line(model));
    lines.push(Line::from(""));
    lines.push(Line::from("players:"));
    for player in &model.public.players {
        lines.push(public_player_line(player));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("commands: roll | end | buy dev | build road [h1] [h2] | build settlement [h1] [h2] [h3] | build city [h1] [h2] [h3]"));
    lines.push(Line::from(
        "trades: bank-trade/bt for menu | bank-trade [give] [take] [G4 | G3 | S2]",
    ));
    lines.push(Line::from(
        "dev cards: kn | yp | m | rb, or typed: use knight/yop/monopoly/roadbuild ...",
    ));
    lines
}

fn game_ended_lines(
    model: &UiModel,
    winner_id: PlayerId,
    turn_no: u64,
    stats: &[GameEndPlayerStats],
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Game ended",
            Style::default().fg(Color::Green),
        )),
        Line::from(format!("winner: p{winner_id} | turns: {turn_no}")),
        Line::from(format!(
            "robber {} | LR {:?} | LA {:?}",
            model.public.board_state.robber_pos.index().to_spiral(),
            model.public.longest_road_owner,
            model.public.largest_army_owner
        )),
        Line::from(""),
        Line::from("final stats"),
        Line::from("┌───┬──┬──┬──┬──┬──┬──┬──┬──┬─────────┐"),
        Line::from("│P  │VP│B │A │S │C │R │L │K │Tags     │"),
        Line::from("├───┼──┼──┼──┼──┼──┼──┼──┼──┼─────────┤"),
    ];

    let mut sorted = stats.to_vec();
    sorted.sort_by_key(|stats| (std::cmp::Reverse(stats.total_vp), stats.player_id));
    for stats in sorted {
        let mut tags = Vec::new();
        if stats.player_id == winner_id {
            tags.push("WIN");
        }
        if stats.has_longest_road {
            tags.push("LR");
        }
        if stats.has_largest_army {
            tags.push("LA");
        }
        lines.push(Line::from(format!(
            "│p{:<2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:>2}│{:<9}│",
            stats.player_id,
            stats.total_vp,
            stats.build_and_dev_card_vp,
            stats.award_vp,
            stats.settlements,
            stats.cities,
            stats.roads,
            stats.longest_road_length,
            stats.knights_used,
            tags.join(" "),
        )));
    }

    lines.extend([
        Line::from("└───┴──┴──┴──┴──┴──┴──┴──┴──┴─────────┘"),
        Line::from(""),
        Line::from("B=base VP  A=award VP"),
        Line::from("S=set C=city R=road"),
        Line::from("L=longest K=knights"),
        Line::from(Span::styled(
            "[press esc to quit]",
            Style::default().fg(Color::Yellow),
        )),
    ]);
    lines
}

fn personal_model_lines(model: &UiModel) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(private) = &model.private {
        lines.push(Line::from(format!("you: p{}", private.player_id)));
        lines.push(Line::from("resources"));
        lines.extend(resource_card_lines(&private.resources, None));
        lines.push(Line::from(""));
        lines.push(Line::from("development"));
        lines.extend(dev_card_lines(&private.dev_cards));
    } else {
        lines.push(Line::from("no private player data"));
    }
    lines
}

fn bank_resources_line(model: &UiModel) -> Line<'static> {
    let mut spans = vec![Span::raw("bank: resources ")];
    match &model.public.bank.resources {
        UiPublicBankResources::Exact(resources) => {
            push_resource_values(&mut spans, resources, |count| count.to_string());
        }
        UiPublicBankResources::Approx(resources) => {
            for (idx, resource) in Resource::list().into_iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::raw(" "));
                }
                push_resource_value(&mut spans, resource, fullness_symbol(resources[resource]));
            }
        }
    }
    spans.push(Span::raw(" | dev "));
    let dev = match &model.public.bank.resources {
        UiPublicBankResources::Exact(_) => model.public.bank.dev_card_count.to_string(),
        UiPublicBankResources::Approx(_) => {
            fullness_symbol(deck_fullness(model.public.bank.dev_card_count)).to_owned()
        }
    };
    spans.push(Span::styled(dev, Style::default().fg(Color::Magenta)));
    Line::from(spans)
}

fn public_player_line(player: &catan_agents::remote_agent::UiPublicPlayer) -> Line<'static> {
    let style = player_style(player.player_id);
    let mut spans = vec![
        Span::styled("  ", style),
        Span::styled(format!("p{}", player.player_id), style),
        Span::styled(" resources ", style),
    ];
    match &player.resources {
        UiPublicPlayerResources::Exact(resources) => {
            push_resource_values(&mut spans, resources, |count| count.to_string());
        }
        UiPublicPlayerResources::Total(total) => {
            spans.push(Span::styled(total.to_string(), style));
        }
    }
    spans.push(Span::styled(
        format!(
            " active_dev={} queued_dev={} vp={:?}",
            player.active_dev_cards, player.queued_dev_cards, player.victory_points
        ),
        style,
    ));
    Line::from(spans)
}

fn resource_card_lines(
    resources: &ResourceCollection,
    selected_drop: Option<&ResourceCollection>,
) -> Vec<Line<'static>> {
    let mut top = Vec::new();
    let mut middle = Vec::new();
    let mut bottom = Vec::new();
    let mut selected = Vec::new();

    for (idx, resource) in Resource::list().into_iter().enumerate() {
        if idx > 0 {
            for spans in [&mut top, &mut middle, &mut bottom, &mut selected] {
                spans.push(Span::raw(" "));
            }
        }
        let style = resource_style(resource);
        top.push(Span::styled("┌──┐", style));
        middle.push(Span::styled(
            format!("│{:02}│", resources[resource].min(99)),
            style,
        ));
        bottom.push(Span::styled("└──┘", style));
        if let Some(drop) = selected_drop {
            selected.push(Span::styled(
                format!(" {:02} ", drop[resource].min(99)),
                style,
            ));
        }
    }

    let mut lines = vec![Line::from(top), Line::from(middle), Line::from(bottom)];
    if selected_drop.is_some() {
        lines.push(Line::from(selected));
    }
    lines
}

fn dev_card_lines(dev_cards: &DevCardData) -> Vec<Line<'static>> {
    let mut top = Vec::new();
    let mut middle = Vec::new();
    let mut bottom = Vec::new();
    let mut labels = Vec::new();

    for (idx, card) in [
        UsableDevCard::Knight,
        UsableDevCard::YearOfPlenty,
        UsableDevCard::Monopoly,
        UsableDevCard::RoadBuild,
    ]
    .into_iter()
    .enumerate()
    {
        if idx > 0 {
            push_dev_card_gap(&mut top, &mut middle, &mut bottom, &mut labels);
        }
        push_dev_card(
            &mut top,
            &mut middle,
            &mut bottom,
            &mut labels,
            dev_card_abbrev(card),
            [
                Some(dev_cards.used[card]),
                Some(dev_cards.active[card]),
                Some(dev_cards.queued[card]),
            ],
        );
    }

    push_dev_card_gap(&mut top, &mut middle, &mut bottom, &mut labels);
    push_dev_card(
        &mut top,
        &mut middle,
        &mut bottom,
        &mut labels,
        "VP",
        [None, Some(dev_cards.victory_pts), None],
    );

    vec![
        Line::from(top),
        Line::from(middle),
        Line::from(bottom),
        Line::from(labels),
    ]
}

fn drop_personal_lines(
    player_id: PlayerId,
    resources: &ResourceCollection,
    dev_cards: &DevCardData,
    selected: &ResourceCollection,
    required: u16,
    selected_resource: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("you: p{player_id}")),
        Line::from(format!("drop {} / {} cards", selected.total(), required)),
    ];
    lines.extend(drop_resource_card_lines(
        resources,
        selected,
        selected_resource,
    ));
    lines.push(Line::from(""));
    lines.extend(dev_card_lines(dev_cards));
    lines
}

fn drop_resource_card_lines(
    resources: &ResourceCollection,
    selected: &ResourceCollection,
    selected_resource: usize,
) -> Vec<Line<'static>> {
    let mut lines = resource_card_lines(resources, None);
    let mut selector = Vec::new();
    let mut selected_counts = Vec::new();
    for (idx, resource) in Resource::list().into_iter().enumerate() {
        if idx > 0 {
            selector.push(Span::raw(" "));
            selected_counts.push(Span::raw(" "));
        }
        let style = resource_style(resource);
        let marker = if idx == selected_resource {
            "^^^^"
        } else {
            "    "
        };
        selector.push(Span::styled(marker, style));
        selected_counts.push(Span::styled(
            format!("{:^4}", selected[resource].min(99)),
            style,
        ));
    }
    lines.push(Line::from(selector));
    lines.push(Line::from(selected_counts));
    lines
}

fn bank_trade_menu_lines(options: &[BankTrade], selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("bank trade"),
        Line::from("up/down select; enter confirms; esc cancels"),
    ];
    let visible_rows = 11;
    let half_window = visible_rows / 2;
    let start = selected.saturating_sub(half_window);
    let end = (start + visible_rows).min(options.len());

    for (idx, trade) in options.iter().enumerate().skip(start).take(end - start) {
        let marker = if idx == selected { "> " } else { "  " };
        lines.push(bank_trade_menu_line(marker, *trade));
    }
    lines
}

fn bank_trade_menu_line(marker: &str, trade: BankTrade) -> Line<'static> {
    let rate = match trade.kind {
        BankTradeKind::BankGeneric => "4:1",
        BankTradeKind::PortGeneric => "3:1",
        BankTradeKind::PortSpecific => "2:1",
    };
    Line::from(vec![
        Span::raw(marker.to_owned()),
        Span::raw(format!("{rate} ")),
        Span::styled(format!("{:?}", trade.give), resource_style(trade.give)),
        Span::raw(" -> "),
        Span::styled(format!("{:?}", trade.take), resource_style(trade.take)),
    ])
}

fn resource_picker_lines(selected_resource: usize) -> Vec<Line<'static>> {
    let resources = ResourceCollection {
        brick: 1,
        wood: 1,
        wheat: 1,
        sheep: 1,
        ore: 1,
    };
    let mut lines = vec![Line::from("left/right select; enter confirms; esc cancels")];
    lines.extend(resource_card_lines(&resources, None));
    lines.push(resource_selector_line(selected_resource));
    lines
}

fn resource_selector_line(selected_resource: usize) -> Line<'static> {
    let mut selector = Vec::new();
    for (idx, resource) in Resource::list().into_iter().enumerate() {
        if idx > 0 {
            selector.push(Span::raw(" "));
        }
        let marker = if idx == selected_resource {
            "^^^^"
        } else {
            "    "
        };
        selector.push(Span::styled(marker, resource_style(resource)));
    }
    Line::from(selector)
}

fn player_menu_lines(candidates: &[PlayerId], selected: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("rob player"),
        Line::from("up/down select; enter confirms; esc cancels"),
    ];
    for (idx, player_id) in candidates.iter().enumerate() {
        let marker = if idx == selected { "> " } else { "  " };
        lines.push(Line::from(format!("{marker}p{player_id}")));
    }
    lines
}

fn adjust_drop_selection(
    available: &ResourceCollection,
    selected: &mut ResourceCollection,
    resource: Resource,
    delta: i8,
) {
    if delta > 0 {
        selected[resource] = (selected[resource] + delta as u16).min(available[resource]);
    } else {
        selected[resource] = selected[resource].saturating_sub(delta.unsigned_abs() as u16);
    }
}

fn push_dev_card_gap(
    top: &mut Vec<Span<'static>>,
    middle: &mut Vec<Span<'static>>,
    bottom: &mut Vec<Span<'static>>,
    labels: &mut Vec<Span<'static>>,
) {
    for spans in [top, middle, bottom, labels] {
        spans.push(Span::raw(" "));
    }
}

fn push_dev_card(
    top: &mut Vec<Span<'static>>,
    middle: &mut Vec<Span<'static>>,
    bottom: &mut Vec<Span<'static>>,
    labels: &mut Vec<Span<'static>>,
    label: &'static str,
    counts: [Option<u16>; 3],
) {
    let style = Style::default().fg(Color::Magenta);
    top.push(Span::styled("┌──┐", style));
    top.push(Span::raw(format!("{:>2}", count_label(counts[0]))));
    middle.push(Span::styled(format!("│{:^2}│", label), style));
    middle.push(Span::raw(format!("{:>2}", count_label(counts[1]))));
    bottom.push(Span::styled("└──┘", style));
    bottom.push(Span::raw(format!("{:>2}", count_label(counts[2]))));
    labels.push(Span::raw(format!("{:^6}", label)));
}

fn count_label(count: Option<u16>) -> String {
    count
        .map(|count| count.min(99).to_string())
        .unwrap_or_else(|| " ".to_owned())
}

fn dev_card_abbrev(card: UsableDevCard) -> &'static str {
    match card {
        UsableDevCard::Knight => "KN",
        UsableDevCard::YearOfPlenty => "YP",
        UsableDevCard::Monopoly => "M",
        UsableDevCard::RoadBuild => "RB",
    }
}

fn push_resource_values(
    spans: &mut Vec<Span<'static>>,
    resources: &ResourceCollection,
    format_count: impl Fn(u16) -> String,
) {
    for (idx, resource) in Resource::list().into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        push_resource_value(spans, resource, format_count(resources[resource]));
    }
}

fn push_resource_value(
    spans: &mut Vec<Span<'static>>,
    resource: Resource,
    value: impl Into<String>,
) {
    let style = resource_style(resource);
    let name: &'static str = resource.into();
    spans.push(Span::styled(name, style));
    spans.push(Span::styled(":", style));
    spans.push(Span::styled(value.into(), style));
}

fn fullness_symbol(level: DeckFullnessLevel) -> &'static str {
    match level {
        DeckFullnessLevel::High => "???",
        DeckFullnessLevel::Medium => "??",
        DeckFullnessLevel::Low => "?",
        DeckFullnessLevel::Empty => "0",
    }
}

fn deck_fullness(count: u16) -> DeckFullnessLevel {
    DeckFullnessLevel::new(count).unwrap_or(DeckFullnessLevel::High)
}

fn resource_style(resource: Resource) -> Style {
    Style::default().fg(ratatui_color(FieldRenderer::resource_color(resource)))
}

fn player_style(player_id: PlayerId) -> Style {
    Style::default().fg(ratatui_color(FieldRenderer::player_color(player_id)))
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
        if line.is_empty() || matches!(line, "roll" | "r") {
            log::trace!("Init action: RollDice");
            return Ok(InitAction::RollDice);
        }
        if let Some(usage) = handle_interactive_dev_card_action(ui, model, line)? {
            log::trace!("Init interactive action: UseDevCard({:?})", usage);
            return Ok(InitAction::UseDevCard(usage));
        }
        if partial_dev_card_command(line).is_some() {
            continue;
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
        if let Some(usage) = handle_interactive_dev_card_action(ui, model, &line)? {
            log::trace!("Post-dice interactive action: UseDevCard({:?})", usage);
            return Ok(PostDiceAction::UseDevCard(usage));
        }
        if partial_dev_card_command(&line).is_some() {
            continue;
        }
        if let Some(action) = handle_interactive_regular_action(ui, model, &line)? {
            log::trace!("Post-dice interactive action: RegularAction({:?})", action);
            return Ok(PostDiceAction::RegularAction(action));
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
        if let Some(action) = handle_interactive_regular_action(ui, model, &line)? {
            log::trace!("Regular interactive action: {:?}", action);
            return Ok(action);
        }
        if partial_regular_command(&line) {
            continue;
        }
        if let Some(action) = parse_regular_action(&line) {
            log::trace!("Regular action: {:?}", action);
            return Ok(action);
        }
        log::warn!("Could not parse regular action: {}", line);
        ui.set_message("could not parse action".to_owned())?;
    }
}

fn handle_interactive_regular_action(
    ui: &mut CliUi,
    model: &UiModel,
    line: &str,
) -> io::Result<Option<RegularAction>> {
    if let Some(kind) = partial_build_command(line) {
        let builds = legal_builds_for_mode(model, kind);
        return Ok(ui
            .select_build(model, builds, "build: ")?
            .map(RegularAction::Build));
    }
    if matches!(line, "bank-trade" | "bt") {
        return Ok(ui
            .select_bank_trade(model)?
            .map(RegularAction::TradeWithBank));
    }
    Ok(None)
}

fn partial_regular_command(line: &str) -> bool {
    partial_build_command(line).is_some() || matches!(line, "bank-trade" | "bt")
}

fn handle_interactive_dev_card_action(
    ui: &mut CliUi,
    model: &UiModel,
    line: &str,
) -> io::Result<Option<DevCardUsage>> {
    match partial_dev_card_command(line) {
        Some(PartialDevCardMode::Knight) => select_knight_usage(ui, model),
        Some(PartialDevCardMode::RoadBuild) => select_roadbuild_usage(ui, model),
        Some(PartialDevCardMode::Monopoly) => Ok(ui
            .select_resource(
                model,
                "monopoly: ",
                "select monopoly resource with left/right",
            )?
            .map(DevCardUsage::Monopoly)),
        Some(PartialDevCardMode::YearOfPlenty) => {
            let Some(first) = ui.select_resource(
                model,
                "year-of-plenty 1: ",
                "select first year-of-plenty resource",
            )?
            else {
                return Ok(None);
            };
            let Some(second) = ui.select_resource(
                model,
                "year-of-plenty 2: ",
                "select second year-of-plenty resource",
            )?
            else {
                return Ok(None);
            };
            Ok(Some(DevCardUsage::YearOfPlenty([first, second])))
        }
        None => Ok(None),
    }
}

fn select_knight_usage(ui: &mut CliUi, model: &UiModel) -> io::Result<Option<DevCardUsage>> {
    let rob_hex = ui.select_hex_where(model, "knight hex: ", |hex| {
        hex != model.public.board_state.robber_pos
    })?;
    let candidates = robbable_players_on_hex(model, rob_hex);
    let robbed_id = ui.select_player(model, &candidates, "robbed player: ")?;
    if robbed_id.is_none() && !candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(DevCardUsage::Knight { rob_hex, robbed_id }))
}

fn select_roadbuild_usage(ui: &mut CliUi, model: &UiModel) -> io::Result<Option<DevCardUsage>> {
    let first_options = legal_roadbuild_roads(model);
    let Some(Build::Road(first)) = ui.select_build(model, first_options, "roadbuild 1: ")? else {
        return Ok(None);
    };

    let mut second_model = model.clone();
    add_road_to_model(&mut second_model, model.actor.unwrap_or_default(), first);
    let second_options = legal_roadbuild_roads(&second_model);
    let Some(Build::Road(second)) =
        ui.select_build(&second_model, second_options, "roadbuild 2: ")?
    else {
        return Ok(None);
    };

    Ok(Some(DevCardUsage::RoadBuild([first.pos, second.pos])))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialDevCardMode {
    Knight,
    RoadBuild,
    Monopoly,
    YearOfPlenty,
}

fn partial_dev_card_command(line: &str) -> Option<PartialDevCardMode> {
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["use", "knight"] | ["kn"] => Some(PartialDevCardMode::Knight),
        ["use", "roadbuild"] | ["use", "road-build"] | ["rb"] => {
            Some(PartialDevCardMode::RoadBuild)
        }
        ["use", "monopoly"] | ["m"] => Some(PartialDevCardMode::Monopoly),
        ["use", "yop"] | ["use", "year-of-plenty"] | ["yp"] => {
            Some(PartialDevCardMode::YearOfPlenty)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialBuildMode {
    Settlement,
    Road,
    City,
}

fn partial_build_command(line: &str) -> Option<PartialBuildMode> {
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["build", "settlement"] | ["bs"] => Some(PartialBuildMode::Settlement),
        ["build", "road"] | ["br"] => Some(PartialBuildMode::Road),
        ["build", "city"] | ["bc"] => Some(PartialBuildMode::City),
        _ => None,
    }
}

fn parse_regular_action(line: &str) -> Option<RegularAction> {
    let line = line.trim();
    if matches!(line, "end" | "e") || line.is_empty() {
        return Some(RegularAction::EndMove);
    }
    if matches!(line, "buy dev" | "buy-dev" | "bd") {
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
        if line == "drop" {
            if let Some(resources) = ui.select_drop_cards(model)? {
                return Ok(resources);
            }
            continue;
        }
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

fn read_robbed_player(ui: &mut CliUi, model: &UiModel, prompt: &str) -> io::Result<PlayerId> {
    if let Some(hex) = ui.pending_robber_hex {
        let candidates = robbable_players_on_hex(model, hex);
        if let Some(player_id) = ui.select_player(model, &candidates, prompt)? {
            return Ok(player_id);
        }
        ui.message = "rob target selection cancelled".to_owned();
    }

    read_player_id(ui, model, prompt)
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

fn legal_builds_for_mode(model: &UiModel, mode: PartialBuildMode) -> Vec<Build> {
    match mode {
        PartialBuildMode::Settlement => legal_regular_settlements(model),
        PartialBuildMode::Road => legal_regular_roads(model),
        PartialBuildMode::City => legal_regular_cities(model),
    }
}

fn legal_regular_settlements(model: &UiModel) -> Vec<Build> {
    if !can_afford(
        model,
        ResourceCollection {
            brick: 1,
            wood: 1,
            wheat: 1,
            sheep: 1,
            ore: 0,
        },
    ) || player_settlement_count(model, model.actor.unwrap_or_default()) >= 5
    {
        return Vec::new();
    }

    let actor = model.actor.unwrap_or_default();
    let deadzone = settlement_deadzone(model);
    board_intersection_set(model)
        .into_iter()
        .filter(|intersection| !deadzone.contains(intersection))
        .filter(|intersection| {
            player_builds(model, actor)
                .map(|builds| {
                    builds.roads.iter().any(|road| {
                        road.pos
                            .intersections_iter()
                            .any(|pos| pos == *intersection)
                    })
                })
                .unwrap_or(false)
        })
        .map(|pos| {
            Build::Establishment(Establishment {
                pos,
                stage: EstablishmentType::Settlement,
            })
        })
        .collect()
}

fn legal_regular_roads(model: &UiModel) -> Vec<Build> {
    if !can_afford(
        model,
        ResourceCollection {
            brick: 1,
            wood: 1,
            ..ResourceCollection::ZERO
        },
    ) || player_road_count(model, model.actor.unwrap_or_default()) >= 15
    {
        return Vec::new();
    }

    legal_roadbuild_roads(model)
}

fn legal_roadbuild_roads(model: &UiModel) -> Vec<Build> {
    if player_road_count(model, model.actor.unwrap_or_default()) >= 15 {
        return Vec::new();
    }

    let actor = model.actor.unwrap_or_default();
    let occupied = occupied_roads(model);
    let opponent_intersections = opponent_occupied_intersections(model, actor);
    board_path_set(model)
        .into_iter()
        .filter(|path| !occupied.contains(path))
        .filter(|path| road_connects_to_player(model, actor, *path, &opponent_intersections))
        .map(|pos| Build::Road(Road { pos }))
        .collect()
}

fn legal_regular_cities(model: &UiModel) -> Vec<Build> {
    if !can_afford(
        model,
        ResourceCollection {
            wheat: 2,
            ore: 3,
            ..ResourceCollection::ZERO
        },
    ) || player_city_count(model, model.actor.unwrap_or_default()) >= 4
    {
        return Vec::new();
    }

    let actor = model.actor.unwrap_or_default();
    player_builds(model, actor)
        .into_iter()
        .flat_map(|builds| builds.establishments.iter())
        .filter(|est| est.stage == EstablishmentType::Settlement)
        .map(|est| {
            Build::Establishment(Establishment {
                pos: est.pos,
                stage: EstablishmentType::City,
            })
        })
        .collect()
}

fn can_afford(model: &UiModel, cost: ResourceCollection) -> bool {
    model
        .private
        .as_ref()
        .map(|private| private.resources.has_enough(&cost))
        .unwrap_or(false)
}

fn player_builds(
    model: &UiModel,
    player_id: PlayerId,
) -> Option<&catan_agents::remote_agent::UiPlayerBuilds> {
    model
        .public
        .builds
        .iter()
        .find(|builds| builds.player_id == player_id)
}

fn player_settlement_count(model: &UiModel, player_id: PlayerId) -> usize {
    player_builds(model, player_id)
        .map(|builds| {
            builds
                .establishments
                .iter()
                .filter(|est| est.stage == EstablishmentType::Settlement)
                .count()
        })
        .unwrap_or_default()
}

fn player_city_count(model: &UiModel, player_id: PlayerId) -> usize {
    player_builds(model, player_id)
        .map(|builds| {
            builds
                .establishments
                .iter()
                .filter(|est| est.stage == EstablishmentType::City)
                .count()
        })
        .unwrap_or_default()
}

fn player_road_count(model: &UiModel, player_id: PlayerId) -> usize {
    player_builds(model, player_id)
        .map(|builds| builds.roads.len())
        .unwrap_or_default()
}

fn opponent_occupied_intersections(model: &UiModel, actor: PlayerId) -> BTreeSet<Intersection> {
    model
        .public
        .builds
        .iter()
        .filter(|builds| builds.player_id != actor)
        .flat_map(|builds| builds.establishments.iter().map(|est| est.pos))
        .collect()
}

fn road_connects_to_player(
    model: &UiModel,
    actor: PlayerId,
    path: BoardPath,
    opponent_intersections: &BTreeSet<Intersection>,
) -> bool {
    let Some(builds) = player_builds(model, actor) else {
        return false;
    };
    path.intersections_iter().any(|intersection| {
        builds
            .establishments
            .iter()
            .any(|est| est.pos == intersection)
            || (!opponent_intersections.contains(&intersection)
                && builds.roads.iter().any(|road| {
                    road.pos
                        .intersections_iter()
                        .any(|road_intersection| road_intersection == intersection)
                }))
    })
}

fn build_label(build: Build) -> String {
    match build {
        Build::Road(road) => path_label(road.pos),
        Build::Establishment(establishment) => match establishment.stage {
            EstablishmentType::Settlement => {
                format!("settlement {}", intersection_label(establishment.pos))
            }
            EstablishmentType::City => format!("city {}", intersection_label(establishment.pos)),
        },
    }
}

fn bank_trade_options(model: &UiModel) -> Vec<BankTrade> {
    let Some(private) = &model.private else {
        return Vec::new();
    };
    let ports = acquired_ports(model, private.player_id);
    let has_universal_port = ports.contains(&PortKind::Universal);
    let mut trades = Vec::new();

    for give in Resource::list() {
        for take in Resource::list().into_iter().filter(|take| *take != give) {
            if private.resources[give] >= 4 {
                trades.push(BankTrade {
                    give,
                    take,
                    kind: BankTradeKind::BankGeneric,
                });
            }
            if has_universal_port && private.resources[give] >= 3 {
                trades.push(BankTrade {
                    give,
                    take,
                    kind: BankTradeKind::PortGeneric,
                });
            }
            if ports.contains(&PortKind::Special(give)) && private.resources[give] >= 2 {
                trades.push(BankTrade {
                    give,
                    take,
                    kind: BankTradeKind::PortSpecific,
                });
            }
        }
    }

    trades
}

fn acquired_ports(model: &UiModel, player_id: PlayerId) -> BTreeSet<PortKind> {
    let establishments = player_builds(model, player_id)
        .map(|builds| {
            builds
                .establishments
                .iter()
                .map(|est| est.pos)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    model
        .public
        .board
        .ports
        .iter()
        .filter_map(|(port_pos, port)| {
            port_pos
                .intersections()
                .into_iter()
                .any(|intersection| establishments.contains(&intersection))
                .then_some(*port)
        })
        .collect()
}

fn bank_trade_label(trade: BankTrade) -> String {
    let rate = match trade.kind {
        BankTradeKind::BankGeneric => "4:1",
        BankTradeKind::PortGeneric => "3:1",
        BankTradeKind::PortSpecific => "2:1",
    };
    format!("{rate} {:?} -> {:?}", trade.give, trade.take)
}

fn robbable_players_on_hex(model: &UiModel, hex: Hex) -> Vec<PlayerId> {
    let actor = model.actor.unwrap_or_default();
    model
        .public
        .builds
        .iter()
        .filter(|builds| builds.player_id != actor)
        .filter(|builds| {
            builds
                .establishments
                .iter()
                .any(|est| est.pos.as_set().contains(&hex))
        })
        .filter(|builds| public_player_resource_total(model, builds.player_id) > 0)
        .map(|builds| builds.player_id)
        .collect()
}

fn public_player_resource_total(model: &UiModel, player_id: PlayerId) -> u16 {
    if model
        .private
        .as_ref()
        .map(|private| private.player_id == player_id)
        .unwrap_or(false)
    {
        return model
            .private
            .as_ref()
            .map(|private| private.resources.total())
            .unwrap_or_default();
    }

    model
        .public
        .players
        .iter()
        .find(|player| player.player_id == player_id)
        .map(|player| match &player.resources {
            UiPublicPlayerResources::Exact(resources) => resources.total(),
            UiPublicPlayerResources::Total(total) => *total,
        })
        .unwrap_or_default()
}

fn add_road_to_model(model: &mut UiModel, player_id: PlayerId, road: Road) {
    if let Some(builds) = model
        .public
        .builds
        .iter_mut()
        .find(|builds| builds.player_id == player_id)
    {
        builds.roads.push(road);
    }
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
    fn public_and_personal_lines_are_separate() {
        let init = GameInitializationState::default();
        let model = model_from_state(&init.finish());
        let public_lines = public_model_lines(&model);
        let personal_lines = personal_model_lines(&model);

        assert!(
            public_lines
                .iter()
                .any(|line| line.to_string().contains("players:"))
        );
        assert!(
            !public_lines
                .iter()
                .any(|line| line.to_string().contains("your dev cards"))
        );
        assert!(
            personal_lines
                .iter()
                .any(|line| line.to_string().contains("development"))
        );
    }

    #[test]
    fn card_lines_render_resource_and_dev_counts() {
        let resources = ResourceCollection {
            brick: 1,
            wood: 2,
            wheat: 13,
            sheep: 0,
            ore: 5,
        };
        let rendered_resources = resource_card_lines(&resources, None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(rendered_resources[0].contains("┌──┐"));
        assert!(rendered_resources[1].contains("│01│"));
        assert!(rendered_resources[1].contains("│13│"));

        let dev_cards = DevCardData::default();
        let rendered_dev = dev_card_lines(&dev_cards)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(rendered_dev[1].contains("│KN│"));
        assert!(rendered_dev[1].contains("│YP│"));
        assert!(rendered_dev[1].contains("│ M│") || rendered_dev[1].contains("│M │"));
        assert!(rendered_dev[1].contains("│VP│"));
    }

    #[test]
    fn partial_build_commands_parse_aliases() {
        assert_eq!(
            partial_build_command("build settlement"),
            Some(PartialBuildMode::Settlement)
        );
        assert_eq!(
            partial_build_command("bs"),
            Some(PartialBuildMode::Settlement)
        );
        assert_eq!(
            partial_build_command("build road"),
            Some(PartialBuildMode::Road)
        );
        assert_eq!(partial_build_command("br"), Some(PartialBuildMode::Road));
        assert_eq!(
            partial_build_command("build city"),
            Some(PartialBuildMode::City)
        );
        assert_eq!(partial_build_command("bc"), Some(PartialBuildMode::City));
        assert_eq!(partial_build_command("build road 0 1"), None);
    }

    #[test]
    fn interactive_shortcuts_parse() {
        assert_eq!(partial_regular_command("bt"), true);
        assert!(matches!(
            parse_regular_action("bd"),
            Some(RegularAction::BuyDevCard)
        ));
        assert!(matches!(
            parse_regular_action("e"),
            Some(RegularAction::EndMove)
        ));
        assert_eq!(
            partial_dev_card_command("kn"),
            Some(PartialDevCardMode::Knight)
        );
        assert_eq!(
            partial_dev_card_command("use knight"),
            Some(PartialDevCardMode::Knight)
        );
        assert_eq!(
            partial_dev_card_command("rb"),
            Some(PartialDevCardMode::RoadBuild)
        );
        assert_eq!(
            partial_dev_card_command("use roadbuild"),
            Some(PartialDevCardMode::RoadBuild)
        );
        assert_eq!(
            partial_dev_card_command("m"),
            Some(PartialDevCardMode::Monopoly)
        );
        assert_eq!(
            partial_dev_card_command("yp"),
            Some(PartialDevCardMode::YearOfPlenty)
        );
    }

    #[test]
    fn drop_selection_is_bounded_by_available_resources() {
        let available = ResourceCollection {
            brick: 2,
            ..ResourceCollection::ZERO
        };
        let mut selected = ResourceCollection::ZERO;

        adjust_drop_selection(&available, &mut selected, Resource::Brick, 1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, 1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, 1);
        assert_eq!(selected.brick, 2);

        adjust_drop_selection(&available, &mut selected, Resource::Brick, -1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, -1);
        adjust_drop_selection(&available, &mut selected, Resource::Brick, -1);
        assert_eq!(selected.brick, 0);
    }

    #[test]
    fn drop_lines_show_selector_counts_and_total() {
        let resources = ResourceCollection {
            brick: 2,
            wood: 1,
            ..ResourceCollection::ZERO
        };
        let selected = ResourceCollection {
            brick: 1,
            ..ResourceCollection::ZERO
        };
        let dev_cards = DevCardData::default();
        let lines = drop_personal_lines(0, &resources, &dev_cards, &selected, 2, 0)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("drop 1 / 2 cards")));
        assert!(lines.iter().any(|line| line.contains("^^^^")));
        assert!(lines.iter().any(|line| line.contains(" 1  ")));
        assert!(lines.iter().any(|line| line.contains(" 0  ")));
    }

    #[test]
    fn bank_trade_menu_shows_instructions_and_selected_trade() {
        let options = vec![
            BankTrade {
                give: Resource::Brick,
                take: Resource::Wood,
                kind: BankTradeKind::BankGeneric,
            },
            BankTrade {
                give: Resource::Wheat,
                take: Resource::Ore,
                kind: BankTradeKind::PortGeneric,
            },
        ];
        let lines = bank_trade_menu_lines(&options, 1)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("up/down select")));
        assert!(lines.iter().any(|line| line.contains("> 3:1 Wheat -> Ore")));
    }

    #[test]
    fn game_ended_lines_include_winner_stats_and_quit_hint() {
        let init = GameInitializationState::default();
        let model = model_from_state(&init.finish());
        let stats = vec![
            GameEndPlayerStats {
                player_id: 0,
                total_vp: 10,
                build_and_dev_card_vp: 8,
                award_vp: 2,
                settlements: 4,
                cities: 2,
                roads: 7,
                longest_road_length: 5,
                knights_used: 1,
                has_longest_road: true,
                has_largest_army: false,
            },
            GameEndPlayerStats {
                player_id: 1,
                total_vp: 6,
                build_and_dev_card_vp: 6,
                award_vp: 0,
                settlements: 2,
                cities: 2,
                roads: 4,
                longest_road_length: 4,
                knights_used: 0,
                has_longest_road: false,
                has_largest_army: false,
            },
        ];

        let lines = game_ended_lines(&model, 0, 42, &stats)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("winner: p0")));
        assert!(lines.iter().any(|line| line.contains("turns: 42")));
        assert!(lines.iter().any(|line| line.contains("┌───┬")));
        assert!(lines.iter().any(|line| line.contains("│P  │VP│")));
        assert!(lines.iter().any(|line| line.contains("└───┴")));
        assert!(lines.iter().any(|line| line.contains("p0")));
        assert!(lines.iter().any(|line| line.contains("WIN LR")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("[press esc to quit]"))
        );
        assert!(lines.iter().all(|line| line.chars().count() <= 40));
    }

    fn model_with_port_and_resources(
        port_kind: PortKind,
        resources: ResourceCollection,
    ) -> UiModel {
        let mut init = GameInitializationState::default();
        let (port_pos, _) = init
            .board
            .arrangement
            .ports()
            .iter()
            .find(|(_, kind)| **kind == port_kind)
            .or_else(|| init.board.arrangement.ports().iter().next())
            .expect("default board should have ports");
        let settlement_pos = port_pos.intersections()[0];
        let road = init
            .board
            .arrangement
            .path_set()
            .into_iter()
            .find(|path| {
                path.intersections_iter()
                    .any(|intersection| intersection == settlement_pos)
            })
            .map(|pos| Road { pos })
            .expect("port settlement should have adjacent road");
        init.builds
            .try_init_place(
                0,
                road,
                Establishment {
                    pos: settlement_pos,
                    stage: EstablishmentType::Settlement,
                },
            )
            .expect("port settlement should be valid on empty board");
        let mut state = init.finish();
        state
            .transfer_from_bank(resources, 0)
            .expect("bank should fund test resources");
        model_from_state(&state)
    }

    #[test]
    fn bank_trade_options_include_generic_trade_without_ports() {
        let init = GameInitializationState::default();
        let mut state = init.finish();
        state
            .transfer_from_bank(
                ResourceCollection {
                    brick: 4,
                    ..ResourceCollection::ZERO
                },
                0,
            )
            .expect("bank should fund test resources");
        let model = model_from_state(&state);

        let options = bank_trade_options(&model);

        assert!(options.iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::BankGeneric) && trade.give == Resource::Brick
        }));
    }

    #[test]
    fn bank_trade_options_include_universal_and_specific_ports() {
        let universal = model_with_port_and_resources(
            PortKind::Universal,
            ResourceCollection {
                brick: 3,
                ..ResourceCollection::ZERO
            },
        );
        assert!(bank_trade_options(&universal).iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::PortGeneric) && trade.give == Resource::Brick
        }));

        let specific = model_with_port_and_resources(
            PortKind::Special(Resource::Brick),
            ResourceCollection {
                brick: 2,
                ..ResourceCollection::ZERO
            },
        );
        assert!(bank_trade_options(&specific).iter().any(|trade| {
            matches!(trade.kind, BankTradeKind::PortSpecific) && trade.give == Resource::Brick
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
