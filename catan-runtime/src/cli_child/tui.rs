//! Stateful ratatui terminal UI.
//!
//! Contains `CliUi`, terminal setup/cleanup, the main draw routine, and interactive
//! selection widgets for board positions, builds, resources, bank trades, and players.

use std::{
    io::{self, Stdout},
    time::Duration,
};

use catan_agents::remote_agent::{
    LegalDecisionOptions, UiModel, UiPrivatePlayer, UiTradeOffer, UiTradeSession, ui_model_summary,
};
use catan_core::gameplay::{
    game::{
        event::{GameEndPlayerStats, GameEvent},
        input::TradeCommand,
        trade::{TradeOfferId, TradeResponseState, TradeSessionId},
    },
    primitives::{
        build::{Build, Establishment, EstablishmentType, Road},
        player::PlayerId,
        resource::{Resource, ResourceSet},
        trade::{BankTrade, PlayerTrade},
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
        adjust_discard_selection, bank_panel_lines, bank_trade_menu_lines, discard_personal_lines,
        game_ended_lines, personal_model_lines, player_menu_lines, player_trade_builder_lines,
        public_player_lines, resource_choice_lines, snapshot_state_lines, trade_panel_lines,
        trade_tree_lines_for_viewer,
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
    interactive_override: Option<Vec<Line<'static>>>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TradeResponseMenuAction {
    Accept(TradeOfferId),
    Reject,
    Counter(PlayerTrade),
}

#[derive(Debug, Clone)]
pub(crate) struct CardGlyph {
    label: String,
    style: CardGlyphStyle,
    indices: [Option<CardGlyphIndex>; 3],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CardGlyphStyle {
    border: Style,
    face: Style,
}

#[derive(Debug, Clone, Copy)]
struct CardGlyphIndex {
    value: u16,
    style: Style,
}

impl CardGlyph {
    pub(crate) fn new(label: impl Into<String>, style: Style) -> Self {
        Self {
            label: label.into(),
            style: CardGlyphStyle {
                border: style,
                face: style,
            },
            indices: [None, None, None],
        }
    }

    #[allow(dead_code)]
    pub(crate) fn border_style(mut self, style: Style) -> Self {
        self.style.border = style;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn face_style(mut self, style: Style) -> Self {
        self.style.face = style;
        self
    }

    pub(crate) fn index_top(mut self, value: u16, style: Style) -> Self {
        self.indices[0] = Some(CardGlyphIndex { value, style });
        self
    }

    pub(crate) fn index_mid(mut self, value: u16, style: Style) -> Self {
        self.indices[1] = Some(CardGlyphIndex { value, style });
        self
    }

    pub(crate) fn index_bottom(mut self, value: u16, style: Style) -> Self {
        self.indices[2] = Some(CardGlyphIndex { value, style });
        self
    }

    #[allow(dead_code)]
    pub(crate) fn lines(&self) -> Vec<Line<'static>> {
        let rows = self.row_texts();
        rows.into_iter()
            .zip(self.indices)
            .map(|(row, index)| {
                let mut spans = row;
                push_index(&mut spans, index);
                Line::from(spans)
            })
            .collect()
    }

    pub(crate) fn push_to_rows(&self, rows: &mut [Vec<Span<'static>>; 3]) {
        let row_texts = self.row_texts();
        for (idx, row) in row_texts.into_iter().enumerate() {
            rows[idx].extend(row);
            if self.indices.iter().any(Option::is_some) {
                push_index(&mut rows[idx], self.indices[idx]);
            }
        }
    }

    fn row_texts(&self) -> [Vec<Span<'static>>; 3] {
        [
            vec![Span::styled("┌──┐", self.style.border)],
            vec![
                Span::styled("│", self.style.border),
                Span::styled(
                    format!("{:^2}", truncate_cell_text(&self.label, 2)),
                    self.style.face,
                ),
                Span::styled("│", self.style.border),
            ],
            vec![Span::styled("└──┘", self.style.border)],
        ]
    }
}

fn push_index(spans: &mut Vec<Span<'static>>, index: Option<CardGlyphIndex>) {
    spans.push(Span::raw(" "));
    match index {
        Some(index) => spans.push(Span::styled(
            format!("{:>1}", count_label(Some(index.value))),
            index.style,
        )),
        None => spans.push(Span::raw(count_label(None))),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MiniCardGlyph {
    face: String,
    face_style: Style,
    left_bracket_style: Style,
    right_bracket_style: Style,
}

impl MiniCardGlyph {
    pub(crate) fn new(face: impl Into<String>) -> Self {
        Self {
            face: face.into(),
            face_style: Style::default(),
            left_bracket_style: Style::default(),
            right_bracket_style: Style::default(),
        }
    }

    pub(crate) fn face_style(mut self, style: Style) -> Self {
        self.face_style = style;
        self
    }

    pub(crate) fn bracket_style(mut self, style: Style) -> Self {
        self.left_bracket_style = style;
        self.right_bracket_style = style;
        self
    }

    pub(crate) fn bracket_styles(mut self, left: Style, right: Style) -> Self {
        self.left_bracket_style = left;
        self.right_bracket_style = right;
        self
    }

    pub(crate) fn spans(&self) -> Vec<Span<'static>> {
        vec![
            Span::styled("[", self.left_bracket_style),
            Span::styled(truncate_cell_text(&self.face, 1), self.face_style),
            Span::styled("]", self.right_bracket_style),
        ]
    }

    pub(crate) fn push_to(&self, target: &mut Vec<Span<'static>>) {
        target.extend(self.spans());
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InlineBadge {
    spans: Vec<Span<'static>>,
}

impl InlineBadge {
    pub(crate) fn new(spans: Vec<Span<'static>>) -> Self {
        Self { spans }
    }

    pub(crate) fn push_to(self, target: &mut Vec<Span<'static>>) {
        target.extend(self.spans);
    }
}

pub(crate) fn append_gap(rows: &mut [Vec<Span<'static>>; 3], gap: &'static str) {
    for row in rows {
        row.push(Span::raw(gap));
    }
}

pub(crate) fn join_lines_horizontal(
    left: &[Line<'static>],
    separator: Line<'static>,
    right: &[Line<'static>],
) -> Vec<Line<'static>> {
    let rows = left.len().max(right.len());
    (0..rows)
        .map(|idx| {
            let mut spans = left
                .get(idx)
                .cloned()
                .unwrap_or_else(|| Line::from(""))
                .spans;
            spans.extend(separator.clone().spans);
            spans.extend(
                right
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| Line::from(""))
                    .spans,
            );
            Line::from(spans)
        })
        .collect()
}

fn count_label(count: Option<u16>) -> String {
    count
        .map(|count| count.min(99).to_string())
        .unwrap_or_else(|| " ".to_owned())
}

fn truncate_cell_text(text: &str, width: usize) -> String {
    let mut value = text.chars().take(width).collect::<String>();
    while value.chars().count() < width {
        value.push(' ');
    }
    value
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
            interactive_override: None,
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
        self.interactive_override = None;
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
        self.interactive_override = None;
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
        event: &GameEvent,
        model: &UiModel,
    ) -> Option<String> {
        match event {
            GameEvent::TurnStarted { player_id, .. } => {
                self.active_player = Some(*player_id);
            }
            GameEvent::GameFinished { .. } => {
                self.active_player = None;
            }
            _ => {}
        }
        self.journal.push_event_with_model(event, Some(model))
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
        self.interactive_override = None;
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
        self.interactive_override = None;
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

        let mut selected: usize = 0;
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
        self.select_build_with_preview(model, builds, prompt, Vec::new())
    }

    pub(crate) fn select_build_with_preview(
        &mut self,
        model: &UiModel,
        builds: Vec<Build>,
        prompt: &str,
        fixed_preview: Vec<FieldPreview>,
    ) -> io::Result<Option<Build>> {
        if builds.is_empty() {
            self.message = "no legal placements".to_owned();
            return Ok(None);
        }

        let actor = model.actor.unwrap_or_default();
        let mut selected: usize = 0;
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
            self.overlay.preview = fixed_preview.clone();
            self.overlay.preview.push(match build {
                Build::Road(road) => FieldPreview::Road {
                    player_id: actor,
                    road,
                },
                Build::Establishment(establishment) => FieldPreview::Establishment {
                    player_id: actor,
                    establishment,
                },
            });
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

    pub(crate) fn select_discard_cards(
        &mut self,
        model: &UiModel,
    ) -> io::Result<Option<ResourceSet>> {
        let Some(private) = &model.private else {
            self.message = "no private resources".to_owned();
            return Ok(None);
        };

        let required = private.resources.total() / 2;
        let mut selected_resource = 0;
        let mut selected = ResourceSet::EMPTY;
        self.message =
            format!("select exactly {required} cards to discard; enter confirms; esc cancels");

        loop {
            self.interactive_override = Some(discard_personal_lines(
                private.player_id,
                &private.resources,
                &private.dev_cards,
                &selected,
                required,
                selected_resource,
            ));
            self.draw(
                Some(model),
                "discard: ",
                &format!("selected {} of {}", selected.total(), required),
            )?;

            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                let resource = Resource::ALL[selected_resource];
                match key.code {
                    KeyCode::Enter => {
                        if selected.total() == required {
                            self.interactive_override = None;
                            return Ok(Some(selected));
                        }
                        self.message =
                            format!("selected {} cards; expected {}", selected.total(), required);
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
                        self.message = "discard cancelled".to_owned();
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
                        adjust_discard_selection(&private.resources, &mut selected, resource, 1);
                    }
                    KeyCode::Down => {
                        adjust_discard_selection(&private.resources, &mut selected, resource, -1);
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
            self.interactive_override = Some(bank_trade_menu_lines(&options, selected));
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
                        self.interactive_override = None;
                        return Ok(Some(options[selected]));
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
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

    pub(crate) fn select_trade_response_action(
        &mut self,
        model: &UiModel,
        session_id: TradeSessionId,
    ) -> io::Result<Option<TradeResponseMenuAction>> {
        let Some(session) = model
            .public
            .trade_sessions
            .iter()
            .find(|session| session.id == session_id)
            .or_else(|| model.public.trade_sessions.last())
        else {
            self.message = "no active trade session".to_owned();
            return Ok(None);
        };

        let offer_indices = session
            .offers
            .iter()
            .enumerate()
            .filter_map(|(idx, offer)| (offer.proposer == session.proposer).then_some(idx))
            .collect::<Vec<_>>();
        if offer_indices.is_empty() {
            self.message = "no active trade offers".to_owned();
            return Ok(None);
        }

        let mut selected = 0;
        self.message = "select trade with up/down; enter decides; esc rejects trade".to_owned();
        loop {
            let offer_idx = offer_indices[selected];
            self.interactive_override = Some(trade_browser_lines(
                session,
                Some(offer_idx),
                None,
                model.private.as_ref(),
            ));
            self.draw(
                Some(model),
                "trade: ",
                &format!("offer #{}", session.offers[offer_idx].id.0),
            )?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        let offer = &session.offers[offer_idx];
                        let actions =
                            trade_response_actions_for_offer(model.private.as_ref(), offer);
                        match self.select_trade_decision(
                            model,
                            session,
                            Some(offer_idx),
                            actions,
                        )? {
                            Some("accept") => {
                                self.interactive_override = None;
                                return Ok(Some(TradeResponseMenuAction::Accept(offer.id)));
                            }
                            Some("reject") => {
                                self.interactive_override = None;
                                return Ok(Some(TradeResponseMenuAction::Reject));
                            }
                            Some("counter") => {
                                if let Some(offer) = self.select_player_trade_offer(model)? {
                                    return Ok(Some(TradeResponseMenuAction::Counter(offer)));
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
                        self.message = "trade rejected".to_owned();
                        return Ok(Some(TradeResponseMenuAction::Reject));
                    }
                    KeyCode::Up => {
                        selected = selected.checked_sub(1).unwrap_or(offer_indices.len() - 1)
                    }
                    KeyCode::Down => selected = (selected + 1) % offer_indices.len(),
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn select_trade_owner_action(
        &mut self,
        model: &UiModel,
        session_id: TradeSessionId,
    ) -> io::Result<Option<TradeCommand>> {
        let Some(session) = model
            .public
            .trade_sessions
            .iter()
            .find(|session| session.id == session_id)
            .or_else(|| model.public.trade_sessions.last())
        else {
            self.message = "no active trade session".to_owned();
            return Ok(None);
        };

        let mut selected: usize = 0;
        let row_count = session.offers.len() + 1;
        self.message = "select trade with up/down; enter decides; esc cancels trade".to_owned();
        loop {
            let selected_offer = selected.checked_sub(1);
            self.interactive_override = Some(trade_browser_lines(
                session,
                selected_offer,
                Some(selected == 0),
                model.private.as_ref(),
            ));
            self.draw(
                Some(model),
                "trade: ",
                if selected == 0 {
                    "+ new offer"
                } else {
                    "selected offer"
                },
            )?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter if selected == 0 => {
                        if let Some(offer) = self.select_player_trade_offer(model)? {
                            return Ok(Some(TradeCommand::Propose { offer }));
                        }
                    }
                    KeyCode::Enter => {
                        let offer_idx = selected - 1;
                        let offer = &session.offers[offer_idx];
                        let actions: &[&str] = if offer.proposer == session.proposer {
                            if trade_offer_is_accepted(session, offer.id) {
                                &["commit", "cancel"]
                            } else {
                                &["cancel"]
                            }
                        } else {
                            &["commit", "reject"]
                        };
                        match self.select_trade_decision(
                            model,
                            session,
                            Some(offer_idx),
                            actions,
                        )? {
                            Some("commit") => {
                                self.interactive_override = None;
                                return Ok(Some(TradeCommand::Commit { offer_id: offer.id }));
                            }
                            Some("reject") => {
                                self.interactive_override = None;
                                return Ok(Some(TradeCommand::Reject { offer_id: offer.id }));
                            }
                            Some("cancel") => {
                                self.interactive_override = None;
                                return Ok(Some(TradeCommand::Cancel));
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
                        self.message = "trade cancelled".to_owned();
                        return Ok(Some(TradeCommand::Cancel));
                    }
                    KeyCode::Up => selected = selected.checked_sub(1).unwrap_or(row_count - 1),
                    KeyCode::Down => selected = (selected + 1) % row_count,
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn select_player_trade_offer(
        &mut self,
        model: &UiModel,
    ) -> io::Result<Option<PlayerTrade>> {
        let Some(private) = &model.private else {
            self.message = "no private resources".to_owned();
            return Ok(None);
        };

        let available = private.resources;
        let mut give = ResourceSet::EMPTY;
        let mut take = ResourceSet::EMPTY;
        let mut selected_resource = 0;
        let mut editing_give = true;
        self.message = "build trade with arrows/tab; enter offers; esc cancels".to_owned();

        loop {
            self.interactive_override = Some(player_trade_builder_lines(
                &available,
                &give,
                &take,
                selected_resource,
                editing_give,
            ));
            self.draw(
                Some(model),
                "player-trade: ",
                &player_trade_builder_summary(&give, &take, selected_resource, editing_give),
            )?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                let resource = Resource::ALL[selected_resource];
                match key.code {
                    KeyCode::Enter => {
                        if give.is_empty() || take.is_empty() {
                            self.message =
                                "trade needs at least one give and one take card".to_owned();
                            continue;
                        }
                        if catan_core::gameplay::game::trade::trade_has_overlapping_resources(
                            &PlayerTrade { give, take },
                        ) {
                            self.message =
                                "same resource cannot appear on both trade sides".to_owned();
                            continue;
                        }
                        self.interactive_override = None;
                        return Ok(Some(PlayerTrade { give, take }));
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
                        self.message = "player trade cancelled".to_owned();
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
                    KeyCode::Tab | KeyCode::BackTab => {
                        editing_give = !editing_give;
                    }
                    KeyCode::Up => {
                        adjust_player_trade_selection(
                            &available,
                            &mut give,
                            &mut take,
                            resource,
                            editing_give,
                            1,
                        );
                    }
                    KeyCode::Down => {
                        adjust_player_trade_selection(
                            &available,
                            &mut give,
                            &mut take,
                            resource,
                            editing_give,
                            -1,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    fn select_trade_decision(
        &mut self,
        model: &UiModel,
        session: &UiTradeSession,
        selected_offer: Option<usize>,
        actions: &[&'static str],
    ) -> io::Result<Option<&'static str>> {
        if actions.is_empty() {
            return Ok(None);
        }

        let mut selected = 0;
        loop {
            self.interactive_override = Some(trade_decision_lines(
                session,
                selected_offer,
                actions,
                selected,
                model.private.as_ref(),
            ));
            self.draw(Some(model), "trade action: ", actions[selected])?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => return Ok(Some(actions[selected])),
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Left => {
                        selected = selected.checked_sub(1).unwrap_or(actions.len() - 1);
                    }
                    KeyCode::Right => {
                        selected = (selected + 1) % actions.len();
                    }
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
            self.interactive_override = Some(resource_choice_lines("resource", selected, None));
            let resource = Resource::ALL[selected];
            self.draw(Some(model), prompt, &format!("{resource:?}"))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        self.interactive_override = None;
                        return Ok(Some(resource));
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
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

    pub(crate) fn select_resource_pair(
        &mut self,
        model: &UiModel,
        prompt: &str,
        message: &str,
    ) -> io::Result<Option<[Resource; 2]>> {
        let mut selected = 0;
        let mut first = None;
        self.message = message.to_owned();
        loop {
            let title = if first.is_some() {
                "year of plenty: second"
            } else {
                "year of plenty: first"
            };
            self.interactive_override = Some(resource_choice_lines(title, selected, first));
            let resource = Resource::ALL[selected];
            self.draw(Some(model), prompt, &format!("{resource:?}"))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(first) = first {
                            self.interactive_override = None;
                            return Ok(Some([first, resource]));
                        }
                        first = Some(resource);
                        self.message = "first resource locked; select second resource".to_owned();
                    }
                    KeyCode::Esc => {
                        if first.take().is_some() {
                            self.message = "first resource cleared".to_owned();
                        } else {
                            self.interactive_override = None;
                            self.message = "resource selection cancelled".to_owned();
                            return Ok(None);
                        }
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
            self.interactive_override = Some(player_menu_lines(candidates, selected));
            self.draw(Some(model), prompt, &format!("p{}", candidates[selected]))?;
            if let CrosstermEvent::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        self.interactive_override = None;
                        return Ok(Some(candidates[selected]));
                    }
                    KeyCode::Esc => {
                        self.interactive_override = None;
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
        let interactive_override = self.interactive_override.clone();
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
                            interactive_override,
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

fn trade_offer_is_accepted(session: &UiTradeSession, offer_id: TradeOfferId) -> bool {
    session.responses.iter().flatten().any(|response| {
        matches!(
            response,
            TradeResponseState::Accepted { offer_id: accepted } if *accepted == offer_id
        )
    })
}

fn trade_response_actions_for_offer(
    private: Option<&UiPrivatePlayer>,
    offer: &UiTradeOffer,
) -> &'static [&'static str] {
    if can_accept_trade_offer(private, offer) {
        &["accept", "reject", "counter"]
    } else {
        &["reject", "counter"]
    }
}

fn can_accept_trade_offer(private: Option<&UiPrivatePlayer>, offer: &UiTradeOffer) -> bool {
    let Some(private) = private else {
        return false;
    };
    let required = if private.player_id == offer.proposer {
        &offer.trade.give
    } else {
        &offer.trade.take
    };
    private.resources.has_enough(required)
}

fn trade_browser_lines(
    session: &UiTradeSession,
    selected_offer: Option<usize>,
    new_offer_selected: Option<bool>,
    private: Option<&UiPrivatePlayer>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(selected) = new_offer_selected {
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "> " } else { "  " },
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("+", Style::default().fg(Color::Green)),
            Span::raw(" new trade"),
        ]));
    }
    lines.extend(trade_tree_lines_for_viewer(
        session,
        selected_offer,
        private.map(|private| private.player_id),
        private.map(|private| &private.resources),
    ));
    lines
}

fn trade_decision_lines(
    session: &UiTradeSession,
    selected_offer: Option<usize>,
    actions: &[&'static str],
    selected_action: usize,
    private: Option<&UiPrivatePlayer>,
) -> Vec<Line<'static>> {
    let mut lines = trade_browser_lines(session, selected_offer, None, private);
    lines.push(Line::from(""));
    let mut action_spans = Vec::new();
    for (idx, action) in actions.iter().enumerate() {
        if idx > 0 {
            action_spans.push(Span::raw("  "));
        }
        let style = if idx == selected_action {
            Style::default().fg(Color::Yellow)
        } else {
            subtle_panel_style()
        };
        action_spans.push(Span::styled(format!("[{action}]"), style));
    }
    lines.push(Line::from(action_spans));
    lines
}

fn adjust_player_trade_selection(
    available: &ResourceSet,
    give: &mut ResourceSet,
    take: &mut ResourceSet,
    resource: Resource,
    editing_give: bool,
    delta: i16,
) {
    let max = if editing_give {
        available[resource]
    } else {
        19
    };
    let current = if editing_give {
        give[resource]
    } else {
        take[resource]
    } as i16;
    let next = (current + delta).clamp(0, max as i16) as u16;
    if editing_give {
        give[resource] = next;
        if next > 0 {
            take[resource] = 0;
        }
    } else {
        take[resource] = next;
        if next > 0 {
            give[resource] = 0;
        }
    }
}

fn player_trade_builder_summary(
    give: &ResourceSet,
    take: &ResourceSet,
    selected_resource: usize,
    editing_give: bool,
) -> String {
    let side = if editing_give { "give" } else { "take" };
    let resource = Resource::ALL[selected_resource];
    format!(
        "{side} {resource:?}; give {} take {}",
        give.total(),
        take.total()
    )
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
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            let mut marker = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == ']' {
                    closed = true;
                    break;
                }
                marker.push(next);
            }
            if closed && let Some(marker) = parse_event_mini_card_marker(&marker) {
                push_event_token(&mut spans, &token);
                token.clear();
                match marker {
                    EventMiniCardMarker::Resource { count, resource } => {
                        MiniCardGlyph::new(count)
                            .face_style(resource_event_style_for(resource))
                            .bracket_style(resource_event_style_for(resource))
                            .push_to(&mut spans);
                    }
                    EventMiniCardMarker::Unknown => {
                        MiniCardGlyph::new("?")
                            .face_style(subtle_panel_style())
                            .bracket_style(subtle_panel_style())
                            .push_to(&mut spans);
                    }
                }
                continue;
            }
            push_event_token(&mut spans, &token);
            token.clear();
            spans.push(Span::raw(format!("[{marker}")));
            if closed {
                spans.push(Span::raw("]"));
            }
            continue;
        }
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

enum EventMiniCardMarker {
    Resource { count: String, resource: Resource },
    Unknown,
}

fn parse_event_mini_card_marker(marker: &str) -> Option<EventMiniCardMarker> {
    if marker == "?" {
        return Some(EventMiniCardMarker::Unknown);
    }
    let digit_len = marker
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;
    let (count, code) = marker.split_at(digit_len);
    let resource = match code {
        "B" => Resource::Brick,
        "W" => Resource::Wood,
        "H" => Resource::Wheat,
        "S" => Resource::Sheep,
        "O" => Resource::Ore,
        _ => return None,
    };
    Some(EventMiniCardMarker::Resource {
        count: count.to_owned(),
        resource,
    })
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
    Some(resource_event_style_for(resource))
}

fn resource_event_style_for(resource: Resource) -> Style {
    Style::default().fg(catan_render::adapters::ratatui::color(
        catan_render::field::FieldRenderer::resource_color(resource),
    ))
}

fn render_bank(frame: &mut Frame<'_>, area: Rect, model: &UiModel) {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let bank = Paragraph::new(bank_panel_lines(model, inner_width))
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

    let constraints = vec![Constraint::Length(4); model.public.players.len()];
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
        .max(layout.players.width)
        .max(layout.personal.width);
    let bottom = layout
        .journal
        .bottom()
        .max(layout.bank.bottom())
        .max(layout.players.bottom())
        .max(layout.personal.bottom());
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
    interactive_override: Option<Vec<Line<'static>>>,
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
    let field = Paragraph::new(center_field_lines(
        field_lines(model, overlay),
        layout.field,
    ))
    .block(panel_block("Field"));
    frame.render_widget(field, layout.field);

    let interactive_lines = state.interactive_override.unwrap_or_else(|| {
        trade_panel_lines(model, usize::from(layout.trade.width.saturating_sub(2)))
    });
    let interactive = Paragraph::new(interactive_lines)
        .wrap(Wrap { trim: false })
        .block(panel_block("Interactive"));
    frame.render_widget(interactive, layout.trade);

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

fn center_field_lines(lines: Vec<Line<'static>>, area: Rect) -> Vec<Line<'static>> {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let inner_height = usize::from(area.height.saturating_sub(2));
    let content_width = lines.iter().map(Line::width).max().unwrap_or(0);
    let left_pad = inner_width.saturating_sub(content_width) / 2;
    let top_pad = inner_height.saturating_sub(lines.len()) / 2;
    let skip_rows = lines.len().saturating_sub(inner_height) / 2;

    let mut centered = Vec::with_capacity(top_pad + lines.len());
    centered.extend((0..top_pad).map(|_| Line::from("")));
    centered.extend(
        lines
            .into_iter()
            .skip(skip_rows)
            .take(inner_height)
            .map(|mut line| {
                if left_pad > 0 {
                    let mut spans = vec![Span::raw(" ".repeat(left_pad))];
                    spans.extend(line.spans);
                    line.spans = spans;
                }
                line
            }),
    );
    centered
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
        let mut spans = vec![Span::raw(prompt.to_owned())];
        if input.is_empty() {
            spans.push(Span::styled(
                fit_command_text(
                    command_short_hints(view_mode),
                    inner_width.saturating_sub(prompt.len()),
                ),
                Style::default().fg(Color::Gray),
            ));
        } else {
            spans.push(Span::styled(
                fit_command_text(input, inner_width.saturating_sub(prompt.len())),
                Style::default().fg(Color::Yellow),
            ));
        }
        lines.push(Line::from(spans));
    }
    if show_help {
        lines.extend(command_help_lines(view_mode, inner_width));
    }
    lines
}

fn command_short_hints(view_mode: CliViewMode) -> &'static str {
    match view_mode {
        CliViewMode::Normal => "[r|e|bd|br|bs|bc|bt|pt|kn|yp|m|rb]",
        CliViewMode::Snapshot => "[s]",
    }
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
            "player-trade: pt or trade give take",
            "dev: kn, yp, m, rb or use knight|yop|monopoly|roadbuild ...",
            "discard: five resource counts or discard",
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
        log::trace!("Cleaning up CLI UI terminal");
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
    use catan_agents::remote_agent::{UiPrivatePlayer, UiTradeOffer};
    use catan_core::gameplay::{
        game::trade::TradeOfferId,
        primitives::{
            dev_card::DevCardData,
            player::PlayerId,
            resource::{Resource, ResourceSet},
            trade::PlayerTrade,
        },
    };
    use ratatui::style::{Color, Style};

    use super::{
        CardGlyph, CliViewMode, adjust_player_trade_selection, command_panel_lines,
        styled_event_text, trade_response_actions_for_offer,
    };

    #[test]
    fn command_panel_shows_short_hints_when_empty() {
        let rendered = command_panel_lines(CliViewMode::Normal, "command: ", "", false, 80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(rendered.lines().count(), 1);
        assert!(rendered.contains("command: [r|e|bd|br|bs|bc|bt|pt|kn|yp|m|rb]"));
    }

    #[test]
    fn command_panel_renders_input_to_right_of_prompt() {
        let rendered = command_panel_lines(CliViewMode::Normal, "command: ", "kn", false, 80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(rendered, "command: kn");
    }

    #[test]
    fn card_glyph_renders_three_lines_with_optional_indices() {
        let lines = CardGlyph::new("KN", Style::default().fg(Color::Magenta))
            .index_top(1, Style::default())
            .index_mid(2, Style::default())
            .lines()
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert_eq!(lines, vec!["┌──┐ 1", "│KN│ 2", "└──┘  "]);
    }

    #[test]
    fn journal_resource_markers_render_as_colored_mini_cards() {
        let line = styled_event_text("p0 got [4B][2S] and stole [?]");

        assert_eq!(line.to_string(), "p0 got [4][2] and stole [?]");
        let brick_color = catan_render::adapters::ratatui::color(
            catan_render::field::FieldRenderer::resource_color(Resource::Brick),
        );
        assert!(
            line.spans
                .iter()
                .any(|span| { span.content.as_ref() == "4" && span.style.fg == Some(brick_color) })
        );
    }

    #[test]
    fn player_trade_adjustment_caps_give_and_clears_opposite_side() {
        let available = ResourceSet {
            ore: 1,
            ..ResourceSet::EMPTY
        };
        let mut give = ResourceSet::EMPTY;
        let mut take = ResourceSet {
            ore: 2,
            ..ResourceSet::EMPTY
        };

        adjust_player_trade_selection(&available, &mut give, &mut take, Resource::Ore, true, 1);
        adjust_player_trade_selection(&available, &mut give, &mut take, Resource::Ore, true, 1);

        assert_eq!(give.ore, 1);
        assert_eq!(take.ore, 0);
    }

    #[test]
    fn trade_response_actions_hide_accept_when_viewer_cannot_pay() {
        let offer = UiTradeOffer {
            id: TradeOfferId(0),
            proposer: PlayerId::new(0),
            trade: PlayerTrade {
                give: ResourceSet::from(Resource::Brick),
                take: ResourceSet {
                    ore: 2,
                    ..ResourceSet::EMPTY
                },
            },
        };
        let private = UiPrivatePlayer {
            player_id: PlayerId::new(1),
            resources: ResourceSet {
                ore: 1,
                ..ResourceSet::EMPTY
            },
            dev_cards: DevCardData::default(),
        };

        assert_eq!(
            trade_response_actions_for_offer(Some(&private), &offer),
            &["reject", "counter"]
        );
    }

    #[test]
    fn trade_response_actions_include_accept_when_viewer_can_pay() {
        let offer = UiTradeOffer {
            id: TradeOfferId(0),
            proposer: PlayerId::new(0),
            trade: PlayerTrade {
                give: ResourceSet::from(Resource::Brick),
                take: ResourceSet {
                    ore: 2,
                    ..ResourceSet::EMPTY
                },
            },
        };
        let private = UiPrivatePlayer {
            player_id: PlayerId::new(1),
            resources: ResourceSet {
                ore: 2,
                ..ResourceSet::EMPTY
            },
            dev_cards: DevCardData::default(),
        };

        assert_eq!(
            trade_response_actions_for_offer(Some(&private), &offer),
            &["accept", "reject", "counter"]
        );
    }
}
