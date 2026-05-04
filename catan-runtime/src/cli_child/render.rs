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

pub(crate) fn field_size() -> (u16, u16) {
    let renderer = FieldRenderer::new();
    (
        renderer.canvas().width() as u16,
        renderer.canvas().height() as u16,
    )
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
