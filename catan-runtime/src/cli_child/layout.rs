//! Responsive layout helpers for the CLI TUI.

use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NormalLayoutAreas {
    pub status: Rect,
    pub field: Rect,
    pub personal: Rect,
    pub journal: Rect,
    pub bank: Rect,
    pub players: Rect,
    pub command: Rect,
}

pub(crate) fn normal_layout_areas(
    area: Rect,
    field_size: (u16, u16),
    command_line_count: usize,
) -> NormalLayoutAreas {
    let (status, body, command) = split_terminal(area, command_line_count);
    let (left, right) = split_body_columns(body, field_size.0.saturating_add(2), 34);
    let (field, personal) = split_left_column(left, field_size.1.saturating_add(2));
    let (journal, bank, players) = split_info_column(right);

    NormalLayoutAreas {
        status,
        field,
        personal,
        journal,
        bank,
        players,
        command,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SnapshotLayoutAreas {
    pub status: Rect,
    pub field: Rect,
    pub command: Rect,
    pub state: Rect,
}

pub(crate) fn snapshot_layout_areas(
    area: Rect,
    field_size: (u16, u16),
    command_line_count: usize,
) -> SnapshotLayoutAreas {
    let (status, body, command) = split_terminal(area, command_line_count);
    let field_target = field_size.0.saturating_add(2);
    let (field, state) = split_body_columns(body, field_target, 38);

    SnapshotLayoutAreas {
        status,
        field,
        command,
        state,
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

fn split_body_columns(body: Rect, preferred_left_width: u16, min_right_width: u16) -> (Rect, Rect) {
    if body.width <= 1 {
        return (body, Rect::new(body.right(), body.y, 0, body.height));
    }

    let min_right_width = min_right_width.min(body.width.saturating_sub(1));
    let max_left_width = body.width.saturating_sub(min_right_width);
    let balanced_left_width = body.width.saturating_mul(55) / 100;
    let left_width = preferred_left_width
        .min(max_left_width)
        .max(balanced_left_width.min(max_left_width))
        .max(1)
        .min(body.width);
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
        return (left, Rect::new(left.x, left.bottom(), left.width, 0));
    }

    let personal_min = if left.height >= 8 { 5 } else { 1 };
    let max_field_height = left.height.saturating_sub(personal_min).max(1);
    let field_height = preferred_field_height.min(max_field_height).max(1);
    let personal_height = left.height.saturating_sub(field_height);

    (
        Rect::new(left.x, left.y, left.width, field_height),
        Rect::new(
            left.x,
            left.y.saturating_add(field_height),
            left.width,
            personal_height,
        ),
    )
}

fn split_info_column(right: Rect) -> (Rect, Rect, Rect) {
    if right.height == 0 || right.width == 0 {
        let empty = Rect::new(right.x, right.y, right.width, 0);
        return (empty, empty, empty);
    }

    let bank_height = right.height.clamp(1, 5);
    let remaining = right.height.saturating_sub(bank_height);
    let journal_height = if remaining == 0 {
        0
    } else {
        (right.height.saturating_mul(40) / 100)
            .clamp(1, remaining)
            .min(remaining)
    };
    let players_height = remaining.saturating_sub(journal_height);

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

    (journal, bank, players)
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{normal_layout_areas, snapshot_layout_areas};

    #[test]
    fn normal_layout_uses_available_area_without_overflowing() {
        let area = Rect::new(0, 0, 100, 34);
        let layout = normal_layout_areas(area, (48, 24), 2);

        for pane in [
            layout.status,
            layout.field,
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
        assert!(layout.journal.width > 0);
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
}
