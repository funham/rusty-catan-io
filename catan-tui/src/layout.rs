//! Responsive layout helpers for the CLI TUI.

use ratatui::layout::Rect;

const NORMAL_RIGHT_MIN_WIDTH: u16 = 26;
const NORMAL_RIGHT_MAX_WIDTH: u16 = 64;
const SNAPSHOT_RIGHT_MIN_WIDTH: u16 = 32;
const SNAPSHOT_RIGHT_MAX_WIDTH: u16 = 42;
const END_STATS_MIN_WIDTH: u16 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalLayoutAreas {
    pub status: Rect,
    pub field: Rect,
    pub trade: Rect,
    pub personal: Rect,
    pub journal: Rect,
    pub bank: Rect,
    pub players: Rect,
    pub command: Rect,
}

pub fn normal_layout_areas(
    area: Rect,
    field_size: (u16, u16),
    command_line_count: usize,
) -> NormalLayoutAreas {
    let (status, body, command) = split_terminal(area, command_line_count);
    let (left, right) = split_body_columns(
        body,
        field_size.0.saturating_add(2),
        NORMAL_RIGHT_MIN_WIDTH,
        NORMAL_RIGHT_MAX_WIDTH,
    );
    let (field, trade) = split_left_column(left, field_size.1);
    let (journal, bank, players, personal) = split_info_column(right);

    NormalLayoutAreas {
        status,
        field,
        trade,
        personal,
        journal,
        bank,
        players,
        command,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotLayoutAreas {
    pub status: Rect,
    pub field: Rect,
    pub command: Rect,
    pub state: Rect,
}

pub fn snapshot_layout_areas(
    area: Rect,
    field_size: (u16, u16),
    command_line_count: usize,
) -> SnapshotLayoutAreas {
    let (status, body, command) = split_terminal(area, command_line_count);
    let field_target = field_size.0.saturating_add(2);
    let (field, state) = split_body_columns(
        body,
        field_target,
        SNAPSHOT_RIGHT_MIN_WIDTH,
        SNAPSHOT_RIGHT_MAX_WIDTH,
    );

    SnapshotLayoutAreas {
        status,
        field,
        command,
        state,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndLayoutAreas {
    pub status: Rect,
    pub field: Rect,
    pub result: Rect,
    pub rankings: Rect,
    pub dice: Rect,
    pub activity: Rect,
    pub command: Rect,
}

pub fn end_layout_areas(
    area: Rect,
    field_size: (u16, u16),
    command_line_count: usize,
) -> EndLayoutAreas {
    let (status, body, command) = split_terminal(area, command_line_count);
    if body.width == 0 || body.height == 0 {
        let empty = Rect::new(body.x, body.y, 0, 0);
        return EndLayoutAreas {
            status,
            field: empty,
            result: empty,
            rankings: empty,
            dice: empty,
            activity: empty,
            command,
        };
    }

    let desired_field_width = field_size.0.saturating_add(4);
    let max_field_width = body.width.saturating_sub(END_STATS_MIN_WIDTH).max(1);
    let field_col_width = desired_field_width.min(max_field_width).max(1);
    let stats_width = body.width.saturating_sub(field_col_width);
    let field_col = Rect::new(body.x, body.y, field_col_width, body.height);
    let stats_col = Rect::new(
        body.x.saturating_add(field_col_width),
        body.y,
        stats_width,
        body.height,
    );

    let field_width = field_col.width.min(field_size.0.saturating_add(2)).max(1);
    let field_height = field_col.height.min(field_size.1.saturating_add(2)).max(1);
    let field = Rect::new(
        field_col
            .x
            .saturating_add(field_col.width.saturating_sub(field_width) / 2),
        field_col
            .y
            .saturating_add(field_col.height.saturating_sub(field_height) / 2),
        field_width,
        field_height,
    );

    let result_height = stats_col.height.clamp(1, 5);
    let activity_height = if stats_col.height >= 22 { 8 } else { 6 }.min(stats_col.height);
    let remaining = stats_col
        .height
        .saturating_sub(result_height)
        .saturating_sub(activity_height);
    let rankings_height = (remaining.saturating_mul(55) / 100).max(1).min(remaining);
    let dice_height = remaining.saturating_sub(rankings_height);

    let result = Rect::new(stats_col.x, stats_col.y, stats_col.width, result_height);
    let rankings = Rect::new(
        stats_col.x,
        result.bottom(),
        stats_col.width,
        rankings_height,
    );
    let dice = Rect::new(stats_col.x, rankings.bottom(), stats_col.width, dice_height);
    let activity = Rect::new(
        stats_col.x,
        dice.bottom(),
        stats_col.width,
        stats_col.bottom().saturating_sub(dice.bottom()),
    );

    EndLayoutAreas {
        status,
        field,
        result,
        rankings,
        dice,
        activity,
        command,
    }
}

fn split_terminal(area: Rect, command_line_count: usize) -> (Rect, Rect, Rect) {
    let status_height = area.height.min(3);
    let remaining_height = area.height.saturating_sub(status_height);
    let command_height = command_height(command_line_count, remaining_height);
    let body_height = remaining_height.saturating_sub(command_height);

    let status = Rect::new(area.x, area.y, area.width, status_height);
    let body = Rect::new(
        area.x,
        area.y.saturating_add(status_height),
        area.width,
        body_height,
    );
    let command = Rect::new(
        area.x,
        area.y
            .saturating_add(status_height)
            .saturating_add(body_height),
        area.width,
        command_height,
    );

    (status, body, command)
}

fn command_height(line_count: usize, available_height: u16) -> u16 {
    if available_height == 0 {
        return 0;
    }

    let desired = (line_count as u16).saturating_add(2).clamp(3, 10);
    desired.min(available_height)
}

fn split_body_columns(
    body: Rect,
    preferred_left_width: u16,
    min_right_width: u16,
    max_right_width: u16,
) -> (Rect, Rect) {
    if body.width <= 1 {
        return (body, Rect::new(body.right(), body.y, 0, body.height));
    }

    let min_right_width = min_right_width.min(body.width.saturating_sub(1));
    let max_right_width = max_right_width.max(min_right_width).min(body.width);
    let right_width = body
        .width
        .saturating_sub(preferred_left_width)
        .clamp(min_right_width, max_right_width)
        .min(body.width.saturating_sub(1));
    let left_width = body.width.saturating_sub(right_width).max(1);
    let right_width = body.width.saturating_sub(left_width);

    (
        Rect::new(body.x, body.y, left_width, body.height),
        Rect::new(
            body.x.saturating_add(left_width),
            body.y,
            right_width,
            body.height,
        ),
    )
}

fn split_left_column(left: Rect, preferred_field_height: u16) -> (Rect, Rect) {
    if left.height <= 1 {
        let empty = Rect::new(left.x, left.bottom(), left.width, 0);
        return (left, empty);
    }

    let trade_height = if left.height >= 18 {
        9
    } else if left.height >= 14 {
        5
    } else {
        0
    };
    let max_field_height = left.height.saturating_sub(trade_height).max(1);
    let field_height = preferred_field_height.min(max_field_height).max(1);
    let trade_height = left.height.saturating_sub(field_height);

    (
        Rect::new(left.x, left.y, left.width, field_height),
        Rect::new(
            left.x,
            left.y.saturating_add(field_height),
            left.width,
            trade_height,
        ),
    )
}

fn split_info_column(right: Rect) -> (Rect, Rect, Rect, Rect) {
    if right.height == 0 || right.width == 0 {
        let empty = Rect::new(right.x, right.y, right.width, 0);
        return (empty, empty, empty, empty);
    }

    let bank_height = right.height.clamp(1, 5);
    let personal_height = if right.height >= 20 { 7 } else { 4 }.min(right.height);
    let remaining_after_fixed = right
        .height
        .saturating_sub(bank_height)
        .saturating_sub(personal_height);
    let journal_height = if remaining_after_fixed == 0 {
        0
    } else {
        (right.height.saturating_mul(40) / 100)
            .clamp(1, remaining_after_fixed)
            .min(remaining_after_fixed)
    };
    let players_height = remaining_after_fixed.saturating_sub(journal_height);

    let journal = Rect::new(right.x, right.y, right.width, journal_height);
    let bank = Rect::new(
        right.x,
        right.y.saturating_add(journal_height),
        right.width,
        bank_height,
    );
    let players = Rect::new(
        right.x,
        right
            .y
            .saturating_add(journal_height)
            .saturating_add(bank_height),
        right.width,
        players_height,
    );
    let personal = Rect::new(
        right.x,
        right
            .y
            .saturating_add(journal_height)
            .saturating_add(bank_height)
            .saturating_add(players_height),
        right.width,
        personal_height,
    );

    (journal, bank, players, personal)
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{end_layout_areas, normal_layout_areas, snapshot_layout_areas};

    #[test]
    fn normal_layout_uses_available_area_without_overflowing() {
        let area = Rect::new(0, 0, 100, 34);
        let layout = normal_layout_areas(area, (48, 24), 2);

        for pane in [
            layout.status,
            layout.field,
            layout.trade,
            layout.personal,
            layout.journal,
            layout.bank,
            layout.players,
            layout.command,
        ] {
            assert!(pane.x >= area.x);
            assert!(pane.y >= area.y);
            assert!(pane.right() <= area.right());
            assert!(pane.bottom() <= area.bottom());
        }

        assert!(layout.field.width > 0);
        assert!(layout.trade.height >= 9);
        assert!(layout.journal.width > 0);
        assert!(layout.personal.y >= layout.players.bottom());
        assert!(layout.command.height >= 3);
    }

    #[test]
    fn normal_layout_allows_command_panel_to_grow_for_help() {
        let area = Rect::new(0, 0, 100, 34);
        let compact = normal_layout_areas(area, (48, 24), 2);
        let help = normal_layout_areas(area, (48, 24), 8);

        assert!(help.command.height > compact.command.height);
        assert!(help.command.bottom() <= area.bottom());
    }

    #[test]
    fn normal_layout_places_private_below_players_in_right_column() {
        let area = Rect::new(0, 0, 120, 36);
        let layout = normal_layout_areas(area, (48, 24), 2);

        assert_eq!(layout.personal.x, layout.players.x);
        assert_eq!(layout.personal.width, layout.players.width);
        assert!(layout.personal.y >= layout.players.bottom());
        assert!(layout.personal.right() <= area.right());
    }

    #[test]
    fn normal_layout_allocates_full_private_panel_width_on_wide_terminals() {
        let area = Rect::new(0, 0, 120, 36);
        let layout = normal_layout_areas(area, (48, 24), 2);

        assert_eq!(layout.personal.width, 64);
        assert!(layout.field.width >= 50);
    }

    #[test]
    fn normal_layout_keeps_right_column_bounded_on_wide_terminals() {
        let area = Rect::new(0, 0, 140, 36);
        let layout = normal_layout_areas(area, (48, 24), 2);

        assert!(layout.players.width <= 64);
        assert!(layout.bank.width <= 64);
        assert!(layout.journal.width <= 64);
        assert!(layout.personal.width <= 64);
    }

    #[test]
    fn normal_layout_reduces_field_panel_height_from_canvas_plus_border() {
        let area = Rect::new(0, 0, 120, 36);
        let layout = normal_layout_areas(area, (48, 24), 2);

        assert!(layout.field.height <= 24);
    }

    #[test]
    fn normal_layout_survives_small_terminals() {
        let area = Rect::new(0, 0, 54, 18);
        let layout = normal_layout_areas(area, (48, 24), 8);

        assert!(layout.field.width > 0);
        assert!(layout.command.height > 0);
        assert!(layout.command.bottom() <= area.bottom());
        assert!(layout.players.right() <= area.right());
    }

    #[test]
    fn snapshot_layout_uses_available_area_without_overflowing() {
        let area = Rect::new(0, 0, 90, 26);
        let layout = snapshot_layout_areas(area, (48, 24), 2);

        for pane in [layout.status, layout.field, layout.command, layout.state] {
            assert!(pane.x >= area.x);
            assert!(pane.y >= area.y);
            assert!(pane.right() <= area.right());
            assert!(pane.bottom() <= area.bottom());
        }

        assert!(layout.field.width > 0);
        assert!(layout.state.width > 0);
    }

    #[test]
    fn end_layout_keeps_only_final_screen_panels_without_overflowing() {
        let area = Rect::new(0, 0, 120, 36);
        let layout = end_layout_areas(area, (48, 24), 2);

        for pane in [
            layout.status,
            layout.field,
            layout.result,
            layout.rankings,
            layout.dice,
            layout.activity,
            layout.command,
        ] {
            assert!(pane.x >= area.x);
            assert!(pane.y >= area.y);
            assert!(pane.right() <= area.right());
            assert!(pane.bottom() <= area.bottom());
        }

        assert!(layout.field.width <= 50);
        assert!(layout.rankings.x >= layout.field.right());
        assert_eq!(layout.command.bottom(), area.bottom());
    }
}
