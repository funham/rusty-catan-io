use std::collections::BTreeSet;

use catan_core::{
    gameplay::{
        game::view::PublicGameView,
        primitives::{
            Tile,
            build::{Establishment, EstablishmentType, Road},
            player::PlayerId,
            resource::Resource,
        },
    },
    topology::{Axis, Hex, HexIndex, Intersection, Path, SignedAxis},
};

use crate::{
    canvas::Canvas,
    cursor::CursorPosition,
    model::RenderGameView,
    style::{RenderColor, RenderStyle},
};

mod utils {
    use super::*;

    pub fn axis_path_cells(axis: Axis) -> &'static [(i32, i32, char)] {
        match axis {
            Axis::Q => &[
                (0, 0, '_'),
                (1, 0, '_'),
                (2, 0, '_'),
                (3, 0, '_'),
                (4, 0, '_'),
                (5, 0, '_'),
                (6, 0, '_'),
            ],
            Axis::R => &[(1, 0, '/'), (0, 1, '/')],
            Axis::S => &[(0, 0, '\\'), (1, 1, '\\')],
        }
    }

    pub fn path_anchor(path: Path) -> CursorPosition {
        match path.axis() {
            Axis::Q => {
                let (h1, h2) = path.as_pair();
                let (x, y0) = hex_anchor_flat(h1.q, h1.r);
                let (_, y1) = hex_anchor_flat(h2.q, h2.r);
                CursorPosition {
                    x: x + 2,
                    y: y0.max(y1),
                }
            }
            Axis::R => {
                let (h1, h2) = path.as_pair();
                let (x0, y0) = hex_anchor_flat(h1.q, h1.r);
                let (x1, y1) = hex_anchor_flat(h2.q, h2.r);
                let (y, x) = (y0, x0).max((y1, x1));
                CursorPosition { x, y: y + 1 }
            }
            Axis::S => {
                let (h1, h2) = path.as_pair();
                let (x0, y0) = hex_anchor_flat(h1.q, h1.r);
                let (x1, y1) = hex_anchor_flat(h2.q, h2.r);
                let (y, x) = (y0, x0).min((y1, x1));
                CursorPosition { x, y: y + 3 }
            }
        }
    }

    pub fn intersection_anchor(v: Intersection) -> CursorPosition {
        let hexes = v.as_set();
        let nether_hex = hexes
            .iter()
            .max_by_key(|h| hex_anchor(**h).y)
            .expect("intersection has at least one hex");

        let top_path = nether_hex.paths_arr()[SignedAxis::QP.dir_index()];
        let right = nether_hex.neighbors()[SignedAxis::SP.dir_index()];
        let mut anchor = path_anchor(top_path);

        if hexes.contains(&right) {
            anchor.x += 7;
        } else {
            anchor.x -= 1;
        }

        anchor
    }

    pub const fn hex_anchor(hex: Hex) -> CursorPosition {
        let (x, y) = hex_anchor_flat(hex.q, hex.r);
        CursorPosition { x, y }
    }

    pub const fn hex_anchor_flat(q: i32, r: i32) -> (i32, i32) {
        let (q, r) = (q + 3, r + 1);
        (q * 9 + 1, q * 2 + r * 4)
    }

    pub fn centered(pos: CursorPosition, text: &str) -> CursorPosition {
        CursorPosition {
            x: pos.x - (text.len() / 2) as i32,
            y: pos.y,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FieldSelection {
    Hex(Hex),
    Path(Path),
    Intersection(Intersection),
}

#[derive(Debug, Clone, Default)]
pub struct FieldOverlay {
    pub selected: Option<FieldSelection>,
    pub status: SelectionStatus,
    pub preview: Vec<FieldPreview>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SelectionStatus {
    #[default]
    Neutral,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldPreview {
    Establishment {
        player_id: PlayerId,
        establishment: Establishment,
    },
    Road {
        player_id: PlayerId,
        road: Road,
    },
}

pub enum PathAttr {
    Road { color: RenderColor },
    Selector,
}

pub enum HexAttr {
    TileNum(u8),
    Index,
    Resource(Resource),
    Robber,
    Selector,
}

pub enum IntersectionAttr {
    Selector,
    Establishment(EstablishmentType, RenderColor),
}

pub struct FieldRenderer {
    canvas: Canvas,
}

impl Default for FieldRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldRenderer {
    pub fn new() -> Self {
        const FIELD_HEX_WIDTH: usize = 5 + 2;
        const HEX_WIDTH_BODY: usize = 9;
        const HEX_WIDTH_SIDE: usize = 1;
        const FIELD_HEX_HEIGHT: usize = 5 + 2;
        const HEX_HEIGHT: usize = 5;

        let width = FIELD_HEX_WIDTH * HEX_WIDTH_BODY + 2 * HEX_WIDTH_SIDE;
        let height = (HEX_HEIGHT - 1) * FIELD_HEX_HEIGHT + 1;

        Self {
            canvas: Canvas::new(width, height),
        }
    }

    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub fn plain_lines(&self) -> Vec<String> {
        self.canvas.plain_lines()
    }

    pub fn clear(&mut self) {
        self.canvas.clear();
    }

    pub fn draw_context(&mut self, view: &PublicGameView<'_>) {
        self.draw_game(&RenderGameView::from(view));
    }

    pub fn render(&self) {
        let mut stdout = termcolor::StandardStream::stdout(termcolor::ColorChoice::Always);
        crate::adapters::termcolor::write_canvas_with_rulers(&self.canvas, &mut stdout)
            .expect("failed to render field to terminal");
    }

    pub fn draw_game(&mut self, view: &RenderGameView) {
        self.clear();
        self.draw_field(RenderStyle::default().dim());

        for (index, tile) in view.board.tiles.iter().copied().enumerate() {
            let hex = HexIndex::spiral_to_hex(index);
            match tile {
                Tile::Resource { resource, number } => {
                    self.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                    self.draw_hex_attr(hex, HexAttr::Resource(resource));
                }
                Tile::Desert => {}
                Tile::River { number } => self.draw_hex_attr(hex, HexAttr::TileNum(number.into())),
            }
        }

        for index in 0..HexIndex::spiral_start_of_ring(view.board.field_radius as usize + 2) {
            self.draw_hex_attr(HexIndex::spiral_to_hex(index), HexAttr::Index);
        }

        self.draw_hex_attr(view.board_state.robber_pos, HexAttr::Robber);

        for player in &view.builds {
            for road in &player.roads {
                self.draw_path_attr(
                    road.pos,
                    PathAttr::Road {
                        color: Self::player_color(player.player_id),
                    },
                );
            }
            for est in &player.establishments {
                self.draw_intersection_attr(
                    est.pos,
                    IntersectionAttr::Establishment(
                        est.stage,
                        Self::player_color(player.player_id),
                    ),
                );
            }
        }
    }

    pub fn draw_overlay(&mut self, overlay: &FieldOverlay) {
        for preview in &overlay.preview {
            match *preview {
                FieldPreview::Establishment {
                    player_id,
                    establishment,
                } => self.draw_intersection_attr(
                    establishment.pos,
                    IntersectionAttr::Establishment(
                        establishment.stage,
                        Self::player_color(player_id),
                    ),
                ),
                FieldPreview::Road { player_id, road } => self.draw_path_attr(
                    road.pos,
                    PathAttr::Road {
                        color: Self::player_color(player_id),
                    },
                ),
            }
        }

        match overlay.selected {
            Some(FieldSelection::Hex(hex)) => self.draw_hex_selector(
                hex,
                Self::selector_color(overlay.status, RenderColor::Ansi256(250)),
            ),
            Some(FieldSelection::Path(path)) => self.draw_path(
                path,
                RenderStyle::default()
                    .bg(Self::selector_color(overlay.status, RenderColor::Blue))
                    .bold(),
            ),
            Some(FieldSelection::Intersection(intersection)) => self.draw_intersection_selector(
                intersection,
                Self::selector_color(overlay.status, RenderColor::Green),
            ),
            None => {}
        }
    }

    fn selector_color(status: SelectionStatus, available_color: RenderColor) -> RenderColor {
        match status {
            SelectionStatus::Neutral => RenderColor::Ansi256(250),
            SelectionStatus::Available => available_color,
            SelectionStatus::Unavailable => RenderColor::Red,
        }
    }

    pub fn player_color(player_id: PlayerId) -> RenderColor {
        const COLORS: [RenderColor; 4] = [
            RenderColor::Blue,
            RenderColor::White,
            RenderColor::Red,
            RenderColor::Ansi256(172),
        ];
        COLORS[player_id % COLORS.len()]
    }

    pub fn resource_color(res: Resource) -> RenderColor {
        match res {
            Resource::Brick => RenderColor::Red,
            Resource::Wood => RenderColor::Ansi256(28),
            Resource::Wheat => RenderColor::Yellow,
            Resource::Sheep => RenderColor::Green,
            Resource::Ore => RenderColor::Cyan,
        }
    }

    pub fn draw_intersection_attr(&mut self, intersection: Intersection, attr: IntersectionAttr) {
        match attr {
            IntersectionAttr::Selector => {
                self.draw_intersection_selector(intersection, RenderColor::Red)
            }
            IntersectionAttr::Establishment(kind, color) => {
                self.draw_intersection_establishment(intersection, kind, color)
            }
        }
    }

    pub fn draw_hex_attr(&mut self, hex: Hex, attr: HexAttr) {
        match attr {
            HexAttr::TileNum(num) => self.draw_hex_tile_num(hex, num),
            HexAttr::Index => self.draw_hex_index(hex),
            HexAttr::Resource(res) => self.draw_hex_resource(hex, res),
            HexAttr::Robber => self.draw_hex_robber(hex),
            HexAttr::Selector => self.draw_hex_selector(hex, RenderColor::Red),
        }
    }

    pub fn draw_path_attr(&mut self, path: Path, attr: PathAttr) {
        match attr {
            PathAttr::Road { color } => {
                self.draw_path(path, RenderStyle::default().fg(color).bold())
            }
            PathAttr::Selector => {
                self.draw_path(path, RenderStyle::default().bg(RenderColor::Red).bold())
            }
        }
    }

    fn draw_field(&mut self, style: RenderStyle) {
        let paths = (0..=2)
            .flat_map(|radius| HexIndex::hex_ring(Hex::new(0, 0), radius))
            .flat_map(|h| h.paths_arr())
            .collect::<BTreeSet<_>>();

        for path in paths {
            self.draw_path(path, style);
        }
    }

    fn draw_path(&mut self, path: Path, style: RenderStyle) {
        let pos = utils::path_anchor(path);
        for &(dx, dy, ch) in utils::axis_path_cells(path.axis()) {
            self.canvas.set(
                pos + CursorPosition::new(dx, dy),
                crate::canvas::Cell { ch, style },
            );
        }
    }

    fn draw_hex_tile_num(&mut self, hex: Hex, num: u8) {
        let style = match num {
            6 | 8 => RenderStyle::default().fg(RenderColor::Red),
            _ => RenderStyle::default().fg(RenderColor::Ansi256(188)),
        };
        self.draw_hex_text(hex, 1, &num.to_string(), style, false);
    }

    fn draw_hex_robber(&mut self, hex: Hex) {
        self.draw_hex_text(
            hex,
            3,
            "ROB",
            RenderStyle::default().bg(RenderColor::Ansi256(244)),
            false,
        );
    }

    fn draw_hex_selector(&mut self, hex: Hex, color: RenderColor) {
        self.draw_hex_text(hex, 3, "[ ]", RenderStyle::default().bg(color).bold(), true);
    }

    fn draw_hex_index(&mut self, hex: Hex) {
        self.draw_hex_text(
            hex,
            3,
            &hex.index().to_spiral().to_string(),
            RenderStyle::default().dim(),
            false,
        );
    }

    fn draw_hex_resource(&mut self, hex: Hex, res: Resource) {
        let label: &'static str = res.into();
        self.draw_hex_text(
            hex,
            2,
            label,
            RenderStyle::default().fg(Self::resource_color(res)),
            false,
        );
    }

    fn draw_hex_text(
        &mut self,
        hex: Hex,
        line: i32,
        text: &str,
        style: RenderStyle,
        with_blank: bool,
    ) {
        let pos = utils::hex_anchor(hex) + CursorPosition::new(5, line);
        let pos = utils::centered(pos, text);
        if with_blank {
            self.canvas.write_text_with_blank(pos, text, style);
        } else {
            self.canvas.write_text(pos, text, style);
        }
    }

    fn draw_intersection_selector(&mut self, v: Intersection, color: RenderColor) {
        let pos = utils::centered(utils::intersection_anchor(v), "[ ]");
        self.canvas
            .write_text_with_blank(pos, "[ ]", RenderStyle::default().bg(color).bold());
    }

    fn draw_intersection_establishment(
        &mut self,
        v: Intersection,
        kind: EstablishmentType,
        color: RenderColor,
    ) {
        let text = match kind {
            EstablishmentType::Settlement => "(S)",
            EstablishmentType::City => "[C]",
        };
        let pos = utils::centered(utils::intersection_anchor(v), text);
        self.canvas
            .write_text_with_blank(pos, text, RenderStyle::default().fg(color).bold());
    }
}

impl From<Road> for FieldSelection {
    fn from(value: Road) -> Self {
        Self::Path(value.pos)
    }
}

#[cfg(test)]
mod tests {
    use catan_core::gameplay::game::init::GameInitializationState;

    use super::*;
    use crate::model::RenderBoard;

    #[test]
    fn renders_default_game_field() {
        let init = GameInitializationState::default();
        let view = RenderGameView {
            board: RenderBoard::from_board(&init.board),
            board_state: init.board_state,
            builds: Vec::new(),
        };

        let mut renderer = FieldRenderer::new();
        renderer.draw_game(&view);

        let lines = renderer.plain_lines();
        assert!(lines.iter().any(|line| line.contains("ROB")));
        assert!(lines.iter().any(|line| line.contains("Brick")));
    }

    #[test]
    fn hex_overlay_marks_selected_cells() {
        let mut renderer = FieldRenderer::new();
        renderer.draw_overlay(&FieldOverlay {
            selected: Some(FieldSelection::Hex(Hex::new(0, 0))),
            status: SelectionStatus::Unavailable,
            preview: Vec::new(),
        });

        assert!(
            renderer
                .plain_lines()
                .iter()
                .any(|line| line.contains("[ ]"))
        );
        assert!(renderer.canvas().cells().iter().any(|cell| {
            cell.ch == '[' && cell.style.bg == Some(RenderColor::Red) && cell.style.bold
        }));
    }
}
