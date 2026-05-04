//! Rendering adapters for the terminal field view.
//!
//! Converts remote-agent `UiModel` snapshots into `catan-render` view models and then
//! into ratatui lines, including field overlays used by interactive selectors.

use catan_agents::remote_agent::UiModel;
use catan_render::{
    adapters::ratatui::canvas_lines,
    field::{FieldOverlay, FieldRenderer},
    model::{RenderBoard, RenderGameView, RenderPlayerBuilds},
};
use ratatui::text::Line;

pub(crate) fn field_lines(model: &UiModel, overlay: &FieldOverlay) -> Vec<Line<'static>> {
    let mut renderer = FieldRenderer::new();
    renderer.draw_game(&render_game_view(model));
    renderer.draw_overlay(overlay);
    canvas_lines(renderer.canvas())
}

pub(crate) fn field_lines_cropped_left(
    model: &UiModel,
    overlay: &FieldOverlay,
    cols: usize,
) -> Vec<Line<'static>> {
    field_lines(model, overlay)
        .into_iter()
        .map(|line| crop_line_left(line, cols))
        .collect()
}

pub(crate) fn field_size() -> (u16, u16) {
    let renderer = FieldRenderer::new();
    (
        renderer.canvas().width() as u16,
        renderer.canvas().height() as u16,
    )
}

fn crop_line_left(line: Line<'static>, mut cols: usize) -> Line<'static> {
    if cols == 0 {
        return line;
    }

    let mut spans = Vec::new();
    for span in line.spans {
        if cols == 0 {
            spans.push(span);
            continue;
        }

        let content = span.content.to_string();
        let content_len = content.chars().count();
        if cols >= content_len {
            cols -= content_len;
            continue;
        }

        let cropped = content.chars().skip(cols).collect::<String>();
        cols = 0;
        spans.push(ratatui::text::Span::styled(cropped, span.style));
    }
    Line::from(spans)
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
