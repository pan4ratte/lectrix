//! Text box sizing. A text box keeps the width it was given and grows (or shrinks) in
//! height to fit its text, measured the way MuPDF lays it out in the appearance stream:
//! Helvetica widths, greedy breaks at spaces, explicit line breaks, 1.2 × the font size
//! per line (`write_variable_text` in MuPDF's `pdf-appearance.c`).

use mupdf::Font;

use crate::error::Result;
use crate::geometry::Rect;

/// Line height as a multiple of the font size (MuPDF's value).
const LINE_HEIGHT: f64 = 1.2;

/// Advance widths of Helvetica, in units of the font size.
struct Metrics {
    font: Font,
}

impl Metrics {
    fn new() -> Result<Metrics> {
        Ok(Metrics {
            font: Font::new("Helvetica")?,
        })
    }

    fn advance(&self, c: char) -> f64 {
        let glyph = self
            .font
            .encode_character(i32::try_from(u32::from(c)).unwrap_or(0))
            .unwrap_or(0);
        if glyph == 0 {
            // Not in Helvetica: MuPDF falls back to another font (CJK, for example).
            // Full-width is the safe guess for the height.
            return 1.0;
        }
        f64::from(self.font.advance_glyph(glyph).unwrap_or(0.5))
    }
}

/// The number of lines `text` takes in a box `width` points wide at `size` points.
fn line_count(metrics: &Metrics, text: &str, size: f64, width: f64) -> usize {
    let mut lines = 0;
    for paragraph in text.split('\n') {
        let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        lines += 1;
        // MuPDF's break_string: remember the last space; once the line is wider than the
        // box, break after that space.
        let mut x = 0.0;
        let mut space_at: Option<f64> = None;
        for c in paragraph.chars() {
            if c == ' ' {
                space_at = Some(x);
            }
            x += metrics.advance(c) * size;
            if let Some(sx) = space_at
                && x > width
            {
                lines += 1;
                // The next line starts after the space.
                x -= sx + metrics.advance(' ') * size;
                space_at = None;
            }
        }
    }
    // MuPDF writes no empty last line for a trailing line break.
    if text.ends_with('\n') && lines > 1 {
        lines -= 1;
    }
    lines.max(1)
}

/// The box (view space) for `text`: same top-left corner and width as `rect`; the height
/// that fits the text, or `rect`'s height if that is larger and `grow_only`.
pub(super) fn fit(rect: Rect, text: &str, size: f64, grow_only: bool) -> Result<Rect> {
    let metrics = Metrics::new()?;
    let lines = line_count(&metrics, text, size, rect.width());
    let needed = (lines as f64 * LINE_HEIGHT * size).ceil();
    let height = if grow_only {
        needed.max(rect.height())
    } else {
        needed
    };
    Ok(Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_wrapped_and_explicit_lines() {
        let m = Metrics::new().unwrap();
        // "Hello" in Helvetica 12 is about 28 pt wide.
        assert_eq!(line_count(&m, "Hello", 12.0, 200.0), 1);
        assert_eq!(line_count(&m, "Hello\nWorld", 12.0, 200.0), 2);
        assert_eq!(line_count(&m, "Hello\n", 12.0, 200.0), 1);
        assert_eq!(line_count(&m, "", 12.0, 200.0), 1);
        assert_eq!(line_count(&m, "Hello world", 12.0, 40.0), 2);
        assert_eq!(line_count(&m, "Hello world again", 12.0, 40.0), 3);
        // A word longer than the box is not broken (MuPDF clips it).
        assert_eq!(line_count(&m, "Unbreakableword", 12.0, 20.0), 1);
    }

    #[test]
    fn fits_height_to_text() {
        let r = Rect::new(10.0, 20.0, 210.0, 25.0);
        let one = fit(r, "One line", 12.0, false).unwrap();
        assert_eq!((one.x0, one.y0, one.x1), (10.0, 20.0, 210.0));
        assert!((one.height() - 15.0).abs() < 1.0);
        let two = fit(r, "One\nTwo", 12.0, false).unwrap();
        assert!(two.height() > one.height());
        let tall = Rect::new(10.0, 20.0, 210.0, 220.0);
        assert_eq!(fit(tall, "One", 12.0, true).unwrap(), tall);
        assert!(fit(tall, "One", 12.0, false).unwrap().height() < 20.0);
    }
}
