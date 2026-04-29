use std::io;

use termcolor::WriteColor;

use crate::canvas::Canvas;

pub fn write_canvas(canvas: &Canvas, writer: &mut impl WriteColor) -> io::Result<()> {
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let cell = canvas.at(x, y);
            let spec = termcolor::ColorSpec::from(cell.style);
            writer.set_color(&spec)?;
            write!(writer, "{}", cell.ch)?;
        }
        writer.set_color(&termcolor::ColorSpec::new())?;
        writeln!(writer)?;
    }
    Ok(())
}

pub fn write_canvas_with_rulers(canvas: &Canvas, writer: &mut impl WriteColor) -> io::Result<()> {
    writeln!(writer, "\n{}x{}", canvas.width(), canvas.height())?;
    write!(writer, " ")?;
    for x in 0..canvas.width() {
        write!(writer, "{}", x % 10)?;
    }
    writeln!(writer)?;

    for y in 0..canvas.height() {
        write!(writer, "{}", y % 10)?;
        for x in 0..canvas.width() {
            let cell = canvas.at(x, y);
            let spec = termcolor::ColorSpec::from(cell.style);
            writer.set_color(&spec)?;
            write!(writer, "{}", cell.ch)?;
        }
        writer.set_color(&termcolor::ColorSpec::new())?;
        writeln!(writer, "{}", y % 10)?;
    }

    write!(writer, " ")?;
    for x in 0..canvas.width() {
        write!(writer, "{}", x % 10)?;
    }
    writeln!(writer)?;
    writer.set_color(&termcolor::ColorSpec::new())?;
    Ok(())
}
