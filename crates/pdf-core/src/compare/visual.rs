//! Pages compared as images (section 6.13), for pages that have no text to compare (scans,
//! drawings): both rendered in grey at the same resolution, laid one over the other from
//! their top-left corners, and the areas where they differ found, small specks of
//! anti-aliasing left out.

use mupdf::{Colorspace, DisplayList, Matrix};

use crate::error::Result;

/// The resolution pages are compared at: 1.0 is 72 dpi.
const SCALE: f32 = 100.0 / 72.0;
/// How far apart two grey levels (0 to 255) must be to differ.
const THRESHOLD: u8 = 64;
/// Pixels are counted in cells of this many pixels a side...
const CELL: usize = 6;
/// ...and a cell differs when this many of its pixels do.
const CELL_MIN: usize = 4;
/// Differing cells this close (in cells) are one area.
const REACH: usize = 3;

/// A page as grey pixels, one byte each, rows `width` long, white where nothing is drawn.
pub struct Grey {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Grey {
    fn at(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            255
        }
    }
}

/// Renders a page's display list in grey at the comparison resolution.
pub fn render(list: &DisplayList) -> Result<Grey> {
    let pixmap = list.to_pixmap(
        &Matrix::new_scale(SCALE, SCALE),
        &Colorspace::device_gray(),
        false,
    )?;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let stride = usize::try_from(pixmap.stride()).unwrap_or(w);
    // Where the pixmap starts: a page's list starts at its top-left corner, (0, 0).
    let (ox, oy) = (pixmap.x().max(0) as usize, pixmap.y().max(0) as usize);
    let (width, height) = (w + ox, h + oy);
    let mut pixels = vec![255u8; width * height];
    let samples = pixmap.samples();
    for row in 0..h {
        let from = &samples[row * stride..row * stride + w];
        let at = (row + oy) * width + ox;
        pixels[at..at + w].copy_from_slice(from);
    }
    Ok(Grey {
        width,
        height,
        pixels,
    })
}

/// The areas where `a` and `b` differ, in page points (view space, zoom 1), largest first.
pub fn differences(a: &Grey, b: &Grey) -> Vec<[f32; 4]> {
    let (width, height) = (a.width.max(b.width), a.height.max(b.height));
    let (cols, rows) = (width.div_ceil(CELL), height.div_ceil(CELL));
    let mut cells = vec![false; cols * rows];
    for cy in 0..rows {
        for cx in 0..cols {
            let mut count = 0;
            'cell: for y in cy * CELL..((cy + 1) * CELL).min(height) {
                for x in cx * CELL..((cx + 1) * CELL).min(width) {
                    if a.at(x, y).abs_diff(b.at(x, y)) >= THRESHOLD {
                        count += 1;
                        if count >= CELL_MIN {
                            break 'cell;
                        }
                    }
                }
            }
            cells[cy * cols + cx] = count >= CELL_MIN;
        }
    }
    // Areas: differing cells joined when within REACH of each other.
    let mut seen = vec![false; cols * rows];
    let mut areas: Vec<[usize; 4]> = Vec::new();
    for start in 0..cells.len() {
        if !cells[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            let (cx, cy) = (i % cols, i / cols);
            x0 = x0.min(cx);
            y0 = y0.min(cy);
            x1 = x1.max(cx);
            y1 = y1.max(cy);
            for ny in cy.saturating_sub(REACH)..=(cy + REACH).min(rows - 1) {
                for nx in cx.saturating_sub(REACH)..=(cx + REACH).min(cols - 1) {
                    let n = ny * cols + nx;
                    if cells[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
        }
        areas.push([x0, y0, x1 + 1, y1 + 1]);
    }
    let to_points = |cells: usize| (cells * CELL) as f32 / SCALE;
    let mut out: Vec<[f32; 4]> = areas
        .into_iter()
        .map(|[x0, y0, x1, y1]| {
            [
                to_points(x0),
                to_points(y0),
                to_points(x1).min(width as f32 / SCALE),
                to_points(y1).min(height as f32 / SCALE),
            ]
        })
        .collect();
    out.sort_by(|p, q| {
        let area = |r: &[f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
        area(q).total_cmp(&area(p))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(width: usize, height: usize) -> Grey {
        Grey {
            width,
            height,
            pixels: vec![255; width * height],
        }
    }

    fn fill(g: &mut Grey, x0: usize, y0: usize, x1: usize, y1: usize, v: u8) {
        for y in y0..y1 {
            for x in x0..x1 {
                g.pixels[y * g.width + x] = v;
            }
        }
    }

    #[test]
    fn finds_changed_areas_and_ignores_specks() {
        let mut a = page(400, 300);
        let mut b = page(400, 300);
        // The same drawing on both.
        fill(&mut a, 10, 10, 100, 40, 0);
        fill(&mut b, 10, 10, 100, 40, 0);
        // A block only b has, and a speck of one pixel.
        fill(&mut b, 200, 150, 260, 200, 0);
        fill(&mut b, 350, 20, 351, 21, 0);
        // A grey shift too slight to count.
        fill(&mut a, 300, 250, 390, 290, 230);
        let found = differences(&a, &b);
        assert_eq!(found.len(), 1, "{found:?}");
        let r = found[0];
        let px = |v: f32| v * SCALE;
        assert!(
            px(r[0]) <= 200.0 && px(r[2]) >= 260.0 && px(r[1]) <= 150.0 && px(r[3]) >= 200.0,
            "{r:?}"
        );
    }

    #[test]
    fn a_larger_page_differs_where_the_other_has_nothing() {
        let a = page(100, 100);
        let mut b = page(100, 160);
        fill(&mut b, 10, 120, 90, 150, 0);
        assert_eq!(differences(&a, &b).len(), 1);
    }
}
