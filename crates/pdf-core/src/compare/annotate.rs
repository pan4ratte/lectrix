//! A copy of one of the documents compared with each difference marked and commented
//! (section 6.13), as FineReader saves one: the text this document has highlighted, with
//! a comment saying what the other has there; a sticky note where the other has text this
//! one lacks. Written to the interop profile like every annotation (section 5.1), in a
//! copy: the document compared is only read.

use std::path::Path;

use crate::annot::quads::Quad;
use crate::annot::{Body, MarkupKind, NewAnnotation, Rgb};
use crate::error::Result;
use crate::geometry::{Point, Rect};
use crate::ops::Operation;
use crate::save::SaveKind;
use crate::session::Session;
use crate::text::TextRange;

use super::{Difference, Kind, Side};

/// Which of the two documents compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The first (the old version).
    Old,
    /// The second (the new version).
    New,
}

/// The colours of the marks, by kind: light enough for a highlight to blend over text.
pub const INSERTED: Rgb = Rgb {
    r: 0.55,
    g: 0.87,
    b: 0.55,
};
pub const DELETED: Rgb = Rgb {
    r: 1.0,
    g: 0.6,
    b: 0.6,
};
pub const CHANGED: Rgb = Rgb {
    r: 1.0,
    g: 0.8,
    b: 0.35,
};

/// The longest quotation of the other document's text a comment gives.
const QUOTE_MAX: usize = 500;

fn color(kind: Kind) -> Rgb {
    match kind {
        Kind::Inserted => INSERTED,
        Kind::Deleted => DELETED,
        Kind::Changed => CHANGED,
    }
}

fn quote(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(QUOTE_MAX).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    format!("“{out}”")
}

/// The comment a difference gets in document `which`; `other` is the other document's
/// file name.
pub fn comment(d: &Difference, which: Which, other: &str) -> String {
    let theirs = match which {
        Which::Old => &d.new,
        Which::New => &d.old,
    };
    let what = match d.kind {
        Kind::Inserted => "Inserted.",
        Kind::Deleted => "Deleted.",
        Kind::Changed => "Changed.",
    };
    if theirs.text.is_empty() && theirs.rects.is_empty() {
        format!("{what} Not in {other}.")
    } else if theirs.text.is_empty() {
        // Pages compared as images: an area, no text.
        format!("{what} The area differs in {other}.")
    } else {
        format!("{what} In {other}: {}", quote(&theirs.text))
    }
}

/// The annotations that mark `differences` in document `which`, by `author`.
pub fn annotations(
    differences: &[&Difference],
    which: Which,
    other: &str,
    author: &str,
) -> Vec<NewAnnotation> {
    let mut out = Vec::new();
    for d in differences {
        let side: &Side = match which {
            Which::Old => &d.old,
            Which::New => &d.new,
        };
        let note = comment(d, which, other);
        let make = |page: usize, body: Body| NewAnnotation {
            page,
            body,
            color: color(d.kind),
            opacity: 1.0,
            author: author.to_owned(),
        };
        if !side.spans.is_empty() {
            // One highlight a page, the comment on the first.
            let mut pages: Vec<(usize, Vec<TextRange>)> = Vec::new();
            for s in &side.spans {
                let range = TextRange {
                    line: s.line,
                    start: s.start,
                    end: s.end,
                };
                match pages.last_mut() {
                    Some((page, ranges)) if *page == s.page => match ranges.last_mut() {
                        // The words of a line as one range, the spaces between them with
                        // them: one highlight along the line.
                        Some(last) if last.line == range.line && range.start >= last.end => {
                            last.end = range.end;
                        }
                        _ => ranges.push(range),
                    },
                    _ => pages.push((s.page, vec![range])),
                }
            }
            for (n, (page, ranges)) in pages.into_iter().enumerate() {
                out.push(make(
                    page,
                    Body::TextMarkup {
                        kind: MarkupKind::Highlight,
                        ranges,
                        note: (n == 0).then(|| note.clone()),
                    },
                ));
            }
        } else if !side.rects.is_empty() {
            // An area of a page compared as an image.
            for (n, r) in side.rects.iter().enumerate() {
                let rect = Rect::new(
                    f64::from(r.rect[0]),
                    f64::from(r.rect[1]),
                    f64::from(r.rect[2]),
                    f64::from(r.rect[3]),
                );
                out.push(make(
                    r.page,
                    Body::Markup {
                        kind: MarkupKind::Highlight,
                        quads: vec![Quad::from_view_rect(rect)],
                        note: (n == 0).then(|| note.clone()),
                    },
                ));
            }
        } else if let Some(caret) = side.caret {
            // The note's icon just above the line, its left edge where the text would go.
            let size = crate::annot::NOTE_SIZE;
            let at = Point::new(f64::from(caret.x), (f64::from(caret.y0) - size).max(0.0));
            out.push(make(caret.page, Body::Note { at, text: note }));
        }
    }
    out
}

/// Writes a copy of `source` (opened with `password`) to `target` with `annotations`
/// added, incrementally: what the file holds stays as it is, its signatures valid.
///
/// Their characters are counted in the page's text without its annotations, as Compare
/// reads it ([`Session::content_text`]); the text Lectrix's selection counts in has them,
/// so the highlights' quads are made here, from the same text.
pub fn write_copy(
    source: &Path,
    password: Option<&str>,
    target: &Path,
    annotations: Vec<NewAnnotation>,
) -> Result<()> {
    let (session, _) = Session::open(source, password)?;
    let result = (|| {
        let annotations = annotations
            .into_iter()
            .map(|a| with_quads(&session, a))
            .collect::<Result<Vec<_>>>()?;
        if !annotations.is_empty() {
            session.apply(Operation::AddAnnotations { annotations })?;
        }
        session.save(SaveKind::Incremental, Some(target.to_path_buf()))?;
        Ok(())
    })();
    session.close();
    result
}

/// A highlight over characters, as one over their quads.
fn with_quads(session: &Session, mut a: NewAnnotation) -> Result<NewAnnotation> {
    if let Body::TextMarkup { kind, ranges, note } = &a.body {
        let (list, _) = session.content_list(a.page)?;
        let quads = crate::text::range_quads(&list, ranges)?;
        a.body = Body::Markup {
            kind: *kind,
            quads,
            note: note.clone(),
        };
    }
    Ok(a)
}
