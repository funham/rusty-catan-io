pub mod cursor {
    use std::ops::{Add, Mul, Sub};

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
    use std::ops::{Index, IndexMut};

    use termcolor::Color;

    use crate::cli_agent::ascii::cursor::CursorPosition;

    pub trait Bufferable: Sized + PartialEq + Clone + Copy {
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
                    *result.at_mut(x, y) = s[y * width + x];
                }
            }

            result
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

        pub fn paste_at(&mut self, x0: isize, y0: isize, paste: &Self) {
            for y in 0..paste.height {
                for x in 0..paste.width {
                    let xb = (x0 + x as isize) as usize;
                    let yb = (y0 + y as isize) as usize;

                    if !(0..self.width).contains(&xb) || !(0..self.height).contains(&yb) {
                        continue;
                    }

                    if paste.at(x, y).is_blank() {
                        continue;
                    }

                    *self.at_mut(xb, yb) = *paste.at(x, y);
                }
            }
        }

        pub fn paste(&mut self, paste: &BufFragment<T>) {
            self.paste_at(paste.pos.x as isize, paste.pos.y as isize, &paste.fragment);
        }

        pub fn clear(&mut self) {
            self.fill(T::blank());
        }

        pub fn fill(&mut self, val: T) {
            self.buf.fill(val);
        }

        pub fn get_slice(&self) -> &[T] {
            &self.buf[..]
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
    use catan_core::topology::Path;
    use std::io::Write;
    use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

    use crate::cli_agent::ascii::buffer::Buffer;

    mod utils {
        use catan_core::topology::{Axis, Path};
        use termcolor::Color;

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
                    let (x, y0) = hex_anchor_shifted(h1.q, h1.r);
                    let (_, y1) = hex_anchor_shifted(h2.q, h2.r);
                    let y = y0.max(y1);

                    CursorPosition { x: x + 2, y }
                }
                Axis::R => {
                    let (h1, h2) = path.as_pair();
                    let (x0, y0) = hex_anchor_shifted(h1.q, h1.r);
                    let (x1, y1) = hex_anchor_shifted(h2.q, h2.r);
                    let (y, x) = (y0, x0).max((y1, x1));

                    CursorPosition { x, y: y + 1 }
                }
                Axis::S => {
                    let (h1, h2) = path.as_pair();
                    let (x0, y0) = hex_anchor_shifted(h1.q, h1.r);
                    let (x1, y1) = hex_anchor_shifted(h2.q, h2.r);
                    let (y, x) = (y0, x0).min((y1, x1));

                    CursorPosition { x, y: y + 3 }
                }
            }
        }

        pub fn path_buf(path: Path) -> BufFragment {
            BufFragment {
                fragment: axis_path_buf(path.axis()),
                pos: path_anchor(path),
            }
        }

        pub fn path_buf_clr(path: Path, color: Color) -> BufFragment<Color> {
            let (width, height) = match path.axis() {
                Axis::Q => (7, 1),
                _ => (2, 2),
            };
            BufFragment::<Color> {
                fragment: Buffer::<Color>::new_with(width, height, color),
                pos: path_anchor(path),
            }
        }

        pub const fn hex_anchor_shifted(q: i32, r: i32) -> (i32, i32) {
            // supposing (-2, -1) -> (0, 0)
            //
            // (+1, +0) -> (+9, +2)  (y-axis inverted)
            // (+0, +1) -> (+0, +4)

            let (q, r) = (q + 2, r + 1);

            (q * 9, q * 2 + r * 4)
        }
    }

    pub struct FieldRenderer {
        width: usize,
        height: usize,
        buf: Buffer<u8>,
        clr: Buffer<Color>,
    }

    impl FieldRenderer {
        pub fn new() -> Self {
            let width = 47;
            let height = 21;

            Self {
                width,
                height,
                buf: Buffer::new(width, height),
                clr: Buffer::new(width, height),
            }
        }

        pub fn clear(&mut self) {
            self.buf.clear();
            self.clr.clear();
        }

        pub fn draw_path(&mut self, path: Path, color: Color) {
            self.buf.paste(&utils::path_buf(path));
            self.clr.paste(&utils::path_buf_clr(path, color));
        }

        // pub fn draw_field(&mut self, color: Color) {}

        pub fn render(&self) {
            let mut stdout = StandardStream::stdout(ColorChoice::Always);

            print!(" ");
            for x in 0..self.width {
                print!("{}", x % 10);
            }
            print!("\n");
            for y in 0..self.height {
                print!("{}", y % 10);

                let start = y * self.width;
                let end = (y + 1) * self.width;

                for i in start..end {
                    let color = self.clr.get_slice()[i];
                    let chr = self.buf.get_slice()[i];

                    stdout
                        .set_color(ColorSpec::new().set_fg(Some(color)))
                        .unwrap();
                    write!(&mut stdout, "{}", chr as char).unwrap();
                }

                stdout.set_color(ColorSpec::new().set_fg(None)).unwrap();

                print!("{}", y % 10);
                print!("\n");
            }
            print!(" ");
            for x in 0..self.width {
                print!("{}", x % 10);
            }
            print!("\n");
        }
    }
}

pub mod test {
    use catan_core::topology::Hex;
    use termcolor::Color;

    use super::*;

    #[test]
    fn render_color_paths() {
        use field_render::*;

        let mut field = FieldRenderer::new();

        for (path, color) in Hex::new(0, 0).paths_arr().iter().zip([
            Color::White,
            Color::Black,
            Color::Blue,
            Color::Cyan,
            Color::Yellow,
            Color::Magenta,
        ]) {
            field.draw_path(*path, color);
        }

        field.render();
    }
}
