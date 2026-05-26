#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    #[default]
    White,
    Ansi256(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderStyle {
    pub fg: Option<RenderColor>,
    pub bg: Option<RenderColor>,
    pub bold: bool,
    pub dim: bool,
}

impl RenderStyle {
    pub fn fg(mut self, color: RenderColor) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: RenderColor) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
}
