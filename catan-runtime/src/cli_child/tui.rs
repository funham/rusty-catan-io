//! Stateful ratatui terminal UI.
//!
//! Contains `CliUi`, terminal setup/cleanup, the main draw routine, and interactive
//! selection widgets for board positions, builds, resources, bank trades, and players.

use std::{
    io::{self, Stdout},
    time::Duration,
};

use catan_agents::remote_agent::{LegalDecisionOptions, UiModel};
use catan_core::gameplay::{
    game::event::GameEndPlayerStats,
    primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        player::PlayerId,
        resource::{Resource, ResourceCollection},
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
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{
    labels::{bank_trade_label, build_label, hex_label, intersection_label, path_label},
    panels::{
        adjust_drop_selection, bank_trade_menu_lines, drop_personal_lines, game_ended_lines,
        personal_model_lines, player_menu_lines, public_model_lines, resource_picker_lines,
        snapshot_state_lines,
    },
    render::{field_lines, field_size},
    selectors::{
        board_hex_set, initial_roads_for_settlement, move_hex_by_key, ordered_bank_trades_for_menu,
        selection_status,
    },
};

pub(crate) struct CliUi {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    display_mode: CliDisplayMode,
    message: String,
    overlay: FieldOverlay,
    public_override: Option<Vec<Line<'static>>>,
    personal_override: Option<Vec<Line<'static>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CliDisplayMode {
    Normal,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotInput {
    Snapshot,
    Redraw,
}

impl CliUi {
    pub(crate) fn new(display_mode: CliDisplayMode) -> io::Result<Self> {
        log::trace!("Initializing CLI UI");
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        log::trace!("CLI UI initialized successfully");
        Ok(Self {
            terminal,
            display_mode,
            message: "waiting for host".to_owned(),
            overlay: FieldOverlay::default(),
            public_override: None,
            personal_override: None,
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
        self.draw(None, "", "")
    }

    pub(crate) fn display_mode(&self) -> CliDisplayMode {
        self.display_mode
    }

    pub(crate) fn show_model(&mut self, model: &UiModel, message: String) -> io::Result<()> {
        log::trace!("Showing model with message: {}", message);
        self.message = message;
        self.overlay.selected = None;
        self.overlay.status = SelectionStatus::Neutral;
        self.overlay.preview.clear();
        self.public_override = None;
        self.personal_override = None;
        self.draw(Some(model), "", "")
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

    pub(crate) fn poll_snapshot_input(&mut self) -> io::Result<Option<SnapshotInput>> {
        if !event::poll(Duration::from_millis(25))? {
            return Ok(None);
        }
        match event::read()? {
            CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                if self.display_mode == CliDisplayMode::Snapshot
                    && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
                {
                    Ok(Some(SnapshotInput::Snapshot))
                } else {
                    Ok(None)
                }
            }
            CrosstermEvent::Resize(_, _) => Ok(Some(SnapshotInput::Redraw)),
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

    pub(crate) fn select_drop_cards(
        &mut self,
        model: &UiModel,
    ) -> io::Result<Option<ResourceCollection>> {
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
                let resource = Resource::LIST[selected_resource];
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
                            .unwrap_or(Resource::LIST.len() - 1);
                    }
                    KeyCode::Right => {
                        selected_resource = (selected_resource + 1) % Resource::LIST.len();
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
            let resource = Resource::LIST[selected];
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
                        selected = selected.checked_sub(1).unwrap_or(Resource::LIST.len() - 1);
                    }
                    KeyCode::Right => {
                        selected = (selected + 1) % Resource::LIST.len();
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
                Some(model) => match self.display_mode {
                    CliDisplayMode::Normal => {
                        let (field_width, field_height) = field_size();
                        let field_pane_width = field_width.saturating_add(2);
                        let field_pane_height = field_height.saturating_add(2);
                        let body_chunks = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([
                                Constraint::Length(field_pane_width),
                                Constraint::Min(36),
                            ])
                            .split(chunks[1]);

                        let field_chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(field_pane_height),
                                Constraint::Min(8),
                            ])
                            .split(body_chunks[0]);

                        let field = Paragraph::new(field_lines(model, &overlay))
                            .block(Block::default().borders(Borders::ALL).title("Field"));
                        frame.render_widget(field, field_chunks[0]);

                        let has_public_override = public_override.is_some();
                        let title = if has_public_override {
                            "Game Ended"
                        } else {
                            "Public"
                        };
                        let public = Paragraph::new(
                            public_override.unwrap_or_else(|| public_model_lines(model)),
                        )
                        .wrap(Wrap { trim: false })
                        .block(Block::default().borders(Borders::ALL).title(title));
                        frame.render_widget(public, body_chunks[1]);

                        let personal = Paragraph::new(
                            personal_override.unwrap_or_else(|| personal_model_lines(model)),
                        )
                        .wrap(Wrap { trim: false })
                        .block(Block::default().borders(Borders::ALL).title("Personal"));
                        frame.render_widget(personal, field_chunks[1]);
                    }
                    CliDisplayMode::Snapshot => {
                        let (field_width, _) = field_size();
                        let field_pane_width = field_width.saturating_add(2);
                        let body_chunks = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([
                                Constraint::Length(field_pane_width),
                                Constraint::Min(42),
                            ])
                            .split(chunks[1]);

                        let field = Paragraph::new(field_lines(model, &overlay))
                            .block(Block::default().borders(Borders::ALL).title("Field"));
                        frame.render_widget(field, body_chunks[0]);

                        let state = Paragraph::new(snapshot_state_lines(model))
                            .wrap(Wrap { trim: false })
                            .block(Block::default().borders(Borders::ALL).title("State"));
                        frame.render_widget(state, body_chunks[1]);
                    }
                },
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
                Line::from(match self.display_mode {
                    CliDisplayMode::Normal => {
                        "Text: Esc clears, Enter submits. Select: arrows/tab move, Enter picks."
                    }
                    CliDisplayMode::Snapshot => {
                        "Snapshot observer: press s after an update to save the latest exact state."
                    }
                }),
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
