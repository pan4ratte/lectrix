//! Compare (section 6.13): the differences between the text of two documents, word by
//! word, as ABBYY FineReader's Compare Documents finds them, and where each is on the
//! pages of both; the places the two line up at, for scrolling them together; the
//! annotations an annotated copy gets; the PDF report; and pages compared as images,
//! for pages without text.

pub mod annotate;
mod diff;
pub mod report;
mod run;
pub mod visual;
pub mod words;

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::text::PageText;

pub use run::{Mode, Progress, read_text, run};
pub use words::Span;
use words::Word;

/// What to look for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Differences in punctuation and hyphenation. Off, punctuation is left out of the
    /// comparison.
    pub punctuation: bool,
    /// Words that differ by one letter (spelling). Off, such a word changed alone is no
    /// difference.
    pub one_letter: bool,
    /// Headers and footers found and compared apart from the body text, their differences
    /// in a group of their own. Off, they are body text.
    pub margins: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            punctuation: true,
            one_letter: true,
            margins: true,
        }
    }
}

/// What happened to a passage, from the first document (old) to the second (new).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Only the new document has it.
    Inserted,
    /// Only the old document has it.
    Deleted,
    /// Both have it, worded differently.
    Changed,
}

/// The group FineReader puts a difference in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// The body text: the differences that matter.
    Body,
    /// Headers and footers: running titles, page numbers.
    Margins,
    /// A list's numbers or bullets.
    Numbering,
}

/// A rectangle on a page (view space at zoom 1, as [`crate::text`] gives it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub page: usize,
    pub rect: [f32; 4],
}

/// Where text the other document has would go in this one: a line from `y0` to `y1` at
/// `x` (view space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caret {
    pub page: usize,
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
}

/// A difference as one document has it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Side {
    /// Its text there; empty when it has none (inserted in the other, or deleted from it).
    pub text: String,
    /// Its characters, for annotations.
    pub spans: Vec<Span>,
    /// Where its text is: a rectangle for each run of characters on a line.
    pub rects: Vec<PageRect>,
    /// Where the other document's text would go, when this one has none.
    pub caret: Option<Caret>,
}

impl Side {
    /// The page it is on, or would be.
    pub fn page(&self) -> usize {
        self.rects
            .first()
            .map(|r| r.page)
            .or(self.caret.map(|c| c.page))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub kind: Kind,
    pub group: Group,
    pub old: Side,
    pub new: Side,
}

/// Places the two documents line up at, as pages from the start with the part of a page
/// as a fraction, increasing in both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub old: f64,
    pub new: f64,
}

/// A document's text, page by page, and its pages' heights (view space).
#[derive(Debug, Clone, Copy)]
pub struct DocText<'a> {
    pub pages: &'a [PageText],
    pub heights: &'a [f32],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparison {
    pub differences: Vec<Difference>,
    pub anchors: Vec<Anchor>,
    /// How many words each document has (punctuation not counted).
    pub old_words: usize,
    pub new_words: usize,
}

/// Compares the text of `old` with that of `new`. `cancelled` is asked now and then; once
/// it says so, this fails with [`Error::Cancelled`].
pub fn compare(
    old: DocText<'_>,
    new: DocText<'_>,
    options: Options,
    cancelled: &dyn Fn() -> bool,
) -> Result<Comparison> {
    let old_words = words::words(old.pages, old.heights, options.margins);
    let new_words = words::words(new.pages, new.heights, options.margins);
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let keep = |w: &Word| options.punctuation || !w.punct;
    let stream = |all: &[Word], margin: bool| -> Vec<usize> {
        (0..all.len())
            .filter(|&i| keep(&all[i]) && all[i].margin == margin)
            .collect()
    };
    // Each key as a number, the same in both documents.
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut intern = |all: &[Word], picked: &[usize]| -> Vec<u32> {
        picked
            .iter()
            .map(|&i| {
                let key = &all[i].key;
                if let Some(&id) = ids.get(key) {
                    return id;
                }
                let id = ids.len() as u32;
                ids.insert(key.clone(), id);
                id
            })
            .collect()
    };
    let (old_all, new_all) = (&old_words, &new_words);
    let mut differences = Vec::new();
    let mut anchors = Vec::new();
    for margin in [false, true] {
        let (a, b) = (stream(old_all, margin), stream(new_all, margin));
        if a.is_empty() && b.is_empty() {
            continue;
        }
        let (ka, kb) = (intern(old_all, &a), intern(new_all, &b));
        let found = diff::matches(&ka, &kb, cancelled);
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let ctx = Context {
            old: old_all,
            new: new_all,
            old_text: old,
            new_text: new,
            a: &a,
            b: &b,
        };
        if !margin {
            anchors = ctx.anchors(&found);
        }
        for hunk in cleanup(ctx.hunks(&found), &ctx) {
            if let Some(d) = ctx.difference(&hunk, margin, options) {
                differences.push(d);
            }
        }
    }
    // Sorted by page, the body's and the margins' differences interleave; within a page,
    // body text first, then headers and footers, each in reading order (a stable sort).
    differences.sort_by_key(|d| (d.old.page(), d.new.page(), d.group == Group::Margins));
    Ok(Comparison {
        differences,
        anchors,
        old_words: old_words.iter().filter(|w| !w.punct).count(),
        new_words: new_words.iter().filter(|w| !w.punct).count(),
    })
}

/// A stretch where the two differ: words `a` of the old stream and `b` of the new (either
/// may be empty), as places in the streams.
#[derive(Debug, Clone, PartialEq)]
struct Hunk {
    a: std::ops::Range<usize>,
    b: std::ops::Range<usize>,
}

struct Context<'w> {
    old: &'w [Word],
    new: &'w [Word],
    old_text: DocText<'w>,
    new_text: DocText<'w>,
    /// The streams compared: places in `old` and `new`.
    a: &'w [usize],
    b: &'w [usize],
}

impl Context<'_> {
    /// The stretches between matches.
    fn hunks(&self, found: &[(usize, usize)]) -> Vec<Hunk> {
        let mut out = Vec::new();
        let (mut pa, mut pb) = (0usize, 0usize);
        for &(i, j) in found
            .iter()
            .chain(std::iter::once(&(self.a.len(), self.b.len())))
        {
            if i > pa || j > pb {
                out.push(Hunk { a: pa..i, b: pb..j });
            }
            pa = i + 1;
            pb = j + 1;
        }
        out
    }

    fn chars(&self, words: &[Word], stream: &[usize], range: &std::ops::Range<usize>) -> usize {
        stream[range.clone()]
            .iter()
            .map(|&i| words[i].key.chars().count() + 1)
            .sum()
    }

    /// The places the documents line up at: a matched word now and then, where both go on.
    fn anchors(&self, found: &[(usize, usize)]) -> Vec<Anchor> {
        let place = |text: DocText<'_>, word: &Word| -> Option<f64> {
            let span = word.spans.first()?;
            let line = text.pages.get(span.page)?.lines.get(span.line)?;
            let height = f64::from(*text.heights.get(span.page)?).max(1.0);
            let y = f64::from(line.bbox[1]);
            Some(span.page as f64 + (y / height).clamp(0.0, 0.999))
        };
        let mut out: Vec<Anchor> = Vec::new();
        for &(i, j) in found {
            let (Some(old), Some(new)) = (
                place(self.old_text, &self.old[self.a[i]]),
                place(self.new_text, &self.new[self.b[j]]),
            ) else {
                continue;
            };
            match out.last() {
                Some(last) if old <= last.old + 0.02 || new <= last.new => {}
                _ => out.push(Anchor { old, new }),
            }
        }
        out
    }

    /// The difference a hunk is, or None when the options leave it out.
    fn difference(&self, hunk: &Hunk, margin: bool, options: Options) -> Option<Difference> {
        let olds: Vec<&Word> = self.a[hunk.a.clone()]
            .iter()
            .map(|&i| &self.old[i])
            .collect();
        let news: Vec<&Word> = self.b[hunk.b.clone()]
            .iter()
            .map(|&i| &self.new[i])
            .collect();
        let kind = match (olds.is_empty(), news.is_empty()) {
            (true, true) => return None,
            (true, false) => Kind::Inserted,
            (false, true) => Kind::Deleted,
            (false, false) => Kind::Changed,
        };
        if kind == Kind::Changed && !options.one_letter && one_letter(&olds, &news) {
            return None;
        }
        let group = if margin {
            Group::Margins
        } else if olds
            .iter()
            .chain(news.iter())
            .all(|w| w.numbering || w.punct)
            && olds.iter().chain(news.iter()).any(|w| w.numbering)
        {
            Group::Numbering
        } else {
            Group::Body
        };
        let old = side(self.old_text, &olds, || {
            caret(self.old_text, self.old, self.a, hunk.a.start)
        });
        let new = side(self.new_text, &news, || {
            caret(self.new_text, self.new, self.b, hunk.b.start)
        });
        Some(Difference {
            kind,
            group,
            old,
            new,
        })
    }
}

/// Merges differences split by a few equal words in a passage rewritten as a whole ("the"
/// kept between two changed phrases): an equal stretch of a few words, at most half as
/// long as the changes on either side of it, joins them when both are changes
/// (diff-match-patch's semantic cleanup, made stricter for words). Text only inserted or
/// only deleted on either side stays apart: the words between are the same in both, and
/// marking them changed would say otherwise. Merging can make a change long enough to take
/// in the stretch before it, so it runs until nothing changes.
fn cleanup(hunks: Vec<Hunk>, ctx: &Context<'_>) -> Vec<Hunk> {
    const MOST_WORDS: usize = 3;
    let size = |h: &Hunk| {
        ctx.chars(ctx.old, ctx.a, &h.a)
            .max(ctx.chars(ctx.new, ctx.b, &h.b))
    };
    let mut out = hunks;
    loop {
        let mut merged = false;
        let mut next: Vec<Hunk> = Vec::with_capacity(out.len());
        for hunk in out {
            if let Some(last) = next.last_mut() {
                let between = last.a.end..hunk.a.start;
                let gap = ctx.chars(ctx.old, ctx.a, &between);
                let changes = |h: &Hunk| !h.a.is_empty() && !h.b.is_empty();
                if changes(last)
                    && changes(&hunk)
                    && between.len() <= MOST_WORDS
                    && gap * 2 <= size(last).min(size(&hunk))
                {
                    last.a.end = hunk.a.end;
                    last.b.end = hunk.b.end;
                    merged = true;
                    continue;
                }
            }
            next.push(hunk);
        }
        out = next;
        if !merged {
            return out;
        }
    }
}

/// One word for one word, differing by a letter at most (spelling).
fn one_letter(olds: &[&Word], news: &[&Word]) -> bool {
    let words = |ws: &[&Word]| ws.iter().filter(|w| !w.punct).count();
    if words(olds) != 1 || words(news) != 1 || olds.len() != news.len() {
        return false;
    }
    let joined = |ws: &[&Word]| ws.iter().map(|w| w.key.as_str()).collect::<String>();
    edit_distance(&joined(olds), &joined(news)) <= 1
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if x == y {
                diag
            } else {
                1 + diag.min(up).min(row[j])
            };
            diag = up;
        }
    }
    row[b.len()]
}

/// A difference as one document has it: the text of `words`, where they are, or, for
/// none, where the other document's text would go.
fn side(text: DocText<'_>, words: &[&Word], caret: impl FnOnce() -> Option<Caret>) -> Side {
    if words.is_empty() {
        return Side {
            caret: caret(),
            ..Side::default()
        };
    }
    let mut joined = String::new();
    for (n, w) in words.iter().enumerate() {
        if n > 0 && w.space_before {
            joined.push(' ');
        }
        joined.push_str(&w.text);
    }
    // Runs of characters on a line: a span after the last on the same line extends it
    // (the spaces between words with it).
    let mut runs: Vec<Span> = Vec::new();
    for span in words.iter().flat_map(|w| w.spans.iter()) {
        match runs.last_mut() {
            Some(last)
                if last.page == span.page && last.line == span.line && span.start >= last.end =>
            {
                last.end = span.end;
            }
            _ => runs.push(*span),
        }
    }
    let rects = runs
        .iter()
        .filter_map(|s| {
            let line = text.pages.get(s.page)?.lines.get(s.line)?;
            let boxes = line.boxes.get(s.start..s.end)?;
            let mut r = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for b in boxes.iter().filter(|b| b[2] > b[0]) {
                r = [
                    r[0].min(b[0]),
                    r[1].min(b[1]),
                    r[2].max(b[2]),
                    r[3].max(b[3]),
                ];
            }
            r[0].is_finite().then_some(PageRect {
                page: s.page,
                rect: r,
            })
        })
        .collect();
    Side {
        text: joined,
        spans: words.iter().flat_map(|w| w.spans.iter().copied()).collect(),
        rects,
        caret: None,
    }
}

/// Where text would go at place `at` of a stream: after the word before it, or before the
/// word after it.
fn caret(text: DocText<'_>, words: &[Word], stream: &[usize], at: usize) -> Option<Caret> {
    let edge = |span: Span, after: bool| -> Option<Caret> {
        let line = text.pages.get(span.page)?.lines.get(span.line)?;
        let b = line.boxes.get(if after {
            span.end.checked_sub(1)?
        } else {
            span.start
        })?;
        Some(Caret {
            page: span.page,
            x: if after { b[2] } else { b[0] },
            y0: line.bbox[1],
            y1: line.bbox[3],
        })
    };
    let before = at
        .checked_sub(1)
        .and_then(|i| stream.get(i))
        .and_then(|&i| words[i].spans.last().copied());
    if let Some(found) = before.and_then(|s| edge(s, true)) {
        return Some(found);
    }
    let after = stream
        .get(at)
        .and_then(|&i| words[i].spans.first().copied());
    after.and_then(|s| edge(s, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextLine;

    fn line(text: &str, y: f32, block: u32) -> TextLine {
        let boxes = text
            .chars()
            .enumerate()
            .map(|(i, _)| [i as f32 * 6.0 + 50.0, y, i as f32 * 6.0 + 56.0, y + 10.0])
            .collect::<Vec<_>>();
        let n = boxes.len() as f32;
        TextLine {
            text: text.to_owned(),
            boxes,
            bbox: [50.0, y, 50.0 + n * 6.0, y + 10.0],
            block,
            vertical: false,
        }
    }

    fn doc(pages: &[&[&str]]) -> Vec<PageText> {
        pages
            .iter()
            .map(|lines| PageText {
                lines: lines
                    .iter()
                    .enumerate()
                    .map(|(i, t)| line(t, 100.0 + i as f32 * 14.0, i as u32))
                    .collect(),
            })
            .collect()
    }

    fn run(old: &[PageText], new: &[PageText], options: Options) -> Comparison {
        let ho = vec![792.0; old.len()];
        let hn = vec![792.0; new.len()];
        compare(
            DocText {
                pages: old,
                heights: &ho,
            },
            DocText {
                pages: new,
                heights: &hn,
            },
            options,
            &|| false,
        )
        .unwrap()
    }

    fn summary(c: &Comparison) -> Vec<(Kind, String, String)> {
        c.differences
            .iter()
            .map(|d| (d.kind, d.old.text.clone(), d.new.text.clone()))
            .collect()
    }

    #[test]
    fn finds_inserted_deleted_and_changed_words() {
        let old = doc(&[&["The quick brown fox jumps over the lazy dog."]]);
        let new = doc(&[&["The quick red fox jumps over the dog, twice."]]);
        let c = run(&old, &new, Options::default());
        assert_eq!(
            summary(&c),
            [
                (Kind::Changed, "brown".into(), "red".into()),
                (Kind::Deleted, "lazy".into(), String::new()),
                (Kind::Inserted, String::new(), ", twice".into()),
            ]
        );
        // "red" is where "brown" was: characters 10 to 13 of the line.
        assert_eq!(c.differences[0].new.rects[0].rect[0], 50.0 + 10.0 * 6.0);
        // "lazy" deleted: the new document's caret after "the".
        let caret = c.differences[1].new.caret.unwrap();
        assert_eq!(caret.x, 50.0 + 32.0 * 6.0);
    }

    #[test]
    fn options_leave_out_punctuation_and_spelling() {
        let old = doc(&[&["Colour, as in the regulations."]]);
        let new = doc(&[&["Color as in the regulation."]]);
        let all = run(&old, &new, Options::default());
        assert_eq!(all.differences.len(), 2, "{:?}", summary(&all));
        let quiet = run(
            &old,
            &new,
            Options {
                punctuation: false,
                one_letter: false,
                margins: true,
            },
        );
        assert!(quiet.differences.is_empty(), "{:?}", summary(&quiet));
    }

    #[test]
    fn lines_up_across_an_inserted_page() {
        let filler = |n: usize| format!("Paragraph {n} says something only it says, number {n}.");
        let p: Vec<String> = (0..6).map(filler).collect();
        let old = doc(&[&[&p[0], &p[1]], &[&p[2], &p[3]], &[&p[4], &p[5]]]);
        let new = doc(&[
            &[&p[0], &p[1]],
            &["A new page of text."],
            &[&p[2], &p[3]],
            &[&p[4], &p[5]],
        ]);
        let c = run(&old, &new, Options::default());
        assert_eq!(
            summary(&c),
            [(Kind::Inserted, String::new(), "A new page of text.".into())]
        );
        assert_eq!(c.differences[0].new.page(), 1);
        // Old page 1 is new page 2.
        let at = c.anchors.iter().find(|a| a.old >= 1.0).unwrap();
        assert!((2.0..3.0).contains(&at.new), "{at:?}");
    }

    #[test]
    fn groups_headers_footers_and_numbering() {
        let old = doc(&[
            &["Report", "1. Scope of the work", "Page 1"],
            &["Report", "More text here", "Page 2"],
        ]);
        let mut new = doc(&[
            &["Report v2", "2. Scope of the work", "Page 1"],
            &["Report v2", "More text here", "Page 2"],
        ]);
        // Footers at the bottom of the page, headers at the top.
        for doc in [&mut new] {
            for page in doc.iter_mut() {
                page.lines[0].bbox[1] = 20.0;
                page.lines[0].bbox[3] = 30.0;
                page.lines[2].bbox[1] = 760.0;
                page.lines[2].bbox[3] = 770.0;
            }
        }
        let mut old = old;
        for page in old.iter_mut() {
            page.lines[0].bbox[1] = 20.0;
            page.lines[0].bbox[3] = 30.0;
            page.lines[2].bbox[1] = 760.0;
            page.lines[2].bbox[3] = 770.0;
        }
        let c = run(&old, &new, Options::default());
        let groups: Vec<(Group, &str)> = c
            .differences
            .iter()
            .map(|d| (d.group, d.new.text.as_str()))
            .collect();
        assert_eq!(
            groups,
            [
                (Group::Numbering, "2"),
                (Group::Margins, "v2"),
                (Group::Margins, "v2")
            ]
        );
    }

    #[test]
    fn merges_a_passage_rewritten_around_a_common_word() {
        let old = doc(&[&["We will deliver the goods by Friday."]]);
        let new = doc(&[&["They promised the payment next week."]]);
        let c = run(&old, &new, Options::default());
        assert_eq!(c.differences.len(), 1, "{:?}", summary(&c));
        assert_eq!(c.differences[0].kind, Kind::Changed);
    }
}
