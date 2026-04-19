use crate::cli_agent::ascii::{buffer::Buffer, cursor::CursorPosition};
use catan_core::topology::{Axis, Hex, HexIndex, Path, repr::Canon};

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

    use crate::cli_agent::ascii::cursor::CursorPosition;

    pub struct Buffer {
        width: usize,
        height: usize,
        buf: Vec<u8>,
    }

    impl Buffer {
        pub fn new(width: usize, height: usize) -> Self {
            Self {
                width,
                height,
                buf: vec![b' '; width * height],
            }
        }

        pub fn from_string(width: usize, height: usize, s: &str) -> Self {
            assert_eq!(s.as_bytes().len(), width * height);
            let mut result: Buffer = Self::new(width, height);

            for y in 0..height {
                for x in 0..width {
                    *result.at_mut(x, y) = s.as_bytes()[y * width + x];
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

        pub fn at(&self, x: usize, y: usize) -> u8 {
            self.buf[y * self.width + x]
        }

        pub fn at_mut(&mut self, x: usize, y: usize) -> &mut u8 {
            &mut self.buf[y * self.width + x]
        }

        pub fn paste_at(&mut self, x0: isize, y0: isize, paste: &Buffer) {
            for y in 0..paste.height {
                for x in 0..paste.width {
                    let xb = (x0 + x as isize) as usize;
                    let yb = (y0 + y as isize) as usize;

                    if !(0..self.width).contains(&xb) || !(0..self.height).contains(&yb) {
                        continue;
                    }

                    if paste.at(x, y) == b' ' {
                        continue;
                    }

                    *self.at_mut(xb, yb) = paste.at(x, y);
                }
            }
        }

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
}

pub struct IndexedBuffer {
    buf: Buffer,
    pos: CursorPosition,
}

pub fn axis_path_buf(axis: Axis) -> Buffer {
    match axis {
        Axis::Q => Buffer::from_string(7, 1, r"_______"),
        Axis::R => Buffer::from_string(2, 2, r" // "),
        Axis::S => Buffer::from_string(2, 2, r"\  \"),
    }
}

pub fn path_anchor(path: Path) -> CursorPosition {
    match path.axis() {
        // /
        Axis::Q => {
            let (h1, h2) = path.as_pair();
            let (x0, y0) = hex_anchor_shifted(h1.q, h1.r);
            let (x1, y1) = hex_anchor_shifted(h2.q, h2.r);
            let (x, y) = (x0, y0).max((x1, y1));

            CursorPosition { x, y: y + 1 }
        }
        // \
        Axis::R => {
            let (h1, h2) = path.as_pair();
            let (x0, y0) = hex_anchor_shifted(h1.q, h1.r);
            let (x1, y1) = hex_anchor_shifted(h2.q, h2.r);
            let (y, x) = (y0, x0).max((y1, x1));

            CursorPosition { x, y: y + 3 }
        }
        // _
        Axis::S => {
            let (h1, h2) = path.as_pair();
            let (x, y0) = hex_anchor_shifted(h1.q, h1.r);
            let (_, y1) = hex_anchor_shifted(h2.q, h2.r);
            let y = i32::min(y0, y1);

            CursorPosition { x, y }
        }
    }
}

pub fn path_buf(path: Path) -> IndexedBuffer {
    IndexedBuffer {
        buf: axis_path_buf(path.axis()),
        pos: path_anchor(path),
    }
}

pub const fn hex_anchor(q: i32, r: i32) -> (i32, i32) {
    // supposing (0, 0) -> (0, 0)
    //
    // (+1, +0) -> (+9, +2)
    // (+0, +1) -> (+9, -2)

    ((q + r) * 9, (q - r) * 2)
}

// pub const fn hex_anchor_shifted(q: i32, r: i32) -> (i32, i32) {
//     // supposing (-3, 1) -> (0, 0)
//     //
//     // (+1, +0) -> (+9, +2)  (y-axis inverted)
//     // (+0, +1) -> (+9, -2)

//     let q = q + 3;
//     let r = r - 1;

//     ((q + r) * 9, (q - r) * 2)
// }

pub const fn hex_anchor_shifted(q: i32, r: i32) -> (i32, i32) {
    // supposing (-2, -1) -> (0, 0)
    //
    // (+1, +0) -> (+9, +2)  (y-axis inverted)
    // (+0, +1) -> (+0, +4)

    let (q, r) = (q + 2, r + 1);

    (q * 9, q * 2 + r * 4)
}

pub fn draw_field() {
    let hexagon = r"  _______   /       \ /         \\         / \_______/ ";
    // let hexagon = r".._______.../.......\./.........\\........./.\_______/.";

    let hexagon = Buffer::from_string(11, 5, hexagon);
    let mut buf = Buffer::new(47, 21);
    // let mut buf = Buffer::new(30, 20);

    let (x, y) = hex_anchor_shifted(-3, 1);
    buf.paste_at(x as isize, y as isize, &hexagon);

    for radius in 0..=2 {
        let hexes = HexIndex::hex_ring(Hex::new(0, 0), radius);

        for h in hexes {
            let (x, y) = hex_anchor_shifted(h.q, h.r);
            buf.paste_at(x as isize, y as isize, &hexagon);
        }
    }

    buf.print();
}

pub fn draw_paths() {
    let hexagon = r"  _______   /       \ /         \\         / \_______/ ";
    // let hexagon = r".._______.../.......\./.........\\........./.\_______/.";

    let hexagon = Buffer::from_string(11, 5, hexagon);
    let mut buf = Buffer::new(47, 21);
    // let mut buf = Buffer::new(30, 20);

    let (x, y) = hex_anchor_shifted(-3, 1);
    buf.paste_at(x as isize, y as isize, &hexagon);

    let (x, y) = hex_anchor_shifted(-2, -1);
    buf.paste_at(x as isize, y as isize, &hexagon);

    let (x, y) = hex_anchor_shifted(0, 0);
    buf.paste_at(x as isize, y as isize, &hexagon);

    let (x, y) = hex_anchor_shifted(1, 0);
    buf.paste_at(x as isize, y as isize, &hexagon);

    let hexes = HexIndex::hex_ring(Hex::new(0, 0), 2);

    for h in hexes {
        let (x, y) = hex_anchor_shifted(h.q, h.r);
        buf.paste_at(x as isize, y as isize, &hexagon);
    }

    // for (i, path) in Hex::new(0, 0).paths_arr().iter().enumerate() {
    //     let mut path_indexed_buf = path_buf(*path);

    //     *path_indexed_buf.buf.at_mut(0, 0) = (i as u8) + (b'0');

    //     buf.paste_at(
    //         path_indexed_buf.pos.x as isize,
    //         path_indexed_buf.pos.y as isize,
    //         &path_indexed_buf.buf,
    //     );
    // }

    buf.print();
}

pub mod test {
    #[test]
    fn draw_field() {
        super::draw_field();
    }

    #[test]
    fn draw_paths() {
        super::draw_paths();
    }
}
