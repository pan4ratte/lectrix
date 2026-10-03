//! Structured text: per-page character geometry for selection, and text search.
//!
//! The frontend receives each page's text once per revision and does hit-testing and
//! selection locally (AGENTS.md section 3). All coordinates are in view space at zoom 1
//! (MuPDF page space: origin top-left of the visible page, y down, `/Rotate` applied).

use mupdf::text_page::TextBlockType;
use mupdf::{DisplayList, Quad, TextPage, TextPageFlags};

use crate::error::Result;
use crate::geometry::{Point, Rect};

/// One line of text: characters sharing a baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// The line's characters, one Unicode scalar value per entry in `boxes`.
    pub text: String,
    /// Axis-aligned bounds of each character: `[x0, y0, x1, y1]` per character.
    pub boxes: Vec<[f32; 4]>,
    /// Bounds of the whole line.
    pub bbox: [f32; 4],
    /// Index of the block (roughly a paragraph) the line belongs to, in reading order.
    pub block: u32,
    /// True for vertical writing mode.
    pub vertical: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub lines: Vec<TextLine>,
}

impl PageText {
    /// The page's text with one line break between lines and a blank line between blocks.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        let mut last_block = None;
        for line in &self.lines {
            if let Some(block) = last_block {
                out.push('\n');
                if block != line.block {
                    out.push('\n');
                }
            }
            out.push_str(&line.text);
            last_block = Some(line.block);
        }
        out
    }
}

/// Flags for extraction. Ligatures are expanded (so "ﬁ" selects and searches as "fi"),
/// and whitespace is normalized the way MuPDF does by default.
fn flags() -> TextPageFlags {
    TextPageFlags::empty()
}

/// Extracts the text geometry of a page from its display list.
pub fn page_text(list: &DisplayList) -> Result<PageText> {
    let page = list.to_text_page(flags())?;
    Ok(collect(&page))
}

/// Characters `start..end` of line `line`, counted as in [`page_text`] (the frontend's
/// selection sends these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// One quad per range, in the text's own direction (view space): the first character's
/// left edge and the last character's right edge, from MuPDF's character quads. Ranges
/// outside the page's text are an error.
pub fn range_quads(
    list: &DisplayList,
    ranges: &[TextRange],
) -> Result<Vec<crate::annot::quads::Quad>> {
    let page = list.to_text_page(flags())?;
    let mut lines: Vec<Vec<Quad>> = Vec::new();
    for block in page.blocks() {
        if block.r#type() != TextBlockType::Text {
            continue;
        }
        for line in block.lines() {
            let quads: Vec<Quad> = line.chars().map(|ch| ch.quad()).collect();
            if !quads.is_empty() {
                lines.push(quads);
            }
        }
    }
    let point = |p: mupdf::Point| Point::new(f64::from(p.x), f64::from(p.y));
    ranges
        .iter()
        .map(|r| {
            let chars = lines
                .get(r.line)
                .filter(|l| r.start < r.end && r.end <= l.len())
                .ok_or_else(|| {
                    crate::Error::InvalidArgument("the selected text is not on the page".into())
                })?;
            let (first, last) = (&chars[r.start], &chars[r.end - 1]);
            Ok(crate::annot::quads::Quad {
                ul: point(first.ul),
                ll: point(first.ll),
                ur: point(last.ur),
                lr: point(last.lr),
            })
        })
        .collect()
}

fn collect(page: &TextPage) -> PageText {
    let mut lines = Vec::new();
    for (block_index, block) in page.blocks().enumerate() {
        if block.r#type() != TextBlockType::Text {
            continue;
        }
        let block_index = u32::try_from(block_index).unwrap_or(u32::MAX);
        for line in block.lines() {
            let mut text = String::new();
            let mut boxes = Vec::new();
            for ch in line.chars() {
                // Characters MuPDF could not map to Unicode come through as U+FFFD; keep
                // them so boxes and characters stay aligned.
                let c = ch.char().unwrap_or('\u{FFFD}');
                text.push(c);
                boxes.push(round_box(quad_bounds(&ch.quad())));
            }
            if boxes.is_empty() {
                continue;
            }
            let b = line.bounds();
            lines.push(TextLine {
                text,
                boxes,
                bbox: round_box(Rect::new(
                    f64::from(b.x0),
                    f64::from(b.y0),
                    f64::from(b.x1),
                    f64::from(b.y1),
                )),
                block: block_index,
                vertical: matches!(line.wmode(), mupdf::WriteMode::Vertical),
            });
        }
    }
    PageText { lines }
}

fn quad_bounds(q: &Quad) -> Rect {
    Rect::bounding([q.ul, q.ur, q.ll, q.lr].map(|p| Point::new(f64::from(p.x), f64::from(p.y))))
}

/// Rounds to 1/100 pt: plenty for hit-testing, and it keeps the IPC payload small.
fn round_box(r: Rect) -> [f32; 4] {
    let round = |v: f64| ((v * 100.0).round() / 100.0) as f32;
    [round(r.x0), round(r.y0), round(r.x1), round(r.y1)]
}

/// One search hit: the quads of the matched text (several when it spans lines).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub quads: Vec<[f32; 8]>,
}

/// The most hits returned for one page; MuPDF's own search stops there too.
const MAX_HITS_PER_PAGE: usize = 500;

/// Finds every occurrence of `needle` on a page (case-insensitive, as MuPDF's search is).
pub fn search_page(list: &DisplayList, needle: &str) -> Result<Vec<SearchHit>> {
    if needle.trim().is_empty() {
        return Ok(Vec::new());
    }
    let page = list.to_text_page(flags())?;
    let mut hits: Vec<SearchHit> = Vec::new();
    page.search_cb(needle, &mut hits, |hits, quads| {
        hits.push(SearchHit {
            quads: quads
                .iter()
                .map(|q| {
                    let r = |v: f32| (v * 100.0).round() / 100.0;
                    [
                        r(q.ul.x),
                        r(q.ul.y),
                        r(q.ur.x),
                        r(q.ur.y),
                        r(q.ll.x),
                        r(q.ll.y),
                        r(q.lr.x),
                        r(q.lr.y),
                    ]
                })
                .collect(),
        });
        if hits.len() >= MAX_HITS_PER_PAGE {
            mupdf::text_page::SearchHitResponse::AbortSearch
        } else {
            mupdf::text_page::SearchHitResponse::ContinueSearch
        }
    })?;
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::display_list;
    use crate::testgen::{MARKER, SampleSpec, sample_document};

    #[test]
    fn extracts_lines_with_one_box_per_character() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let text = page_text(&display_list(&doc, 1).unwrap()).unwrap();
        assert_eq!(text.lines[0].text, "Page 2");
        for line in &text.lines {
            assert_eq!(line.text.chars().count(), line.boxes.len());
        }
        let marker = text
            .lines
            .iter()
            .find(|l| l.text.starts_with(MARKER))
            .unwrap();
        // Characters run left to right on a horizontal line, inside the line's bounds.
        let first = marker.boxes[0];
        let last = *marker.boxes.last().unwrap();
        assert!(first[0] < last[0]);
        assert!(first[0] >= marker.bbox[0] - 0.01 && last[2] <= marker.bbox[2] + 0.01);
        assert!(text.plain_text().contains(MARKER));
    }

    #[test]
    fn searches_case_insensitively() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = display_list(&doc, 0).unwrap();
        let hits = search_page(&list, "QUICK brown").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].quads.len(), 1);
        assert!(search_page(&list, "not on this page").unwrap().is_empty());
        assert!(search_page(&list, "  ").unwrap().is_empty());
    }

    #[test]
    fn rotated_page_text_is_in_view_space() {
        let doc = sample_document(&SampleSpec {
            rotate: 90,
            ..SampleSpec::default()
        })
        .unwrap();
        let text = page_text(&display_list(&doc, 0).unwrap()).unwrap();
        let line = &text.lines[0];
        assert!(line.vertical || line.bbox[3] - line.bbox[1] > line.bbox[2] - line.bbox[0]);
        // View space of a 90-degree page is 792 wide and 612 high.
        for b in &line.boxes {
            assert!(b[0] >= 0.0 && b[2] <= 792.0 && b[1] >= 0.0 && b[3] <= 612.0);
        }
    }
}
