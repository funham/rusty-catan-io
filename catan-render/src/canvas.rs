use crate::{cursor::CursorPosition, style::RenderStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: RenderStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: RenderStyle::default(),
        }
    }
}

impl Cell {
    pub fn is_blank(&self) -> bool {
        self.ch == ' ' && self.style == RenderStyle::default()
    }
}

#[derive(Debug, Clone)]
pub struct Canvas {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    pub fn at(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.width + x]
    }

    pub fn set(&mut self, pos: CursorPosition, cell: Cell) {
        if pos.x < 0 || pos.y < 0 {
            return;
        }
        let (x, y) = (pos.x as usize, pos.y as usize);
        if x >= self.width || y >= self.height {
            return;
        }
        self.cells[y * self.width + x] = cell;
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    pub fn write_text(&mut self, pos: CursorPosition, text: &str, style: RenderStyle) {
        for (offset, ch) in text.chars().enumerate() {
            if ch != ' ' {
                self.set(
                    pos + CursorPosition::new(offset as i32, 0),
                    Cell { ch, style },
                );
            }
        }
    }

    pub fn write_text_with_blank(&mut self, pos: CursorPosition, text: &str, style: RenderStyle) {
        for (offset, ch) in text.chars().enumerate() {
            self.set(
                pos + CursorPosition::new(offset as i32, 0),
                Cell { ch, style },
            );
        }
    }

    pub fn plain_lines(&self) -> Vec<String> {
        (0..self.height)
            .map(|y| {
                (0..self.width)
                    .map(|x| self.at(x, y).ch)
                    .collect::<String>()
            })
            .collect()
    }
}
