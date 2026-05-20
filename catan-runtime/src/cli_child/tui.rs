//! Stateful ratatui terminal UI.
//!
//! Contains `CliUi`, terminal setup/cleanup, the main draw routine, and interactive
//! selection widgets for board positions, builds, resources, bank trades, and players.

use std::{
    io::{self, Stdout},
    time::Duration,
};

use catan_agents::remote_agent::{LegalDecisionOptions, UiModel, ui_model_summary};
use catan_core::gameplay::{
    game::event::GameEndPlayerStats,
    primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        player::PlayerId,
        resource::{Resource, ResourceSet},
        trade::BankTrade,
    },
};
use catan_core::topology::{Hex, Intersection};
use catan_render::field::{FieldOverlay, FieldPreview, FieldSelection, SelectionStatus};
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{
    journal::{EventJournal, JournalEntry},
    labels::{bank_trade_label, build_label, hex_label, intersection_label, path_label},
    layout::{NormalLayoutAreas, SnapshotLayoutAreas, normal_layout_areas, snapshot_layout_areas},
    panels::{
        adjust_drop_selection, bank_panel_lines, bank_trade_menu_lines, drop_personal_lines,
        game_ended_lines, personal_model_lines, player_menu_lines, public_player_lines,
        resource_picker_lines, snapshot_state_lines,
    },
    render::{field_lines, field_lines_cropped_left, field_size},
    selectors::{
        board_hex_set, initial_roads_for_settlement, move_hex_by_key, ordered_bank_trades_for_menu,
        selection_status,
    },
};

pub(crate) struct CliUi {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    view_mode: CliViewMode,
    message: String,
    overlay: FieldOverlay,
    public_override: Option<Vec<Line<'static>>>,
    personal_override: Option<Vec<Line<'static>>>,
    observer_event_count: u64,
    observer_summary: Option<String>,
    journal: EventJournal,
    active_player: Option<PlayerId>,
    show_command_help: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CliViewMode {
    Normal,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlInput {
    SaveSnapshot,
    Redraw,
}

impl CliUi {
    pub(crate) fn new(view_mode: CliViewMode) -> io::Result<Self> {
        log::trace!("Initializing CLI UI");
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        log::trace!("CLI UI initialized successfully");
        Ok(Self {
            terminal,
            view_mode,
            message: "waiting for host".to_owned(),
            overlay: FieldOverlay::default(),
            public_override: None,
            personal_override: None,
            observer_event_count: 0,
            observer_summary: None,
            journal: EventJournal::new(64),
            active_player: None,
            show_command_help: false,
        })
    }

    pub(crate) fn set_message(&mut self, message: String) -> io::Result<()> {
        log::trace!("Setting UI message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
        self.show_command_help = false;
        self.draw(None, "", "")
    }

    pub(crate) fn view_mode(&self) -> CliViewMode {
        self.view_mode
    }

    pub(crate) fn show_model(&mut self, model: &UiModel, message: String) -> io::Result<()> {
        log::trace!("Showing model with message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
        self.show_command_help = false;
        self.draw(Some(model), "", "")
    }

    pub(crate) fn show_observer_model(
        &mut self,
        model: &UiModel,
        message: String,
        event_count: u64,
    ) -> io::Result<()> {
        self.observer_event_count = event_count;
        self.observer_summary = Some(ui_model_summary(model));
        self.show_model(model, message)
    }

    pub(crate) fn record_game_event(
        &mut self,
        event: &catan_core::gameplay::game::event::GameEvent,
    ) -> Option<String> {
        match event {
            catan_core::gameplay::game::event::GameEvent::TurnStarted { player_id, .. } => {
                self.active_player = Some(*player_id);
            }
            catan_core::gameplay::game::event::GameEvent::GameFinished { .. } => {
                self.active_player = None;
            }
            _ => {}
        }
        self.journal.push_event(event)
    }

    pub(crate) fn set_active_player(&mut self, player_id: Option<PlayerId>) {
        self.active_player = player_id;
    }

    pub(crate) fn current_message(&self) -> String {
        self.message.clone()
    }

    pub(crate) fn show_game_ended(
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
        self.show_command_help = false;
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

    pub(crate) fn prompt(&mut self, model: &UiModel, prompt: &str) -> io::Result<String> {
        log::trace!("Prompting user: {}", prompt);
        let mut input = String::new();
        self.message = "enter command".to_owned();
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
        self.show_command_help = false;
        loop {
            self.draw(Some(model), prompt, &input)?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        if input.trim() == "help" {
                            self.show_command_help = true;
                            input.clear();
                            continue;
                        }
                        log::trace!("User input: {}", input);
                        self.show_command_help = false;
                        return Ok(input.trim().to_owned());
                    }
                    KeyCode::Backspace => {
                        self.show_command_help = false;
                        input.pop();
                    }
                    KeyCode::Esc => {
                        self.show_command_help = false;
                        input.clear();
                    }
                    KeyCode::Char(c) => {
                        self.show_command_help = false;
                        input.push(c);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn poll_control_input(&mut self) -> io::Result<Option<ControlInput>> {
        if !event::poll(Duration::from_millis(25))? {
            return Ok(None);
        }
        match event::read()? {
            CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                if self.view_mode == CliViewMode::Snapshot
                    && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
                {
                    Ok(Some(ControlInput::SaveSnapshot))
                } else {
                    Ok(None)
                }
            }
            CrosstermEvent::Resize(_, _) => Ok(Some(ControlInput::Redraw)),
            _ => Ok(None),
        }
    }

    pub(crate) fn select_hex_where(
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

    pub(crate) fn select_intersection_where(
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
            let mut selected: usize = 0;
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

    pub(crate) fn select_initial_road(
        &mut self,
        model: &UiModel,
        legal: &LegalDecisionOptions,
        settlement_pos: Intersection,
        prompt: &str,
    ) -> io::Result<Road> {
        log::trace!("Selecting initial road for settlement {:?}", settlement_pos);
        let roads = initial_roads_for_settlement(legal, settlement_pos);
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
                vtx: settlement_pos,
                stage: EstablishmentType::Settlement,
            },
        }];
        self.message = "cycle adjacent initial roads with arrows/tab; enter confirms".to_owned();

        let mut selected = 0;
        loop {
            let road = roads[selected];
            self.overlay.selected = Some(FieldSelection::Path(road.path));
            self.overlay.status = SelectionStatus::Available;
            self.draw(Some(model), prompt, &path_label(road.path))?;
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

    pub(crate) fn select_build(
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
                Build::Road(road) => FieldSelection::Path(road.path),
                Build::Establishment(establishment) => {
                    FieldSelection::Intersection(establishment.vtx)
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

    pub(crate) fn select_drop_cards(&mut self, model: &UiModel) -> io::Result<Option<ResourceSet>> {
        let Some(private) = &model.private else {
            self.message = "no private resources".to_owned();
            return Ok(None);
        };

        let required = private.resources.total() / 2;
        let mut selected_resource = 0;
        let mut selected = ResourceSet::EMPTY;
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
                let resource = Resource::ALL[selected_resource];
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
                            .unwrap_or(Resource::ALL.len() - 1);
                    }
                    KeyCode::Right => {
                        selected_resource = (selected_resource + 1) % Resource::ALL.len();
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

    pub(crate) fn select_bank_trade(
        &mut self,
        model: &UiModel,
        legal: &LegalDecisionOptions,
    ) -> io::Result<Option<BankTrade>> {
        let options = ordered_bank_trades_for_menu(legal);
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

    pub(crate) fn select_resource(
        &mut self,
        model: &UiModel,
        prompt: &str,
        message: &str,
    ) -> io::Result<Option<Resource>> {
        let mut selected = 0;
        self.message = message.to_owned();
        loop {
            self.personal_override = Some(resource_picker_lines(selected));
            let resource = Resource::ALL[selected];
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
                        selected = selected.checked_sub(1).unwrap_or(Resource::ALL.len() - 1);
                    }
                    KeyCode::Right => {
                        selected = (selected + 1) % Resource::ALL.len();
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn select_player(
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
        let view_mode = self.view_mode;
        let observer_event_count = self.observer_event_count;
        let observer_summary = self.observer_summary.clone();
        let journal_entries = self.journal.entries().cloned().collect::<Vec<_>>();
        let active_player = self.active_player;
        let show_command_help = self.show_command_help;
        self.terminal.draw(|frame| match view_mode {
            CliViewMode::Normal => {
                let layout = normal_layout_areas(
                    frame.area(),
                    field_size(),
                    command_panel_line_count(view_mode, prompt, input, show_command_help),
                );

                render_status(
                    frame,
                    layout.status,
                    &message,
                    observer_event_count,
                    observer_summary.as_deref(),
                );

                match model {
                    Some(model) => render_normal_layout(
                        frame,
                        layout,
                        model,
                        &overlay,
                        NormalRenderState {
                            public_override,
                            personal_override,
                            journal_entries: &journal_entries,
                            active_player,
                        },
                    ),
                    None => render_waiting_layout(frame, layout.field),
                }

                render_command(
                    frame,
                    layout.command,
                    view_mode,
                    prompt,
                    input,
                    show_command_help,
                );
            }
            CliViewMode::Snapshot => {
                let layout = snapshot_layout_areas(
                    frame.area(),
                    field_size(),
                    command_panel_line_count(view_mode, prompt, input, show_command_help),
                );

                render_status(
                    frame,
                    layout.status,
                    &message,
                    observer_event_count,
                    observer_summary.as_deref(),
                );

                match model {
                    Some(model) => render_snapshot_layout(
                        frame,
                        layout,
                        model,
                        &overlay,
                        CommandRenderState {
                            prompt,
                            input,
                            show_help: show_command_help,
                        },
                        active_player,
                    ),
                    None => render_snapshot_waiting_layout(
                        frame,
                        layout,
                        prompt,
                        input,
                        show_command_help,
                    ),
                }
            }
        })?;
        Ok(())
    }
}

fn render_status(
    frame: &mut Frame<'_>,
    area: Rect,
    message: &str,
    event_count: u64,
    summary: Option<&str>,
) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled("rusty-catan", Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::raw(message.to_owned()),
        Span::raw(observer_status_suffix(event_count, summary)),
    ]))
    .block(panel_block("Status"));
    frame.render_widget(title, area);
}

fn render_journal(frame: &mut Frame<'_>, area: Rect, entries: &[JournalEntry]) {
    let visible_rows = usize::from(area.height.saturating_sub(2));
    let visible_cols = usize::from(area.width.saturating_sub(2));
    let lines = if entries.is_empty() {
        vec![Line::from(Span::styled(
            "no events yet",
            Style::default().fg(Color::Gray),
        ))]
    } else {
        entries
            .iter()
            .rev()
            .take(visible_rows.max(1))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|entry| journal_entry_line(entry, visible_cols))
            .collect()
    };
    let journal = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel_block("Events"));
    frame.render_widget(journal, area);
}

fn journal_entry_line(entry: &JournalEntry, visible_cols: usize) -> Line<'static> {
    match entry {
        JournalEntry::Event { text, .. } => styled_event_text(text),
        JournalEntry::Divider => {
            Line::from(Span::styled("─".repeat(visible_cols), subtle_panel_style()))
        }
    }
}

fn styled_event_text(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            token.push(ch);
        } else {
            push_event_token(&mut spans, &token);
            token.clear();
            spans.push(Span::raw(ch.to_string()));
        }
    }
    push_event_token(&mut spans, &token);
    Line::from(spans)
}

fn push_event_token(spans: &mut Vec<Span<'static>>, token: &str) {
    if token.is_empty() {
        return;
    }
    if let Some(player_id) = parse_player_token(token) {
        spans.push(Span::styled(
            token.to_owned(),
            player_event_style(player_id),
        ));
    } else if let Some(style) = resource_event_style(token) {
        spans.push(Span::styled(token.to_owned(), style));
    } else if matches!(
        token,
        "development" | "Knight" | "Monopoly" | "Road" | "Building" | "Plenty"
    ) {
        spans.push(Span::styled(
            token.to_owned(),
            Style::default().fg(Color::Magenta),
        ));
    } else {
        spans.push(Span::raw(token.to_owned()));
    }
}

fn parse_player_token(token: &str) -> Option<PlayerId> {
    token
        .strip_prefix('p')
        .and_then(|digits| digits.parse::<u8>().ok())
        .map(PlayerId::new)
}

fn player_event_style(player_id: PlayerId) -> Style {
    Style::default().fg(catan_render::adapters::ratatui::color(
        catan_render::field::FieldRenderer::player_color(player_id),
    ))
}

fn resource_event_style(token: &str) -> Option<Style> {
    let resource = match token {
        "Brick" | "brick" => Resource::Brick,
        "Wood" | "wood" => Resource::Wood,
        "Wheat" | "wheat" => Resource::Wheat,
        "Sheep" | "sheep" => Resource::Sheep,
        "Ore" | "ore" => Resource::Ore,
        _ => return None,
    };
    Some(Style::default().fg(catan_render::adapters::ratatui::color(
        catan_render::field::FieldRenderer::resource_color(resource),
    )))
}

fn render_bank(frame: &mut Frame<'_>, area: Rect, model: &UiModel) {
    let bank = Paragraph::new(bank_panel_lines(model))
        .wrap(Wrap { trim: false })
        .block(panel_block("Bank"));
    frame.render_widget(bank, area);
}

fn render_players(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &UiModel,
    active_player: Option<PlayerId>,
) {
    if model.public.players.is_empty() || area.height == 0 || area.width == 0 {
        return;
    }

    let constraints =
        vec![
            Constraint::Ratio(1, model.public.players.len().try_into().unwrap_or(u32::MAX),);
            model.public.players.len()
        ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (idx, player) in model.public.players.iter().enumerate() {
        let is_active = active_player == Some(player.player_id);
        let title = format!("p{}", player.player_id);
        let block = panel_block(&title).border_style(if is_active {
            active_panel_style()
        } else {
            subtle_panel_style()
        });
        let panel = Paragraph::new(public_player_lines(model, player))
            .wrap(Wrap { trim: false })
            .block(block);
        frame.render_widget(panel, chunks[idx]);
    }
}

fn right_column_area(layout: NormalLayoutAreas) -> Rect {
    let x = layout.journal.x;
    let y = layout.journal.y;
    let width = layout
        .journal
        .width
        .max(layout.bank.width)
        .max(layout.players.width);
    let bottom = layout
        .journal
        .bottom()
        .max(layout.bank.bottom())
        .max(layout.players.bottom());
    Rect::new(x, y, width, bottom.saturating_sub(y))
}

fn panel_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(subtle_panel_style())
        .title(title.to_owned())
}

fn subtle_panel_style() -> Style {
    Style::default().fg(Color::Indexed(244))
}

fn active_panel_style() -> Style {
    Style::default().fg(Color::Indexed(39))
}

struct NormalRenderState<'a> {
    public_override: Option<Vec<Line<'static>>>,
    personal_override: Option<Vec<Line<'static>>>,
    journal_entries: &'a [JournalEntry],
    active_player: Option<PlayerId>,
}

#[derive(Clone, Copy)]
struct CommandRenderState<'a> {
    prompt: &'a str,
    input: &'a str,
    show_help: bool,
}

fn render_normal_layout(
    frame: &mut Frame<'_>,
    layout: NormalLayoutAreas,
    model: &UiModel,
    overlay: &FieldOverlay,
    state: NormalRenderState<'_>,
) {
    let field = Paragraph::new(field_lines(model, overlay)).block(panel_block("Field"));
    frame.render_widget(field, layout.field);

    if let Some(lines) = state.public_override {
        let area = right_column_area(layout);
        let public = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(panel_block("Game Ended"));
        frame.render_widget(public, area);
    } else {
        render_journal(frame, layout.journal, state.journal_entries);
        render_bank(frame, layout.bank, model);
        render_players(frame, layout.players, model, state.active_player);
    }

    let personal = Paragraph::new(
        state
            .personal_override
            .unwrap_or_else(|| personal_model_lines(model)),
    )
    .wrap(Wrap { trim: false })
    .block(panel_block("Private"));
    frame.render_widget(personal, layout.personal);
}

fn render_snapshot_layout(
    frame: &mut Frame<'_>,
    layout: SnapshotLayoutAreas,
    model: &UiModel,
    overlay: &FieldOverlay,
    command: CommandRenderState<'_>,
    active_player: Option<PlayerId>,
) {
    let snapshot_field_crop: usize = 2;
    let field = Paragraph::new(field_lines_cropped_left(
        model,
        overlay,
        snapshot_field_crop,
    ))
    .block(panel_block("Field"));
    frame.render_widget(field, layout.field);

    render_command(
        frame,
        layout.command,
        CliViewMode::Snapshot,
        command.prompt,
        command.input,
        command.show_help,
    );

    let state_inner_width = layout.state.width.saturating_sub(2);
    let state = Paragraph::new(snapshot_state_lines(
        model,
        state_inner_width,
        active_player,
    ))
    .wrap(Wrap { trim: false })
    .block(panel_block("State"));
    frame.render_widget(state, layout.state);
}

fn render_snapshot_waiting_layout(
    frame: &mut Frame<'_>,
    layout: SnapshotLayoutAreas,
    prompt: &str,
    input: &str,
    show_command_help: bool,
) {
    render_waiting_layout(frame, layout.field);
    render_command(
        frame,
        layout.command,
        CliViewMode::Snapshot,
        prompt,
        input,
        show_command_help,
    );

    let state = Paragraph::new(vec![Line::from("waiting for game state")])
        .wrap(Wrap { trim: false })
        .block(panel_block("State"));
    frame.render_widget(state, layout.state);
}

fn render_waiting_layout(frame: &mut Frame<'_>, area: Rect) {
    let body = Paragraph::new(vec![Line::from("waiting for game state")])
        .wrap(Wrap { trim: false })
        .block(panel_block("Game"));
    frame.render_widget(body, area);
}

fn render_command(
    frame: &mut Frame<'_>,
    area: Rect,
    view_mode: CliViewMode,
    prompt: &str,
    input: &str,
    show_help: bool,
) {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let input = Paragraph::new(command_panel_lines(
        view_mode,
        prompt,
        input,
        show_help,
        inner_width,
    ))
    .wrap(Wrap { trim: false })
    .block(panel_block("Command"));
    frame.render_widget(input, area);
}

fn command_panel_lines(
    view_mode: CliViewMode,
    prompt: &str,
    input: &str,
    show_help: bool,
    inner_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !prompt.is_empty() {
        lines.push(Line::from(fit_command_text(prompt, inner_width)));
    }
    if !input.is_empty() {
        lines.push(Line::from(Span::styled(
            fit_command_text(input, inner_width),
            Style::default().fg(Color::Yellow),
        )));
    }
    if show_help {
        lines.extend(command_help_lines(view_mode, inner_width));
    }
    lines
}

fn command_panel_line_count(
    view_mode: CliViewMode,
    prompt: &str,
    input: &str,
    show_help: bool,
) -> usize {
    command_panel_lines(view_mode, prompt, input, show_help, usize::MAX).len()
}

fn command_help_lines(view_mode: CliViewMode, width: usize) -> Vec<Line<'static>> {
    let help = match view_mode {
        CliViewMode::Normal => [
            "roll/r, end/e, buy dev/bd",
            "build: br, bs, bc or build road|settlement|city ...",
            "bank-trade: bt or bank-trade give take G4|G3|S2",
            "dev: kn, yp, m, rb or use knight|yop|monopoly|roadbuild ...",
            "drop: five resource counts or drop",
        ]
        .as_slice(),
        CliViewMode::Snapshot => ["s saves the latest exact state snapshot"].as_slice(),
    };
    help.iter()
        .map(|line| {
            Line::from(Span::styled(
                fit_command_text(line, width),
                Style::default().fg(Color::Gray),
            ))
        })
        .collect()
}

fn fit_command_text(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_owned();
    }
    if width <= 3 {
        return ".".repeat(width);
    }

    let mut output: String = text.chars().take(width - 3).collect();
    output.push_str("...");
    output
}

impl Drop for CliUi {
    fn drop(&mut self) {
        log::trace!("Dropping CLI UI, cleaning up terminal");
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn observer_status_suffix(event_count: u64, summary: Option<&str>) -> String {
    match (event_count, summary) {
        (0, _) => String::new(),
        (count, Some(summary)) => format!("  | events:{count} {summary}"),
        (count, None) => format!("  | events:{count}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{CliViewMode, command_panel_lines};

    #[test]
    fn command_panel_hides_action_hints_by_default() {
        let rendered = command_panel_lines(CliViewMode::Normal, "command: ", "", false, 80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("command:"));
        assert!(!rendered.contains("roll"));
        assert!(!rendered.contains("bank-trade"));
        assert!(!rendered.contains("help"));
    }

    #[test]
    fn command_panel_shows_action_hints_after_help_command() {
        let rendered = command_panel_lines(CliViewMode::Normal, "command: ", "", true, 80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("roll"));
        assert!(rendered.contains("bank-trade"));
        assert!(rendered.contains("dev"));
    }
}
