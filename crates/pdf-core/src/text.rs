//! Structured text: per-page character geometry for selection, and text search.
//!
//! The frontend receives each page's text once per revision and does hit-testing and
//! selection locally (AGENTS.md section 3). All coordinates are in view space at zoom 1
//! (MuPDF page space: origin top-left of the visible page, y down, `/Rotate` applied).

use mupdf::text_page::TextBlockType;
use mupdf::{DisplayList, Quad, TextPage, TextPageFlags};

use crate::annot::quads::Quad as MarkupQuad;
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

/// The quads of each range, in the text's own orientation (view space), from MuPDF's
/// character quads: see [`run_quads`]. Left-to-right text gives one quad per range.
/// Ranges outside the page's text are an error.
pub fn range_quads(list: &DisplayList, ranges: &[TextRange]) -> Result<Vec<MarkupQuad>> {
    let page = list.to_text_page(flags())?;
    let point = |p: mupdf::Point| Point::new(f64::from(p.x), f64::from(p.y));
    let mut lines: Vec<Vec<MarkupQuad>> = Vec::new();
    for block in page.blocks() {
        if block.r#type() != TextBlockType::Text {
            continue;
        }
        for line in block.lines() {
            let quads: Vec<MarkupQuad> = line
                .chars()
                .map(|ch| {
                    let q = ch.quad();
                    MarkupQuad {
                        ul: point(q.ul),
                        ur: point(q.ur),
                        ll: point(q.ll),
                        lr: point(q.lr),
                    }
                })
                .collect();
            if !quads.is_empty() {
                lines.push(quads);
            }
        }
    }
    let mut out = Vec::with_capacity(ranges.len());
    for r in ranges {
        let chars = lines
            .get(r.line)
            .filter(|l| r.start < r.end && r.end <= l.len())
            .ok_or_else(|| {
                crate::Error::InvalidArgument("the selected text is not on the page".into())
            })?;
        out.extend(run_quads(chars, r.start, r.end));
    }
    Ok(out)
}

/// One quad for each run of the characters `start..end` of a line that lie side by side
/// on the page: from the edge its first character on the page starts at to the edge its
/// last one ends at, so every corner keeps its name (upper and lower as the glyphs stand,
/// left to right as they advance).
///
/// The quad is a band square to the line, as high as all the run's characters together.
/// Its corners were the first character's left corners and the last one's right corners,
/// so a run that ended on a superscript (raised and smaller) or a subscript gave a quad
/// that slanted up or down along the line.
///
/// MuPDF lists a line's characters in reading order, each with an upright quad of its
/// own. In left-to-right text the range is one run from its first character to its last.
/// In right-to-left text the characters run leftwards, so the run goes from the range's
/// last character to its first: taking the first character's left edge and the last one's
/// right edge there, as for left-to-right text, gave a mirrored quad that fell short by
/// a character at each end. A range that crosses from one direction into the other (a
/// Latin word in a Hebrew line) is several runs, with the characters between them left
/// out.
fn run_quads(chars: &[MarkupQuad], start: usize, end: usize) -> Vec<MarkupQuad> {
    // The direction glyphs advance in: every character of a line has the same one.
    let (dx, dy) = chars
        .iter()
        .map(|q| (q.lr.x - q.ll.x, q.lr.y - q.ll.y))
        .find(|(x, y)| x.hypot(*y) > 1e-6)
        .map_or((1.0, 0.0), |(x, y)| (x / x.hypot(y), y / x.hypot(y)));
    let along = |p: Point| p.x * dx + p.y * dy;
    // Square to that, towards the glyphs' tops: a quarter turn from the advance, to the
    // side their upper corners are on (view space has y down).
    let (ux, uy) = {
        let side = chars
            .iter()
            .map(|q| (q.ul.x - q.ll.x) * dy - (q.ul.y - q.ll.y) * dx)
            .find(|s| s.abs() > 1e-6)
            .unwrap_or(1.0);
        if side < 0.0 { (-dy, dx) } else { (dy, -dx) }
    };
    let rise = |p: Point| p.x * ux + p.y * uy;
    let span = |q: &MarkupQuad| {
        let (a, b) = (along(q.ll), along(q.lr));
        (a.min(b), a.max(b))
    };
    let mut by_place: Vec<usize> = (0..chars.len()).collect();
    by_place.sort_by(|a, b| span(&chars[*a]).0.total_cmp(&span(&chars[*b]).0));

    let whole = |members: &[usize]| -> Option<MarkupQuad> {
        // Characters of no width (a space MuPDF put in) have no place to mark, and their
        // boxes need not stand on the line.
        let wide: Vec<usize> = members
            .iter()
            .copied()
            .filter(|i| {
                let (from, to) = span(&chars[*i]);
                to - from > 1e-6
            })
            .collect();
        let members = if wide.is_empty() { members } else { &wide };
        if members.is_empty() {
            return None;
        }
        let corners: Vec<Point> = members
            .iter()
            .flat_map(|i| {
                let q = &chars[*i];
                [q.ul, q.ur, q.ll, q.lr]
            })
            .collect();
        let extent = |f: &dyn Fn(Point) -> f64| {
            corners
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(f(*p)), hi.max(f(*p)))
                })
        };
        let (left, right) = extent(&along);
        let (bottom, top) = extent(&rise);
        let at = |a: f64, r: f64| Point::new(a * dx + r * ux, a * dy + r * uy);
        Some(MarkupQuad {
            ul: at(left, top),
            ur: at(right, top),
            ll: at(left, bottom),
            lr: at(right, bottom),
        })
    };

    // A character outside the range ends a run only if it stands between two of the
    // range's: an accent set over a letter before the range does not.
    let outside_between = |a: f64, b: f64| {
        chars.iter().enumerate().any(|(i, q)| {
            let (from, to) = span(q);
            let middle = (from + to) / 2.0;
            !(start..end).contains(&i) && to - from > 1e-6 && middle > a && middle < b
        })
    };
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut reach = f64::NEG_INFINITY;
    for i in by_place {
        if !(start..end).contains(&i) {
            continue;
        }
        let (from, to) = span(&chars[i]);
        match runs.last_mut() {
            Some(run) if !outside_between(reach, from) => run.push(i),
            _ => runs.push(vec![i]),
        }
        reach = reach.max(to);
    }
    let quads: Vec<MarkupQuad> = runs
        .iter()
        .filter_map(|run| whole(run))
        .filter(MarkupQuad::is_valid)
        .collect();
    if quads.is_empty() {
        // Nothing with any width (a space alone): the range as it is, for the caller to
        // refuse.
        let all: Vec<usize> = (start..end).collect();
        return whole(&all).into_iter().collect();
    }
    quads
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

/// A word of the page's text that contains a search's needle, as the text has it, and how
/// many times the page has it: what the search bar suggests (section 6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundWord {
    pub text: String,
    pub count: u32,
}

/// Finds every occurrence of `needle` on a page (case-insensitive, as MuPDF's search is).
pub fn search_page(list: &DisplayList, needle: &str) -> Result<Vec<SearchHit>> {
    Ok(search_page_words(list, needle)?.0)
}

/// [`search_page`], and the words around the occurrences of `needle` within a line (see
/// [`words_containing`]), from the same text page.
pub fn search_page_words(
    list: &DisplayList,
    needle: &str,
) -> Result<(Vec<SearchHit>, Vec<FoundWord>)> {
    if needle.trim().is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let page = list.to_text_page(flags())?;
    let words = words_containing(&collect(&page), needle);
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
    Ok((hits, words))
}

/// The words of `text` that contain `needle`, with how many times each is there: every
/// occurrence of the needle within a line (case-insensitive, any run of spaces in the
/// needle matching one space, as MuPDF's search reads it), taken out to the word
/// boundaries on both sides. An occurrence that runs from one line onto the next is not
/// counted. Each word of the text counts once, however often the needle is in it.
pub fn words_containing(text: &PageText, needle: &str) -> Vec<FoundWord> {
    let mut wanted: Vec<char> = Vec::new();
    for c in needle.trim().chars().map(fold) {
        if !(c == ' ' && wanted.last() == Some(&' ')) {
            wanted.push(c);
        }
    }
    let mut counts: Vec<FoundWord> = Vec::new();
    if wanted.is_empty() {
        return counts;
    }
    for line in &text.lines {
        let chars: Vec<char> = line.text.chars().collect();
        let folded: Vec<char> = chars.iter().copied().map(fold).collect();
        let mut last_start = None;
        let mut at = 0;
        while at + wanted.len() <= folded.len() {
            if folded[at..at + wanted.len()] != wanted[..] {
                at += 1;
                continue;
            }
            let mut start = at;
            while start > 0 && in_word(&chars, start - 1) {
                start -= 1;
            }
            let mut end = at + wanted.len();
            while end < chars.len() && in_word(&chars, end) {
                end += 1;
            }
            at += wanted.len();
            if last_start == Some(start) {
                continue;
            }
            last_start = Some(start);
            let word: String = chars[start..end].iter().collect();
            match counts.iter_mut().find(|w| w.text == word) {
                Some(found) => found.count += 1,
                None => counts.push(FoundWord {
                    text: word,
                    count: 1,
                }),
            }
        }
    }
    counts
}

/// A character as a search compares it: lower case, and any space as one space.
fn fold(c: char) -> char {
    if c.is_whitespace() {
        ' '
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Whether `chars[i]` is part of a word: a letter, a digit or a combining mark, or an
/// apostrophe between two of those ("don't").
fn in_word(chars: &[char], i: usize) -> bool {
    let wordy = |c: char| c.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(&c);
    match chars[i] {
        '\'' | '\u{2019}' => {
            i > 0 && i + 1 < chars.len() && wordy(chars[i - 1]) && wordy(chars[i + 1])
        }
        c => wordy(c),
    }
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

    /// An upright character box on a horizontal line (view space, y down).
    fn glyph(x0: f64, x1: f64) -> MarkupQuad {
        MarkupQuad {
            ul: Point::new(x0, 10.0),
            ur: Point::new(x1, 10.0),
            ll: Point::new(x0, 20.0),
            lr: Point::new(x1, 20.0),
        }
    }

    #[test]
    fn a_range_runs_from_its_first_edge_on_the_page_to_its_last() {
        let ltr = [
            glyph(0.0, 5.0),
            glyph(5.0, 10.0),
            glyph(10.0, 15.0),
            glyph(15.0, 20.0),
        ];
        assert_eq!(run_quads(&ltr, 1, 3), [glyph(5.0, 15.0)]);
        // Right-to-left text: reading order runs leftwards, each character upright. The
        // quad still names its corners as the glyphs stand, and covers every character.
        let rtl = [
            glyph(15.0, 20.0),
            glyph(10.0, 15.0),
            glyph(5.0, 10.0),
            glyph(0.0, 5.0),
        ];
        assert_eq!(run_quads(&rtl, 0, 3), [glyph(5.0, 20.0)]);
        assert_eq!(run_quads(&rtl, 0, 4), [glyph(0.0, 20.0)]);
        // A Latin word in a Hebrew line: two Hebrew letters, then "abc" to their left.
        // The second letter and "a" are apart on the page, with "bc" between them.
        let mixed = [
            glyph(30.0, 35.0),
            glyph(25.0, 30.0),
            glyph(5.0, 10.0),
            glyph(10.0, 15.0),
            glyph(15.0, 20.0),
        ];
        assert_eq!(
            run_quads(&mixed, 1, 3),
            [glyph(5.0, 10.0), glyph(25.0, 30.0)]
        );
        assert_eq!(run_quads(&mixed, 0, 5), [glyph(5.0, 35.0)]);
        // An accent set over the letter before the range stands between none of its
        // characters; a space of no width has nothing to mark.
        let accent = [glyph(0.0, 5.0), glyph(5.0, 10.0), glyph(1.0, 4.0)];
        assert_eq!(run_quads(&accent, 0, 2), [glyph(0.0, 10.0)]);
        let space = [glyph(0.0, 5.0), glyph(5.0, 5.0), glyph(5.0, 10.0)];
        assert_eq!(run_quads(&space, 0, 3), [glyph(0.0, 10.0)]);
        assert!(!run_quads(&space, 1, 2)[0].is_valid());
    }

    #[test]
    fn a_range_ending_on_a_superscript_stays_square_to_the_line() {
        // "x²": a raised, smaller glyph after one on the line.
        let sup = MarkupQuad {
            ul: Point::new(5.0, 6.0),
            ur: Point::new(8.0, 6.0),
            ll: Point::new(5.0, 13.0),
            lr: Point::new(8.0, 13.0),
        };
        let line = [glyph(0.0, 5.0), sup];
        let band = MarkupQuad {
            ul: Point::new(0.0, 6.0),
            ur: Point::new(8.0, 6.0),
            ll: Point::new(0.0, 20.0),
            lr: Point::new(8.0, 20.0),
        };
        assert_eq!(run_quads(&line, 0, 2), [band]);
        // A subscript at the start is the same, below the line.
        let sub = MarkupQuad {
            ul: Point::new(0.0, 15.0),
            ur: Point::new(3.0, 15.0),
            ll: Point::new(0.0, 23.0),
            lr: Point::new(3.0, 23.0),
        };
        let line = [sub, glyph(3.0, 8.0)];
        let band = MarkupQuad {
            ul: Point::new(0.0, 10.0),
            ur: Point::new(8.0, 10.0),
            ll: Point::new(0.0, 23.0),
            lr: Point::new(8.0, 23.0),
        };
        assert_eq!(run_quads(&line, 0, 2), [band]);
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
        let (_, words) = search_page_words(&list, "ROWN").unwrap();
        assert_eq!(
            words,
            [FoundWord {
                text: "brown".into(),
                count: 1
            }]
        );
    }

    fn lines(texts: &[&str]) -> PageText {
        PageText {
            lines: texts
                .iter()
                .map(|t| TextLine {
                    text: (*t).to_string(),
                    boxes: vec![[0.0; 4]; t.chars().count()],
                    bbox: [0.0; 4],
                    block: 0,
                    vertical: false,
                })
                .collect(),
        }
    }

    fn found(words: &[(&str, u32)]) -> Vec<FoundWord> {
        words
            .iter()
            .map(|&(text, count)| FoundWord {
                text: text.into(),
                count,
            })
            .collect()
    }

    #[test]
    fn words_containing_the_needle_are_whole_words_counted_once_each() {
        let text = lines(&[
            "Lectures on lecture halls; a lecturer's lecture.",
            "Banana, (lecture) and LECTURE",
        ]);
        assert_eq!(
            words_containing(&text, "lect"),
            found(&[
                ("Lectures", 1),
                ("lecture", 3),
                ("lecturer's", 1),
                ("LECTURE", 1)
            ])
        );
        // A word with the needle in it twice is one word.
        assert_eq!(
            words_containing(&text, "an"),
            found(&[("Banana", 1), ("and", 1)])
        );
        // Spaces in the needle match any run of them; the words around go with it.
        assert_eq!(
            words_containing(&text, "on   lec"),
            found(&[("on lecture", 1)])
        );
        assert!(words_containing(&text, " ").is_empty());
        assert!(words_containing(&text, "zebra").is_empty());
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
