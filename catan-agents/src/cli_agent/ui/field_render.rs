use catan_core::{
    gameplay::{
        game::view::PublicGameView,
        primitives::{Tile, build::EstablishmentType, player::PlayerId, resource::Resource},
    },
    topology::{Hex, HexIndex, Intersection, Path},
};
use std::{collections::BTreeSet, io::Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    cli_agent::ui::buffer::{BufFragment, Buffer},
    remote_agent::UiPublicGame,
};

mod utils {
    use catan_core::topology::{Axis, Hex, Intersection, Path, SignedAxis};
    use termcolor::ColorSpec;

    use crate::cli_agent::ui::{
        buffer::{BufFragment, Buffer},
        cursor::CursorPosition,
    };

    pub fn axis_path_buf(axis: Axis) -> Buffer {
        match axis {
            Axis::Q => Buffer::from_string(7, 1, r"_______".as_bytes()),
            Axis::R => Buffer::from_string(2, 2, r" // ".as_bytes()),
            Axis::S => Buffer::from_string(2, 2, r"\  \".as_bytes()),
        }
    }

    pub fn path_anchor(path: Path) -> CursorPosition {
        match path.axis() {
            Axis::Q => {
                let (h1, h2) = path.as_pair();
                let (x, y0) = hex_anchor_flat(h1.q, h1.r);
                let (_, y1) = hex_anchor_flat(h2.q, h2.r);
                let y = y0.max(y1);

                CursorPosition { x: x + 2, y }
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
            .expect("can't be empty lol");

        let top_path = nether_hex.paths_arr()[SignedAxis::QP.dir_index()];

        let right = nether_hex.neighbors()[SignedAxis::SP.dir_index()];
        let _ /* left */ = nether_hex.neighbors()[SignedAxis::SN.dir_index()];
        let mut anchor = path_anchor(top_path);

        if hexes.contains(&right) {
            anchor.x += 7;
        } else {
            anchor.x -= 1;
        }

        anchor
    }

    pub fn path_buf(path: Path) -> BufFragment {
        BufFragment {
            fragment: axis_path_buf(path.axis()),
            pos: path_anchor(path),
        }
    }

    pub fn path_buf_fmt(path: Path, fmt: ColorSpec) -> (BufFragment, BufFragment<ColorSpec>) {
        let buf = path_buf(path);
        let fmt = BufFragment::<ColorSpec> {
            fragment: buf.fragment.format(fmt),
            pos: buf.pos,
        };

        (buf, fmt)
    }

    pub fn textbox_buf_centered(pos: CursorPosition, s: &[u8]) -> BufFragment {
        let tb_pos = CursorPosition {
            x: pos.x - (s.len() / 2) as i32,
            y: pos.y,
        };

        BufFragment {
            fragment: Buffer::from_string(s.len(), 1, s),
            pos: tb_pos,
        }
    }

    pub fn textbox_buf_centered_fmt(
        pos: CursorPosition,
        s: &[u8],
        fmt: ColorSpec,
    ) -> (BufFragment, BufFragment<ColorSpec>) {
        let buf = textbox_buf_centered(pos, s);
        let fmt = buf.fragment.format_full(fmt);
        let fmt = BufFragment::<ColorSpec> {
            fragment: fmt,
            pos: buf.pos,
        };

        (buf, fmt)
    }

    pub fn hex_textbox_fmt(
        hex: Hex,
        line: i32,
        s: &[u8],
        fmt: ColorSpec,
    ) -> (BufFragment, BufFragment<ColorSpec>) {
        let pos = hex_anchor(hex) + CursorPosition { x: 5, y: line };
        textbox_buf_centered_fmt(pos, s, fmt)
    }

    pub const fn hex_anchor(hex: Hex) -> CursorPosition {
        let (x, y) = hex_anchor_flat(hex.q, hex.r);
        CursorPosition { x, y }
    }

    pub const fn hex_anchor_flat(q: i32, r: i32) -> (i32, i32) {
        // supposing (-2, -1) -> (0, 0)
        //
        // (+1, +0) -> (+9, +2)  (y-axis inverted)
        // (+0, +1) -> (+0, +4)

        let (q, r) = (q + 3, r + 1);
        const X_PIXEL_OFFSET: i32 = 1;

        (q * 9 + X_PIXEL_OFFSET, q * 2 + r * 4)
    }
}

pub enum PathAttr {
    Road { color: Color },
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
    Establishment(EstablishmentType, Color),
}

pub struct FieldRenderer {
    width: usize,
    height: usize,
    buf: Buffer<u8>,
    fmt: Buffer<ColorSpec>,
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
            width,
            height,
            buf: Buffer::new(width, height),
            fmt: Buffer::new(width, height),
        }
    }

    pub fn player_color(player_id: PlayerId) -> Color {
        const PLAYER_COLORS: [Color; 4] =
            [Color::Blue, Color::White, Color::Red, Color::Ansi256(172)];

        PLAYER_COLORS[player_id]
    }

    pub fn resource_color(res: Resource) -> Color {
        match res {
            Resource::Brick => Color::Red,
            Resource::Wood => Color::Ansi256(28),
            Resource::Wheat => Color::Ansi256(11),
            Resource::Sheep => Color::Ansi256(10),
            Resource::Ore => Color::Cyan,
        }
    }

    /* main functions */

    pub fn clear(&mut self) {
        self.buf.clear();
        self.fmt.clear();
    }

    pub fn draw_skeleton(&mut self, _view: &PublicGameView<'_>) -> &mut Self {
        // TODO: ports
        self.draw_field(ColorSpec::new().set_dimmed(true).clone());
        self
    }

    pub fn draw_tile_info(&mut self, view: &PublicGameView<'_>) -> &mut Self {
        for (hex, tile) in view.board.arrangement.hex_enum_iter() {
            match tile {
                Tile::Resource { resource, number } => {
                    self.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                    self.draw_hex_attr(hex, HexAttr::Resource(resource));
                }
                Tile::Desert => {}
                Tile::River { number } => {
                    self.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                }
            }
        }
        self
    }

    pub fn draw_robber(&mut self, view: &PublicGameView<'_>) -> &mut Self {
        self.draw_hex_attr(view.board_state.robber_pos, HexAttr::Robber);
        self
    }

    pub fn draw_builds(&mut self, view: &PublicGameView<'_>) -> &mut Self {
        for (id, player) in view.builds.players().iter().enumerate() {
            for road in player.roads.iter() {
                self.draw_path_attr(
                    road.pos,
                    PathAttr::Road {
                        color: Self::player_color(id),
                    },
                );
            }

            for est in player.establishments.iter() {
                self.draw_intersection_attr(
                    est.pos,
                    IntersectionAttr::Establishment(est.stage, Self::player_color(id)),
                );
            }
        }

        self
    }

    pub fn draw_index(&mut self, view: &PublicGameView<'_>) -> &mut Self {
        for hex in view.board.arrangement.hex_iter_with_ocean() {
            self.draw_hex_attr(hex, HexAttr::Index);
        }
        self
    }

    pub fn draw_context(&mut self, view: &PublicGameView<'_>) {
        // render skeleton
        self.draw_skeleton(view);

        // render tile info
        self.draw_tile_info(view);

        // render hex indices
        self.draw_index(view);

        // render robber
        self.draw_robber(view);

        // render builds
        self.draw_builds(view);
    }

    pub fn draw_ui_public(&mut self, view: &UiPublicGame) {
        self.draw_field(ColorSpec::new().set_dimmed(true).clone());

        for (index, tile) in view.board.tiles.iter().copied().enumerate() {
            let hex = HexIndex::spiral_to_hex(index);
            match tile {
                Tile::Resource { resource, number } => {
                    self.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                    self.draw_hex_attr(hex, HexAttr::Resource(resource));
                }
                Tile::Desert => {}
                Tile::River { number } => {
                    self.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                }
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

    pub fn plain_lines(&self) -> Vec<String> {
        (0..self.height)
            .map(|y| {
                let start = y * self.width;
                let end = (y + 1) * self.width;
                String::from_utf8_lossy(&self.buf.slice()[start..end]).into_owned()
            })
            .collect()
    }

    pub fn render(&self) {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        writeln!(stdout, "\n{}x{}", self.width, self.height).unwrap();
        write!(stdout, " ").unwrap();
        for x in 0..self.width {
            write!(stdout, "{}", x % 10).unwrap();
        }
        write!(stdout, "\n").unwrap();
        for y in 0..self.height {
            write!(stdout, "{}", y % 10).unwrap();

            let start = y * self.width;
            let end = (y + 1) * self.width;

            for i in start..end {
                let fmt = self.fmt.slice()[i].clone();
                let chr = self.buf.slice()[i];

                stdout.set_color(&fmt).unwrap();
                write!(&mut stdout, "{}", chr as char).unwrap();
            }

            stdout.set_color(&ColorSpec::new()).unwrap();

            write!(stdout, "{}", y % 10).unwrap();
            write!(stdout, "\n").unwrap();
        }
        write!(stdout, " ").unwrap();
        for x in 0..self.width {
            write!(stdout, "{}", x % 10).unwrap();
        }
        write!(stdout, "\n").unwrap();
    }

    /* buffer combining */

    pub fn paste_fmt(&mut self, buf: &BufFragment, fmt: &BufFragment<ColorSpec>) {
        self.buf.paste(&buf);
        self.fmt.paste(&fmt);
    }

    pub fn paste_with_blank_fmt(&mut self, buf: &BufFragment, fmt: &BufFragment<ColorSpec>) {
        self.buf.paste_with_blank(&buf);
        self.fmt.paste_with_blank(&fmt);
    }

    /* draw game objects */

    pub fn draw_intersection_attr(&mut self, intersection: Intersection, attr: IntersectionAttr) {
        match attr {
            IntersectionAttr::Selector => self.draw_intersection_selector(intersection),
            IntersectionAttr::Establishment(establishment_type, color) => {
                self.draw_intersection_establishment(intersection, establishment_type, color)
            }
        }
    }

    pub fn draw_hex_attr(&mut self, hex: Hex, attr: HexAttr) {
        match attr {
            HexAttr::TileNum(num) => self.draw_hex_tile_num(hex, num),
            HexAttr::Index => self.draw_hex_index(hex),
            HexAttr::Resource(res) => self.draw_hex_resourse(hex, res),
            HexAttr::Robber => self.draw_hex_robber(hex),
            HexAttr::Selector => self.draw_hex_selector(hex),
        }
    }

    pub fn draw_path_attr(&mut self, path: Path, attr: PathAttr) {
        match attr {
            PathAttr::Road { color } => self.draw_path(
                path,
                ColorSpec::new()
                    .set_bold(true)
                    .set_intense(true)
                    .set_fg(Some(color))
                    .clone(),
            ),
            PathAttr::Selector => self.draw_path(
                path,
                ColorSpec::new()
                    .set_bold(true)
                    .set_bg(Some(Color::Red))
                    .clone(),
            ),
        }
    }

    pub fn draw_field(&mut self, fmt: ColorSpec) {
        let paths = (0..=2)
            .flat_map(|radius| HexIndex::hex_ring(Hex::new(0, 0), radius))
            .flat_map(|h| h.paths_arr())
            .collect::<BTreeSet<_>>();

        for path in paths {
            self.draw_path(path, fmt.clone());
        }
    }

    /* private helpers */

    fn draw_path(&mut self, path: Path, fmt: ColorSpec) {
        let (buf, fmt) = utils::path_buf_fmt(path, fmt);
        self.buf.paste(&buf);
        self.fmt.paste(&fmt);
    }

    fn draw_hex_tile_num(&mut self, hex: Hex, num: u8) {
        let (buf, fmt) = utils::hex_textbox_fmt(
            hex,
            1,
            format!("{}", num).as_bytes(),
            match num {
                // 6 | 8 => ColorSpec::new().set_fg(Some(Color::Ansi256(131))).clone(),
                6 | 8 => ColorSpec::new().set_fg(Some(Color::Red)).clone(),
                _ => ColorSpec::new().set_fg(Some(Color::Ansi256(188))).clone(),
            },
        );

        self.paste_fmt(&buf, &fmt);
    }

    fn draw_hex_robber(&mut self, hex: Hex) {
        let (buf, fmt) = utils::hex_textbox_fmt(
            hex,
            3,
            "ROB".as_bytes(),
            ColorSpec::new().set_bg(Some(Color::Ansi256(244))).clone(),
        );

        self.paste_fmt(&buf, &fmt);
    }

    fn draw_hex_selector(&mut self, hex: Hex) {
        let (buf, fmt) = utils::hex_textbox_fmt(
            hex,
            3,
            "[ ]".as_bytes(),
            ColorSpec::new().set_bg(Some(Color::Red)).clone(),
        );

        self.paste_with_blank_fmt(&buf, &fmt);
    }

    fn draw_hex_index(&mut self, hex: Hex) {
        let (buf, fmt) = utils::hex_textbox_fmt(
            hex,
            3,
            format!("{}", hex.index().to_spiral()).as_bytes(),
            ColorSpec::new().set_dimmed(true).clone(),
        );

        self.paste_fmt(&buf, &fmt);
    }

    fn draw_hex_resourse(&mut self, hex: Hex, res: Resource) {
        let (buf, fmt) = utils::hex_textbox_fmt(
            hex,
            2,
            Into::<&str>::into(res).as_bytes(),
            ColorSpec::new()
                .set_fg(Some(Self::resource_color(res)))
                .clone(),
        );

        self.paste_fmt(&buf, &fmt);
    }

    fn draw_intersection_selector(&mut self, v: Intersection) {
        let pos = utils::intersection_anchor(v);
        let (buf, fmt) = utils::textbox_buf_centered_fmt(
            pos,
            "[ ]".as_bytes(),
            ColorSpec::new().set_bg(Some(Color::Red)).clone(),
        );
        self.paste_with_blank_fmt(&buf, &fmt);
    }

    fn draw_intersection_establishment(
        &mut self,
        v: Intersection,
        kind: EstablishmentType,
        color: Color,
    ) {
        let pos = utils::intersection_anchor(v);
        let s = match kind {
            EstablishmentType::Settlement => b"(S)",
            EstablishmentType::City => b"[C]",
        };
        let (buf, fmt) = utils::textbox_buf_centered_fmt(
            pos,
            s,
            ColorSpec::new().set_fg(Some(color)).set_bold(true).clone(),
        );
        self.paste_with_blank_fmt(&buf, &fmt);
    }
}
