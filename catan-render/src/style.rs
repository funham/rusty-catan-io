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

impl From<termcolor::Color> for RenderColor {
    fn from(value: termcolor::Color) -> Self {
        match value {
            termcolor::Color::Black => Self::Black,
            termcolor::Color::Blue => Self::Blue,
            termcolor::Color::Green => Self::Green,
            termcolor::Color::Red => Self::Red,
            termcolor::Color::Cyan => Self::Cyan,
            termcolor::Color::Magenta => Self::Magenta,
            termcolor::Color::Yellow => Self::Yellow,
            termcolor::Color::White => Self::White,
            termcolor::Color::Ansi256(color) => Self::Ansi256(color),
            _ => Self::White,
        }
    }
}

impl From<RenderColor> for termcolor::Color {
    fn from(value: RenderColor) -> Self {
        match value {
            RenderColor::Black => Self::Black,
            RenderColor::Red => Self::Red,
            RenderColor::Green => Self::Green,
            RenderColor::Yellow => Self::Yellow,
            RenderColor::Blue => Self::Blue,
            RenderColor::Magenta => Self::Magenta,
            RenderColor::Cyan => Self::Cyan,
            RenderColor::White => Self::White,
            RenderColor::Ansi256(color) => Self::Ansi256(color),
        }
    }
}

impl From<termcolor::ColorSpec> for RenderStyle {
    fn from(value: termcolor::ColorSpec) -> Self {
        Self {
            fg: value.fg().copied().map(Into::into),
            bg: value.bg().copied().map(Into::into),
            bold: value.bold(),
            dim: value.dimmed(),
        }
    }
}

impl From<RenderStyle> for termcolor::ColorSpec {
    fn from(value: RenderStyle) -> Self {
        let mut spec = termcolor::ColorSpec::new();
        spec.set_fg(value.fg.map(Into::into))
            .set_bg(value.bg.map(Into::into))
            .set_bold(value.bold)
            .set_dimmed(value.dim);
        spec
    }
}
