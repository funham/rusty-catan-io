//! Rendering adapters for the terminal field view.
//!
//! Converts game projections into ratatui lines, including field overlays used by
//! interactive selectors.

use catan_core::gameplay::game::projection::GameProjection;
use ratatui::text::Line;

use crate::{
    field::{FieldOverlay, FieldRenderer},
    ratatui_adapter::canvas_lines,
};

pub fn field_lines(model: &GameProjection, overlay: &FieldOverlay) -> Vec<Line<'static>> {
    let mut renderer = FieldRenderer::new();
    renderer.draw_game(&model.public);
    renderer.draw_overlay(overlay);
    canvas_lines(renderer.canvas())
}

pub fn field_lines_cropped_left(
    model: &GameProjection,
    overlay: &FieldOverlay,
    cols: usize,
) -> Vec<Line<'static>> {
    field_lines(model, overlay)
        .into_iter()
        .map(|line| crop_line_left(line, cols))
        .collect()
}

pub fn field_size() -> (u16, u16) {
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
