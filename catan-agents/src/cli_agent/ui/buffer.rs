use crate::cli_agent::ui::cursor::CursorPosition;
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
