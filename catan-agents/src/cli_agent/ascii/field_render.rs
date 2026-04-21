
use catan_core::{
    gameplay::{
        game::state::Perspective,
        primitives::{Tile, build::EstablishmentType, player::PlayerId, resource::Resource},
    },
    topology::{Hex, HexIndex, Intersection, Path},
};
use std::{collections::BTreeSet, io::Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::cli_agent::ascii::buffer::{BufFragment, Buffer};

mod utils {
    use catan_core::topology::{Axis, Hex, Intersection, Path, SignedAxis};
    use termcolor::ColorSpec;

    use crate::cli_agent::ascii::{
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

        let (q, r) = (q + 2, r + 1);
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
    DebugCoords,
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
        let width = 49;
        let height = 21;

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

    pub fn draw_perspective(&mut self, perspective: &Perspective) {
        // render skeleton
        self.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        // render tile info
        for (hex, tile) in perspective.field.arrangement.hex_enum_iter() {
            match tile {
                Tile::Resource { resource, number } => {
                    self.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                    self.draw_hex_attr(hex, HexAttr::Resource(resource));
                }
                Tile::Desert => {}
                _ => todo!(),
            }
        }

        // render robber
        self.draw_hex_attr(perspective.field.robber_pos, HexAttr::Robber);

        // render builds
        for (id, player) in perspective.builds.players().iter().enumerate() {
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
    }

    pub fn render(&self) {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

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
            HexAttr::DebugCoords => todo!(),
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
                6 | 8 => ColorSpec::new().set_fg(Some(Color::Ansi256(131))).clone(),
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
        for path in hex.paths_arr() {
            self.draw_path(path, ColorSpec::new().set_bg(Some(Color::Red)).clone());
        }
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
