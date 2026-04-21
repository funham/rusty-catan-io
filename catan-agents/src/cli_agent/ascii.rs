pub mod cursor {
    use std::ops::{Add, Mul, Sub};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct CursorPosition {
        pub x: i32,
        pub y: i32,
    }

    impl CursorPosition {
        pub fn new(x: i32, y: i32) -> Self {
            Self { x, y }
        }

        pub fn transposed(&self) -> Self {
            Self {
                x: self.y,
                y: self.x,
            }
        }
    }

    impl Add for CursorPosition {
        type Output = Self;

        fn add(self, rhs: Self) -> Self::Output {
            Self::Output {
                x: self.x + rhs.x,
                y: self.y + rhs.y,
            }
        }
    }

    impl Sub for CursorPosition {
        type Output = Self;

        fn sub(self, rhs: Self) -> Self::Output {
            Self::Output {
                x: self.x - rhs.x,
                y: self.y - rhs.y,
            }
        }
    }

    impl Mul<i32> for CursorPosition {
        type Output = CursorPosition;

        fn mul(self, rhs: i32) -> Self::Output {
            Self::Output {
                x: self.x * rhs,
                y: self.y * rhs,
            }
        }
    }

    impl Into<CursorPosition> for (i32, i32) {
        fn into(self) -> CursorPosition {
            CursorPosition {
                x: self.0,
                y: self.1,
            }
        }
    }
}

pub mod buffer {
    use crate::cli_agent::ascii::cursor::CursorPosition;
    use std::ops::{Index, IndexMut};
    use termcolor::{Color, ColorSpec};

    pub trait Bufferable: Sized + PartialEq + Clone {
        fn blank() -> Self;
        fn is_blank(&self) -> bool {
            *self == Self::blank()
        }
    }

    impl Bufferable for u8 {
        fn blank() -> Self {
            b' '
        }
    }

    impl Bufferable for Color {
        fn blank() -> Self {
            Color::White
        }
    }

    impl Bufferable for ColorSpec {
        fn blank() -> Self {
            ColorSpec::new()
        }
    }

    #[derive(Clone)]
    pub struct Buffer<T: Bufferable = u8> {
        width: usize,
        height: usize,
        buf: Vec<T>,
    }

    impl<T: Bufferable> Buffer<T> {
        pub fn new_with(width: usize, height: usize, value: T) -> Self {
            Self {
                width,
                height,
                buf: vec![value; width * height],
            }
        }

        pub fn new(width: usize, height: usize) -> Self {
            Self::new_with(width, height, T::blank())
        }

        pub fn from_string(width: usize, height: usize, s: &[T]) -> Self {
            assert_eq!(s.len(), width * height);
            let mut result = Self::new(width, height);

            for y in 0..height {
                for x in 0..width {
                    *result.at_mut(x, y) = s[y * width + x].clone();
                }
            }

            result
        }

        pub fn same_sized<Y: Bufferable>(&self) -> Buffer<Y> {
            Buffer::<Y>::new(self.width, self.height)
        }

        pub fn width(&self) -> usize {
            self.width
        }

        pub fn height(&self) -> usize {
            self.height
        }

        pub fn at(&self, x: usize, y: usize) -> &T {
            &self.buf[y * self.width + x]
        }

        pub fn at_mut(&mut self, x: usize, y: usize) -> &mut T {
            &mut self.buf[y * self.width + x]
        }

        fn paste_at(&mut self, x0: isize, y0: isize, paste: &Self, ignore_blank: bool) {
            for y in 0..paste.height {
                for x in 0..paste.width {
                    let xb = (x0 + x as isize) as usize;
                    let yb = (y0 + y as isize) as usize;

                    if !(0..self.width).contains(&xb) || !(0..self.height).contains(&yb) {
                        continue;
                    }

                    if ignore_blank && paste.at(x, y).is_blank() {
                        continue;
                    }

                    *self.at_mut(xb, yb) = paste.at(x, y).clone();
                }
            }
        }

        pub fn paste(&mut self, paste: &BufFragment<T>) {
            self.paste_at(
                paste.pos.x as isize,
                paste.pos.y as isize,
                &paste.fragment,
                true,
            );
        }

        pub fn paste_with_blank(&mut self, paste: &BufFragment<T>) {
            self.paste_at(
                paste.pos.x as isize,
                paste.pos.y as isize,
                &paste.fragment,
                false,
            );
        }

        pub fn clear(&mut self) {
            self.fill(T::blank());
        }

        pub fn fill(&mut self, val: T) {
            self.buf.fill(val);
        }

        pub fn slice(&self) -> &[T] {
            &self.buf[..]
        }

        pub fn slice_mut(&mut self) -> &mut [T] {
            &mut self.buf[..]
        }
    }

    impl Buffer<u8> {
        pub fn print(&self) {
            print!(" ");
            for x in 0..self.width {
                print!("{}", x % 10);
            }
            print!("\n");
            for y in 0..self.height {
                let start = y * self.width;
                let end = (y + 1) * self.width;
                print!(
                    "{}{}{}",
                    y % 10,
                    std::str::from_utf8(&self.buf[start..end]).unwrap(),
                    y % 10,
                );
                print!("\n");
            }
            print!(" ");
            for x in 0..self.width {
                print!("{}", x % 10);
            }
            print!("\n");
        }

        /// format non-blank symbols
        pub fn format(&self, fmt: ColorSpec) -> Buffer<ColorSpec> {
            let mut fragment = self.same_sized::<ColorSpec>();

            for (f, ch) in fragment.slice_mut().iter_mut().zip(self.slice()) {
                if !ch.is_blank() {
                    *f = fmt.clone();
                }
            }

            fragment.clone()
        }

        /// apply format to the whole buffer
        pub fn format_full(&self, fmt: ColorSpec) -> Buffer<ColorSpec> {
            let mut fragment = self.same_sized::<ColorSpec>();
            fragment.fill(fmt);
            fragment.clone()
        }
    }

    impl IndexMut<CursorPosition> for Buffer {
        fn index_mut(&mut self, index: CursorPosition) -> &mut Self::Output {
            &mut self.buf[(index.y * self.width as i32 + index.x) as usize]
        }
    }

    impl Index<CursorPosition> for Buffer {
        type Output = u8;

        fn index(&self, index: CursorPosition) -> &Self::Output {
            &self.buf[(index.y * self.width as i32 + index.x) as usize]
        }
    }

    pub struct BufFragment<T: Bufferable = u8> {
        pub fragment: Buffer<T>,
        pub pos: CursorPosition,
    }
}

pub mod field_render {
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

        pub fn draw_intersection_attr(
            &mut self,
            intersection: Intersection,
            attr: IntersectionAttr,
        ) {
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
}

pub mod test {
    use super::field_render::*;
    use catan_core::{
        gameplay::{
            game::init::GameInitializationState,
            primitives::{
                Tile,
                build::{Build, Establishment, EstablishmentType, Road},
                resource::Resource,
            },
        },
        topology::{Hex, Intersection, Path},
    };
    use termcolor::{Color, ColorSpec};

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn p(h1: Hex, h2: Hex) -> Path {
        Path::try_from((h1, h2)).unwrap()
    }

    fn v(h1: Hex, h2: Hex, h3: Hex) -> Intersection {
        Intersection::try_from((h1, h2, h3)).unwrap()
    }

    #[test]
    fn hex_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(Hex::new(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Robber);
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_hex_attr(Hex::new(1, 0), HexAttr::TileNum(6));
        field.draw_hex_attr(Hex::new(1, 0), HexAttr::Selector);
        field.render();
    }

    #[test]
    fn path_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(Hex::new(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Robber);
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_hex_attr(Hex::new(1, 0), HexAttr::TileNum(6));
        field.draw_hex_attr(Hex::new(1, 0), HexAttr::Resource(Resource::Wood));
        field.draw_hex_attr(Hex::new(1, 0), HexAttr::Selector);

        field.draw_path_attr(
            p(h(0, 0), h(0, 1)),
            PathAttr::Road {
                color: Color::Green,
            },
        );

        field.draw_path_attr(
            p(h(0, 0), h(-1, 1)),
            PathAttr::Road {
                color: Color::Green,
            },
        );

        field.render();
    }

    #[test]
    fn intersection_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(Hex::new(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Robber);
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_path_attr(
            p(h(0, 0), h(-1, 1)),
            PathAttr::Road {
                color: Color::Green,
            },
        );

        for (i, v) in h(0, 0).vertices().enumerate() {
            field.draw_intersection_attr(v, IntersectionAttr::Selector);
        }

        for (i, v) in h(-2, 0).vertices().enumerate() {
            field.draw_intersection_attr(
                v,
                IntersectionAttr::Establishment(EstablishmentType::Settlement, Color::Green),
            );
        }

        for (i, v) in h(2, 0).vertices().enumerate() {
            field.draw_intersection_attr(
                v,
                IntersectionAttr::Establishment(EstablishmentType::City, Color::Blue),
            );
        }

        field.render();
    }

    #[test]
    fn field_example() {
        let mut renderer = FieldRenderer::new();
        let mut game = GameInitializationState::default();

        // render skeleton
        renderer.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        // render tile info
        for (hex, tile) in game.field.arrangement.hex_enum_iter() {
            match tile {
                Tile::Resource { resource, number } => {
                    renderer.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                    renderer.draw_hex_attr(hex, HexAttr::Resource(resource));
                }
                Tile::Desert => {}
                _ => todo!(),
            }
        }

        renderer.draw_hex_attr(game.field.robber_pos, HexAttr::Robber);

        let player_colors = [Color::Blue, Color::White, Color::Red, Color::Ansi256(172)];
        let settlements = [
            // player #0
            v(h(0, 0), h(1, 0), h(1, -1)),
            v(h(0, 0), h(-1, 1), h(0, 1)),
            //player #1
            v(h(-1, -1), h(-1, 0), h(0, -1)),
            v(h(0, -2), h(1, -2), h(1, -3)),
            // player #2
            v(h(1, 0), h(2, 0), h(2, -1)),
            v(h(0, 1), h(0, 2), h(-1, 2)),
            //player #3
            v(h(-2, 1), h(-3, 2), h(-3, 1)),
            v(h(-2, 0), h(-3, 1), h(-3, 0)),
        ];
        let roads = [
            // player #0
            p(h(0, 0), h(1, 0)),
            p(h(0, 0), h(0, 1)),
            // player #1
            p(h(-1, -1), h(-1, 0)),
            p(h(0, -2), h(1, -2)),
            // player #2
            p(h(1, 0), h(2, 0)),
            p(h(0, 1), h(0, 2)),
            // player #3
            p(h(-2, 1), h(-3, 2)),
            p(h(-2, 0), h(-3, 1)),
        ];

        for (i, (est_pos, road_pos)) in settlements.iter().zip(roads.iter()).enumerate() {
            game.builds
                .try_init_place(
                    i / 2,
                    Road { pos: *road_pos },
                    Establishment {
                        pos: *est_pos,
                        stage: EstablishmentType::Settlement,
                    },
                )
                .expect(&format!(
                    "try_build failed on settlement {:?} and road {:?}; builds snapshot: {:?}",
                    est_pos, road_pos, game.builds
                ));
        }

        for (id, player) in game.builds.players().iter().enumerate() {
            for road in player.roads.iter() {
                renderer.draw_path_attr(
                    road.pos,
                    PathAttr::Road {
                        color: player_colors[id],
                    },
                );
            }

            for est in player.establishments.iter() {
                renderer.draw_intersection_attr(
                    est.pos,
                    IntersectionAttr::Establishment(est.stage, player_colors[id]),
                );
            }
        }

        renderer.render();
    }
}
