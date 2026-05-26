use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    canvas::{Canvas, Cell},
    style::{RenderColor, RenderStyle},
};

pub fn color(color: RenderColor) -> Color {
    match color {
        RenderColor::Black => Color::Black,
        RenderColor::Red => Color::Red,
        RenderColor::Green => Color::Green,
        RenderColor::Yellow => Color::Yellow,
        RenderColor::Blue => Color::Blue,
        RenderColor::Magenta => Color::Magenta,
        RenderColor::Cyan => Color::Cyan,
        RenderColor::White => Color::White,
        RenderColor::Ansi256(value) => Color::Indexed(value),
    }
}

pub fn style(style: RenderStyle) -> Style {
    let mut out = Style::default();
    if let Some(fg) = style.fg {
        out = out.fg(color(fg));
    }
    if let Some(bg) = style.bg {
        out = out.bg(color(bg));
    }
    if style.bold {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.dim {
        out = out.add_modifier(Modifier::DIM);
    }
    out
}

pub fn canvas_lines(canvas: &Canvas) -> Vec<Line<'static>> {
    (0..canvas.height())
        .map(|y| {
            let mut spans = Vec::new();
            let mut current_style = RenderStyle::default();
            let mut current = String::new();

            for x in 0..canvas.width() {
                let Cell { ch, style } = *canvas.at(x, y);
                if !current.is_empty() && style != current_style {
                    spans.push(Span::styled(
                        std::mem::take(&mut current),
                        self::style(current_style),
                    ));
                }
                current_style = style;
                current.push(ch);
            }

            if !current.is_empty() {
                spans.push(Span::styled(current, self::style(current_style)));
            }

            Line::from(spans)
        })
        .collect()
}
