//! Printing (section 6.12, ADR 0028): where each page goes on a sheet of paper, and a job
//! that draws the pages, as they print, on a [`Target`] the platform provides (a printer
//! through the system's print spooler, or a PDF for CUPS).
//!
//! Everything here is in points (1/72 in) on the sheet, from its top-left corner, y down,
//! in the orientation the job prints it.

pub mod pdf;
pub mod raster;

use std::sync::Arc;

use mupdf::{DisplayList, Matrix};

use crate::error::{Error, Result};
use crate::protection::Printing;
use crate::session::Session;

/// A rectangle on a sheet, in points from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Area {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// The part of `self` inside `other`; empty (zero size) if they do not meet.
    pub fn intersect(&self, other: &Area) -> Area {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Area {
            x,
            y,
            width: (self.right().min(other.right()) - x).max(0.0),
            height: (self.bottom().min(other.bottom()) - y).max(0.0),
        }
    }
}

/// A sheet of paper as the printer has it for a job: its size, and the part it can print
/// on (inside its hardware margins), in the job's orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    pub width: f32,
    pub height: f32,
    pub printable: Area,
}

impl Sheet {
    /// A sheet with margins of `margin` points all round.
    pub fn with_margin(width: f32, height: f32, margin: f32) -> Sheet {
        let margin = margin.min(width / 4.0).min(height / 4.0).max(0.0);
        Sheet {
            width,
            height,
            printable: Area {
                x: margin,
                y: margin,
                width: width - 2.0 * margin,
                height: height - 2.0 * margin,
            },
        }
    }

    pub fn landscape(&self) -> bool {
        self.width > self.height
    }

    /// The sheet turned a quarter, as a printer turns paper for landscape: the content's
    /// top goes along the paper's left edge, so the margin at the paper's left becomes the
    /// top one.
    pub fn turned(&self) -> Sheet {
        let p = &self.printable;
        Sheet {
            width: self.height,
            height: self.width,
            printable: Area {
                // The paper's bottom margin is the content's left one, its left the top.
                x: self.height - p.bottom(),
                y: p.x,
                width: p.height,
                height: p.width,
            },
        }
    }

    /// This sheet in portrait (`false`) or landscape (`true`).
    pub fn oriented(&self, landscape: bool) -> Sheet {
        if self.landscape() == landscape || self.width == self.height {
            *self
        } else {
            self.turned()
        }
    }
}

/// How pages are sized on the sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sizing {
    /// Larger or smaller, to fill the printable area.
    Fit,
    /// As large as the page is.
    ActualSize,
    /// As large as the page is, unless it is larger than the printable area: then fitted.
    ShrinkOversized,
    /// At this scale (1.0 is the page's own size).
    Scale(f32),
}

/// Which way up sheets are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// As the pages are: a job of landscape pages prints on landscape sheets; in a job
    /// that mixes them, the sheets are portrait and landscape pages are turned on them.
    Auto,
    Portrait,
    Landscape,
}

/// Whether a page `width` x `height` is wider than tall (a square page is portrait).
pub fn is_landscape(width: f32, height: f32) -> bool {
    width > height
}

/// Whether the job's sheets are landscape, for pages of these sizes (width, height).
pub fn job_landscape(
    orientation: Orientation,
    pages: impl IntoIterator<Item = (f32, f32)>,
) -> bool {
    match orientation {
        Orientation::Portrait => false,
        Orientation::Landscape => true,
        Orientation::Auto => {
            let mut any = false;
            for (w, h) in pages {
                if !is_landscape(w, h) {
                    return false;
                }
                any = true;
            }
            any
        }
    }
}

/// Which way a page is turned on its sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    Upright,
    /// A quarter to the left: the page's top along the sheet's left edge.
    Left,
    /// A quarter to the right: the page's top along the sheet's right edge.
    Right,
}

/// Where a page goes on a sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub turn: Turn,
    /// Points on the sheet per point of the page.
    pub scale: f32,
    /// The page's box on the sheet, after turning and scaling.
    pub area: Area,
}

/// Where a page `width` x `height` (points, as shown on screen: after its `/Rotate`,
/// `rotation`) goes on `sheet`. With [`Orientation::Auto`], a page whose orientation
/// differs from the sheet's is turned: back the other way if its `/Rotate` turned it (so a
/// page turned for reading on screen prints upright), otherwise to the left.
pub fn place(
    width: f32,
    height: f32,
    rotation: u16,
    sheet: &Sheet,
    sizing: Sizing,
    orientation: Orientation,
) -> Placement {
    let turned = orientation == Orientation::Auto
        && sheet.width != sheet.height
        && width != height
        && is_landscape(width, height) != sheet.landscape();
    let turn = match (turned, rotation % 360) {
        (false, _) => Turn::Upright,
        (true, 270) => Turn::Right,
        (true, _) => Turn::Left,
    };
    let (w, h) = if turned {
        (height, width)
    } else {
        (width, height)
    };
    let p = sheet.printable;
    let fit = if w > 0.0 && h > 0.0 {
        (p.width / w).min(p.height / h)
    } else {
        1.0
    };
    // A tolerance for pages exactly the size of the printable area.
    let fits = w <= p.width + 0.01 && h <= p.height + 0.01;
    let (scale, fitted) = match sizing {
        Sizing::Fit => (fit, true),
        Sizing::ShrinkOversized if !fits => (fit, true),
        Sizing::ShrinkOversized | Sizing::ActualSize => (1.0, false),
        Sizing::Scale(s) => (s.max(0.01), false),
    };
    let (sw, sh) = (w * scale, h * scale);
    let (x, y) = if fitted {
        // Centred in the printable area.
        (p.x + (p.width - sw) / 2.0, p.y + (p.height - sh) / 2.0)
    } else {
        // Centred on the paper, as it would lie on it, but moved inside the printable area
        // where it fits there.
        let centre = |sheet: f32, size: f32, start: f32, room: f32| {
            let at = (sheet - size) / 2.0;
            if size <= room + 0.01 {
                at.clamp(start, start + room - size)
            } else {
                at
            }
        };
        (
            centre(sheet.width, sw, p.x, p.width),
            centre(sheet.height, sh, p.y, p.height),
        )
    };
    Placement {
        turn,
        scale,
        area: Area {
            x,
            y,
            width: sw,
            height: sh,
        },
    }
}

/// The transformation from a page's space (its display list's, whose bounds are
/// `bounds`) to points on the sheet, for `placement`.
pub fn page_to_sheet(bounds: mupdf::Rect, placement: &Placement) -> Matrix {
    let s = placement.scale;
    let a = placement.area;
    let (x0, y0) = (bounds.x0, bounds.y0);
    let (width, height) = (bounds.x1 - x0, bounds.y1 - y0);
    match placement.turn {
        // x' = X + s (x - x0), y' = Y + s (y - y0).
        Turn::Upright => Matrix::new(s, 0.0, 0.0, s, a.x - s * x0, a.y - s * y0),
        // The page's top edge along the box's left edge, its left edge along the bottom:
        // x' = X + s (y - y0), y' = Y + s (x0 + width - x).
        Turn::Left => Matrix::new(0.0, -s, s, 0.0, a.x - s * y0, a.y + s * (x0 + width)),
        // The page's top edge along the box's right edge, its left edge along the top:
        // x' = X + s (y0 + height - y), y' = Y + s (x - x0).
        Turn::Right => Matrix::new(0.0, s, -s, 0.0, a.x + s * (y0 + height), a.y - s * x0),
    }
}

/// Two-sided printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Duplex {
    #[default]
    OneSided,
    /// Pages turned as a book's: bound along the sheet's long edge.
    LongEdge,
    /// Pages turned as a notepad's: bound along the short edge.
    ShortEdge,
}

/// What a printer is told about a job.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintSetup {
    /// The paper, by the printer's own id for it; `None` for the printer's current one.
    pub paper: Option<i32>,
    pub landscape: bool,
    pub duplex: Duplex,
    pub grey: bool,
    pub copies: u16,
    pub collate: bool,
    /// The job's name in the print queue: the document's file name.
    pub title: String,
    /// Prints into this file instead of on paper (Windows' printers that write files, such
    /// as Microsoft Print to PDF, then ask for no name). For tests.
    pub output: Option<std::path::PathBuf>,
    /// The highest resolution pages are drawn at (dots per inch): the printer's own when
    /// it is lower.
    pub max_dpi: f32,
}

/// Highest resolution pages are drawn at for a printer: enough for text as sharp as
/// laser printers make it, without drawing a page at 1200 dpi (130 MB a sheet of A4).
pub const MAX_DPI: f32 = 600.0;

/// Resolution for files that allow printing at low resolution only (as Acrobat prints
/// them).
pub const LOW_RESOLUTION_DPI: f32 = 150.0;

/// Where a job's sheets are drawn: a printer, or a file.
pub trait Target {
    /// The sheet pages go on.
    fn sheet(&self) -> Sheet;
    /// The printer makes the copies (and collates them) itself: the job sends each sheet
    /// once.
    fn makes_copies(&self) -> bool;
    fn begin_sheet(&mut self) -> Result<()>;
    /// Draws `list` through `ctm` (page space to sheet points), clipped to `clip`, in
    /// greys if `grey`.
    fn draw(&mut self, list: &DisplayList, ctm: &Matrix, clip: Area, grey: bool) -> Result<()>;
    fn end_sheet(&mut self) -> Result<()>;
    /// Ends the job: the printer gets it.
    fn finish(self: Box<Self>) -> Result<()>;
    /// Drops the job: nothing is printed.
    fn abort(self: Box<Self>);
}

/// What a job prints.
#[derive(Debug, Clone)]
pub struct Job {
    /// The pages, in the order they print (0-based; the same page may come twice).
    pub pages: Vec<usize>,
    pub sizing: Sizing,
    pub orientation: Orientation,
    pub annotations: bool,
    pub grey: bool,
    pub copies: u16,
    pub collate: bool,
    pub duplex: Duplex,
}

/// The sheets a job prints, each a page or a blank back (`None`), with copies made here
/// when the printer does not make them. A blank back keeps the next copy from starting on
/// the back of the last sheet of the one before, when printing on both sides.
pub fn sheets(job: &Job, printer_copies: bool) -> Vec<Option<usize>> {
    let copies = if printer_copies {
        1
    } else {
        usize::from(job.copies.max(1))
    };
    let two_sided = job.duplex != Duplex::OneSided;
    let mut out = Vec::with_capacity(job.pages.len() * copies);
    if job.collate || copies == 1 {
        for copy in 0..copies {
            out.extend(job.pages.iter().map(|&p| Some(p)));
            if two_sided && copy + 1 < copies && job.pages.len() % 2 == 1 {
                out.push(None);
            }
        }
    } else {
        for &page in &job.pages {
            for _ in 0..copies {
                out.push(Some(page));
                if two_sided {
                    out.push(None);
                }
            }
        }
        if two_sided {
            out.pop();
        }
    }
    out
}

/// The resolution a document allows its pages to be printed at.
pub fn allowed_dpi(printing: Printing) -> Result<f32> {
    match printing {
        Printing::NotAllowed => Err(Error::PrintNotAllowed),
        Printing::LowResolution => Ok(LOW_RESOLUTION_DPI),
        Printing::HighResolution => Ok(MAX_DPI),
    }
}

/// Prints `job` from `session` on `target`, reporting each sheet done (`progress(done,
/// total)`, which returns false to stop: the job is then dropped and the call fails with
/// [`Error::Cancelled`]).
pub fn print(
    session: &Session,
    job: &Job,
    mut target: Box<dyn Target>,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<()> {
    let sheets = sheets(job, target.makes_copies());
    let total = sheets.len();
    let sheet = target.sheet();
    let mut stop = false;
    let result = (|| -> Result<()> {
        let rotations: Vec<u16> = session
            .print_facts()?
            .pages
            .iter()
            .map(|p| p.rotation)
            .collect();
        // A page's display list, kept while the same page prints again (copies).
        let mut last: Option<(usize, Arc<DisplayList>)> = None;
        for (done, page) in sheets.iter().enumerate() {
            if !progress(done, total) {
                stop = true;
                return Err(Error::Cancelled);
            }
            target.begin_sheet()?;
            if let Some(page) = *page {
                let list = match &last {
                    Some((p, list)) if *p == page => list.clone(),
                    _ => {
                        let (list, _) = session.print_list(page, job.annotations)?;
                        last = Some((page, list.clone()));
                        list
                    }
                };
                let bounds = list.bounds();
                let placement = place(
                    bounds.width(),
                    bounds.height(),
                    rotations.get(page).copied().unwrap_or(0),
                    &sheet,
                    job.sizing,
                    job.orientation,
                );
                let ctm = page_to_sheet(bounds, &placement);
                let clip = placement.area.intersect(&sheet.printable);
                if clip.width > 0.0 && clip.height > 0.0 {
                    target.draw(&list, &ctm, clip, job.grey)?;
                }
            }
            target.end_sheet()?;
        }
        if !progress(total, total) {
            stop = true;
            return Err(Error::Cancelled);
        }
        Ok(())
    })();
    match result {
        Ok(()) => target.finish(),
        Err(e) => {
            target.abort();
            Err(if stop { Error::Cancelled } else { e })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: (f32, f32) = (595.0, 842.0);

    fn a4(margin: f32) -> Sheet {
        Sheet::with_margin(A4.0, A4.1, margin)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn fit_fills_the_printable_area_and_centres_the_page() {
        let sheet = a4(18.0);
        // A Letter page on A4: limited by the width.
        let p = place(612.0, 792.0, 0, &sheet, Sizing::Fit, Orientation::Auto);
        assert_eq!(p.turn, Turn::Upright);
        assert!(close(p.area.width, 595.0 - 36.0));
        assert!(close(p.area.x, 18.0));
        assert!(close(p.area.y + p.area.height / 2.0, 421.0));
        // A small page grows.
        let p = place(300.0, 400.0, 0, &sheet, Sizing::Fit, Orientation::Auto);
        assert!(p.scale > 1.0);
    }

    #[test]
    fn actual_size_keeps_the_size_and_centres_on_the_paper() {
        let sheet = a4(18.0);
        let p = place(
            400.0,
            500.0,
            0,
            &sheet,
            Sizing::ActualSize,
            Orientation::Auto,
        );
        assert_eq!(p.scale, 1.0);
        assert!(close(p.area.x, (595.0 - 400.0) / 2.0));
        assert!(close(p.area.y, (842.0 - 500.0) / 2.0));
        // A page the size of the paper stays where the paper is, and is cut at the margins.
        let p = place(A4.0, A4.1, 0, &sheet, Sizing::ActualSize, Orientation::Auto);
        assert_eq!(
            p.area,
            Area {
                x: 0.0,
                y: 0.0,
                width: 595.0,
                height: 842.0
            }
        );
    }

    #[test]
    fn actual_size_moves_a_page_that_fits_inside_uneven_margins() {
        // A printer with a wide bottom margin.
        let sheet = Sheet {
            width: 595.0,
            height: 842.0,
            printable: Area {
                x: 10.0,
                y: 10.0,
                width: 575.0,
                height: 772.0,
            },
        };
        let p = place(
            500.0,
            770.0,
            0,
            &sheet,
            Sizing::ActualSize,
            Orientation::Auto,
        );
        assert!(p.area.bottom() <= sheet.printable.bottom() + 0.01);
        assert!(p.area.y >= sheet.printable.y - 0.01);
    }

    #[test]
    fn shrink_oversized_shrinks_only_pages_too_large() {
        let sheet = a4(18.0);
        let small = place(
            400.0,
            500.0,
            0,
            &sheet,
            Sizing::ShrinkOversized,
            Orientation::Auto,
        );
        assert_eq!(small.scale, 1.0);
        let large = place(
            A4.0,
            A4.1,
            0,
            &sheet,
            Sizing::ShrinkOversized,
            Orientation::Auto,
        );
        assert!(large.scale < 1.0);
        assert!(close(large.area.width, 559.0) || close(large.area.height, 806.0));
    }

    #[test]
    fn custom_scale() {
        let sheet = a4(18.0);
        let p = place(
            400.0,
            500.0,
            0,
            &sheet,
            Sizing::Scale(0.5),
            Orientation::Auto,
        );
        assert!(close(p.area.width, 200.0) && close(p.area.height, 250.0));
    }

    #[test]
    fn auto_turns_pages_whose_orientation_differs_from_the_sheet() {
        let sheet = a4(18.0);
        let p = place(842.0, 595.0, 0, &sheet, Sizing::Fit, Orientation::Auto);
        assert_eq!(p.turn, Turn::Left);
        assert!(p.area.height > p.area.width);
        // A page that its /Rotate turned to the left for the screen turns back to the
        // right, upright.
        let p = place(842.0, 595.0, 270, &sheet, Sizing::Fit, Orientation::Auto);
        assert_eq!(p.turn, Turn::Right);
        let p = place(842.0, 595.0, 90, &sheet, Sizing::Fit, Orientation::Auto);
        assert_eq!(p.turn, Turn::Left);
        // Asked for portrait, a landscape page is not turned but made smaller.
        let p = place(842.0, 595.0, 0, &sheet, Sizing::Fit, Orientation::Portrait);
        assert_eq!(p.turn, Turn::Upright);
        assert!(p.area.width > p.area.height);
    }

    #[test]
    fn job_orientation() {
        let landscape = [(842.0, 595.0), (792.0, 612.0)];
        let mixed = [(842.0, 595.0), (595.0, 842.0)];
        assert!(job_landscape(Orientation::Auto, landscape));
        assert!(!job_landscape(Orientation::Auto, mixed));
        assert!(!job_landscape(Orientation::Auto, []));
        assert!(job_landscape(Orientation::Landscape, mixed));
        assert!(!job_landscape(Orientation::Portrait, landscape));
    }

    #[test]
    fn a_turned_sheet_keeps_its_margins_on_the_same_edges_of_the_paper() {
        let sheet = Sheet {
            width: 595.0,
            height: 842.0,
            // Left 10, top 20, right 30, bottom 40.
            printable: Area {
                x: 10.0,
                y: 20.0,
                width: 555.0,
                height: 782.0,
            },
        };
        let t = sheet.turned();
        assert_eq!((t.width, t.height), (842.0, 595.0));
        // Content top = paper left (10), content left = paper bottom (40).
        assert!(close(t.printable.y, 10.0));
        assert!(close(t.printable.x, 40.0));
        assert!(close(t.printable.width, 782.0));
        assert!(close(t.printable.height, 555.0));
        assert_eq!(sheet.oriented(false), sheet);
        assert_eq!(sheet.oriented(true), t);
    }

    #[test]
    fn page_to_sheet_maps_the_page_corners_into_its_box() {
        let bounds = mupdf::Rect::new(0.0, 0.0, 200.0, 100.0);
        let apply = |m: &Matrix, x: f32, y: f32| (m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f);
        let flat = Placement {
            turn: Turn::Upright,
            scale: 2.0,
            area: Area {
                x: 10.0,
                y: 20.0,
                width: 400.0,
                height: 200.0,
            },
        };
        let m = page_to_sheet(bounds, &flat);
        assert_eq!(apply(&m, 0.0, 0.0), (10.0, 20.0));
        assert_eq!(apply(&m, 200.0, 100.0), (410.0, 220.0));
        let turned = Placement {
            turn: Turn::Left,
            scale: 1.0,
            area: Area {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 200.0,
            },
        };
        let m = page_to_sheet(bounds, &turned);
        // The page's top-left corner at the box's bottom-left; its top-right at the top-left.
        assert_eq!(apply(&m, 0.0, 0.0), (10.0, 220.0));
        assert_eq!(apply(&m, 200.0, 0.0), (10.0, 20.0));
        assert_eq!(apply(&m, 200.0, 100.0), (110.0, 20.0));
        let right = Placement {
            turn: Turn::Right,
            ..turned
        };
        let m = page_to_sheet(bounds, &right);
        // The page's top-left corner at the box's top-right; its bottom-left at the top-left.
        assert_eq!(apply(&m, 0.0, 0.0), (110.0, 20.0));
        assert_eq!(apply(&m, 0.0, 100.0), (10.0, 20.0));
        assert_eq!(apply(&m, 200.0, 100.0), (10.0, 220.0));
    }

    fn job(pages: Vec<usize>, copies: u16, collate: bool, duplex: Duplex) -> Job {
        Job {
            pages,
            sizing: Sizing::Fit,
            orientation: Orientation::Auto,
            annotations: true,
            grey: false,
            copies,
            collate,
            duplex,
        }
    }

    #[test]
    fn copies_made_here() {
        let s = |j: &Job| sheets(j, false);
        assert_eq!(
            s(&job(vec![0, 1], 2, true, Duplex::OneSided)),
            [Some(0), Some(1), Some(0), Some(1)]
        );
        assert_eq!(
            s(&job(vec![0, 1], 2, false, Duplex::OneSided)),
            [Some(0), Some(0), Some(1), Some(1)]
        );
        // Two-sided, an odd number of pages: each copy starts on a sheet of its own.
        assert_eq!(
            s(&job(vec![0, 1, 2], 2, true, Duplex::LongEdge)),
            [Some(0), Some(1), Some(2), None, Some(0), Some(1), Some(2)]
        );
        assert_eq!(
            s(&job(vec![0, 1], 2, false, Duplex::LongEdge)),
            [Some(0), None, Some(0), None, Some(1), None, Some(1)]
        );
        // The printer makes the copies: each page once.
        assert_eq!(
            sheets(&job(vec![0, 1], 3, true, Duplex::OneSided), true),
            [Some(0), Some(1)]
        );
    }

    #[test]
    fn permissions_set_the_resolution() {
        assert!(matches!(
            allowed_dpi(Printing::NotAllowed),
            Err(Error::PrintNotAllowed)
        ));
        assert_eq!(allowed_dpi(Printing::LowResolution).unwrap(), 150.0);
        assert_eq!(allowed_dpi(Printing::HighResolution).unwrap(), 600.0);
    }
}
