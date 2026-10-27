//! A document's text as the words Compare lines up (section 6.13): each word with where it
//! is, the key it is compared by, and what it is (punctuation, a header or footer, a list
//! number).

use unicode_segmentation::UnicodeSegmentation;

use crate::text::PageText;

/// Characters `start..end` of line `line` of page `page`, counted as in
/// [`crate::text::page_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub page: usize,
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// One word, or one punctuation mark, in reading order.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// What it is compared by: its text with quotes, dashes and the like made one kind.
    pub key: String,
    /// Its text as the document has it (a word broken over two lines joined, without the
    /// hyphen).
    pub text: String,
    /// Where its characters are: two spans for a word broken over two lines.
    pub spans: Vec<Span>,
    /// Punctuation, not a word: no letter or digit in it.
    pub punct: bool,
    /// In a header or footer: a line repeated near the top or bottom of the pages.
    pub margin: bool,
    /// A list's number or bullet at the start of a line ("1.", "a)", "iv.", "•").
    pub numbering: bool,
    /// Whitespace or a line break comes before it, so text put together from the words
    /// has a space there.
    pub space_before: bool,
}

/// How far from the top or the bottom of a page a header or footer lies, as a share of
/// the page's height.
const MARGIN_SHARE: f32 = 0.12;

/// The words of a document whose pages have `texts`, each `heights` tall (view space),
/// in reading order: page by page, line by line as MuPDF orders them. With
/// `find_margins`, lines repeated near the top or bottom of the pages are marked as
/// headers and footers.
pub fn words(texts: &[PageText], heights: &[f32], find_margins: bool) -> Vec<Word> {
    let margins = if find_margins {
        margin_lines(texts, heights)
    } else {
        Vec::new()
    };
    let mut out: Vec<Word> = Vec::new();
    for (page, text) in texts.iter().enumerate() {
        for (line_index, line) in text.lines.iter().enumerate() {
            let margin = margins
                .get(page)
                .is_some_and(|m| m.get(line_index).copied().unwrap_or(false));
            let first = out.len();
            split_line(&line.text, page, line_index, margin, &mut out);
            mark_numbering(&mut out[first..]);
        }
    }
    join_hyphenated(texts, &mut out);
    out
}

/// The words of one line, each with its characters counted from the line's start.
fn split_line(text: &str, page: usize, line: usize, margin: bool, out: &mut Vec<Word>) {
    // Byte offsets to character indices: the boxes are one per character.
    let mut char_at = vec![0usize; text.len() + 1];
    let mut count = 0;
    for (byte, ch) in text.char_indices() {
        for slot in &mut char_at[byte..byte + ch.len_utf8()] {
            *slot = count;
        }
        count += 1;
    }
    char_at[text.len()] = count;
    // A new line is a break, as whitespace is.
    let mut space_before = true;
    for (byte, segment) in text.split_word_bound_indices() {
        if segment.chars().all(is_space) {
            space_before = true;
            continue;
        }
        let punct = !segment.chars().any(char::is_alphanumeric);
        // UAX #29 keeps runs of the same punctuation together ("..."), and a word with
        // the marks inside it ("don't", "3.14"); a run of different marks splits.
        let pieces: Vec<(usize, &str)> = if punct {
            segment
                .char_indices()
                .map(|(i, c)| (byte + i, &segment[i..i + c.len_utf8()]))
                .collect()
        } else {
            vec![(byte, segment)]
        };
        for (at, piece) in pieces {
            out.push(Word {
                key: normalize(piece),
                text: piece.to_owned(),
                spans: vec![Span {
                    page,
                    line,
                    start: char_at[at],
                    end: char_at[at + piece.len()],
                }],
                punct,
                margin,
                numbering: false,
                space_before,
            });
            space_before = false;
        }
    }
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{200B}' | '\u{FEFF}')
}

/// The key a word is compared by. Quotes, apostrophes, dashes and ellipses come in
/// several forms that read the same; the forms of each are one here, so a document typed
/// with straight quotes compares equal to its typeset version. Case is kept: "Act" and
/// "act" differ.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{00B4}'
            | '\u{0060}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{00AB}'
            | '\u{00BB}' => out.push('"'),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
            | '\u{2212}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            '\u{00AD}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Marks a line's list number or bullet: its first word when it is a number, a single
/// letter or a roman numeral followed by "." or ")" (or a number with dots inside, as
/// "2.3.1"), or a bullet, with the mark after it; only when more text follows on the line.
fn mark_numbering(line: &mut [Word]) {
    let Some(first) = line.first() else {
        return;
    };
    let bullet = first.punct
        && first.text.chars().count() == 1
        && matches!(
            first.text.chars().next(),
            Some(
                '•' | '◦'
                    | '▪'
                    | '▫'
                    | '‣'
                    | '⁃'
                    | '∙'
                    | '·'
                    | '○'
                    | '■'
                    | '□'
                    | '►'
                    | '▸'
                    | '-'
                    | '–'
                    | '—'
                    | '*'
            )
        );
    let label = !first.punct && is_label(&first.text);
    let closed = line
        .get(1)
        .is_some_and(|w| w.punct && matches!(w.text.as_str(), "." | ")"));
    let dotted = !first.punct
        && first.text.contains('.')
        && first.text.chars().all(|c| c.is_ascii_digit() || c == '.');
    let marks = if bullet {
        1
    } else if label && closed {
        2
    } else if dotted {
        1
    } else {
        0
    };
    if marks == 0 || line.len() <= marks {
        return;
    }
    for word in &mut line[..marks] {
        word.numbering = true;
    }
}

/// A list label without its mark: a number, one letter, or a roman numeral.
fn is_label(text: &str) -> bool {
    let n = text.chars().count();
    (text.chars().all(|c| c.is_ascii_digit()) && n <= 4)
        || (n == 1 && text.chars().all(char::is_alphabetic))
        || is_roman(text)
}

fn is_roman(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 8
        && (text.chars().all(|c| "ivxlcdm".contains(c))
            || text.chars().all(|c| "IVXLCDM".contains(c)))
}

/// Joins a word broken over two lines with a hyphen ("inter-" then "national") into one,
/// the hyphen left out: the break is the layout's, not the text's. Only where the next
/// line goes on in the same block with a lowercase letter.
fn join_hyphenated(texts: &[PageText], words: &mut Vec<Word>) {
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let joined = (|| {
            let (before, hyphen, after) = (words.get(i)?, words.get(i + 1)?, words.get(i + 2)?);
            let (b, h, a) = (
                *before.spans.last()?,
                *hyphen.spans.first()?,
                *after.spans.first()?,
            );
            let hyphen_like = matches!(hyphen.text.as_str(), "-" | "\u{2010}" | "\u{00AD}");
            let line_end = texts
                .get(h.page)
                .and_then(|t| t.lines.get(h.line))
                .is_some_and(|l| h.end == l.text.chars().count());
            let blocks = |s: Span| {
                texts
                    .get(s.page)
                    .and_then(|t| t.lines.get(s.line))
                    .map(|l| l.block)
            };
            let next_line =
                a.page == h.page && a.line == h.line + 1 && blocks(a) == blocks(h) && a.start == 0;
            let lower = after.text.chars().next().is_some_and(char::is_lowercase);
            let ok = !before.punct
                && !after.punct
                && hyphen_like
                && b.line == h.line
                && b.page == h.page
                && b.end == h.start
                && line_end
                && next_line
                && lower
                && before.margin == after.margin;
            ok.then(|| Word {
                key: format!("{}{}", before.key, after.key),
                text: format!("{}{}", before.text, after.text),
                spans: before
                    .spans
                    .iter()
                    .chain(after.spans.iter())
                    .copied()
                    .collect(),
                punct: false,
                margin: before.margin,
                numbering: false,
                space_before: before.space_before,
            })
        })();
        match joined {
            Some(word) => {
                out.push(word);
                i += 3;
            }
            None => {
                out.push(words[i].clone());
                i += 1;
            }
        }
    }
    *words = out;
}

/// For each page, which of its lines are a header or footer: a line near the top or the
/// bottom of the page whose text, with its numbers taken as any number, is near the top
/// (or the bottom) of another page too: a running title, "Page 3 of 12", a lone page
/// number.
fn margin_lines(texts: &[PageText], heights: &[f32]) -> Vec<Vec<bool>> {
    use std::collections::HashMap;
    // (top or bottom, pattern) to the pages it is on.
    let mut seen: HashMap<(bool, String), Vec<usize>> = HashMap::new();
    let mut candidates: Vec<Vec<Option<(bool, String)>>> = Vec::with_capacity(texts.len());
    for (page, text) in texts.iter().enumerate() {
        let height = heights.get(page).copied().unwrap_or(0.0);
        let mut lines = Vec::with_capacity(text.lines.len());
        for line in &text.lines {
            let middle = (line.bbox[1] + line.bbox[3]) / 2.0;
            let band = if height <= 0.0 || line.vertical {
                None
            } else if middle < height * MARGIN_SHARE {
                Some(true)
            } else if middle > height * (1.0 - MARGIN_SHARE) {
                Some(false)
            } else {
                None
            };
            let entry = band.and_then(|top| {
                let pattern = pattern(&line.text);
                (!pattern.is_empty()).then_some((top, pattern))
            });
            if let Some(key) = &entry {
                let pages = seen.entry(key.clone()).or_default();
                if pages.last() != Some(&page) {
                    pages.push(page);
                }
            }
            lines.push(entry);
        }
        candidates.push(lines);
    }
    candidates
        .into_iter()
        .map(|lines| {
            lines
                .into_iter()
                .map(|entry| entry.is_some_and(|key| seen.get(&key).is_some_and(|p| p.len() >= 2)))
                .collect()
        })
        .collect()
}

/// A line's text with each number (and each roman numeral standing alone) as "#", and
/// runs of spaces as one: what tells a line that repeats on every page with its page
/// number.
fn pattern(text: &str) -> String {
    let mut out = String::new();
    for segment in text.split_word_bounds() {
        if segment.chars().all(is_space) {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else if segment
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
            && segment.chars().any(|c| c.is_ascii_digit())
            || is_roman(segment)
        {
            out.push('#');
        } else {
            out.push_str(&normalize(segment));
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextLine;

    /// A line of text at `y`, its characters 6 pt wide.
    pub(crate) fn line(text: &str, y: f32, block: u32) -> TextLine {
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

    fn keys(words: &[Word]) -> Vec<&str> {
        words.iter().map(|w| w.key.as_str()).collect()
    }

    #[test]
    fn splits_words_and_marks() {
        let page = PageText {
            lines: vec![line("Hello, “world” — it’s 3.14!", 100.0, 0)],
        };
        let words = words(&[page], &[792.0], false);
        assert_eq!(
            keys(&words),
            ["Hello", ",", "\"", "world", "\"", "-", "it's", "3.14", "!"]
        );
        assert!(words[1].punct && !words[0].punct);
        assert_eq!(
            words[3].spans[0],
            Span {
                page: 0,
                line: 0,
                start: 8,
                end: 13
            }
        );
        assert!(words[0].space_before && !words[1].space_before && words[2].space_before);
    }

    #[test]
    fn joins_words_broken_over_lines() {
        let page = PageText {
            lines: vec![line("an inter-", 100.0, 0), line("national law", 112.0, 0)],
        };
        let words = words(&[page], &[792.0], false);
        assert_eq!(keys(&words), ["an", "international", "law"]);
        assert_eq!(words[1].spans.len(), 2);
        // A capital after the hyphen, or another block, is not a broken word.
        let page = PageText {
            lines: vec![line("the Austro-", 100.0, 0), line("Hungarian", 112.0, 0)],
        };
        assert_eq!(
            keys(&super::words(&[page], &[792.0], false)),
            ["the", "Austro", "-", "Hungarian"]
        );
    }

    #[test]
    fn finds_headers_footers_and_numbering() {
        let page = |n: usize| PageText {
            lines: vec![
                line("Annual report", 20.0, 0),
                line("1. First item", 300.0, 1),
                line(&format!("Page {n} of 2"), 760.0, 2),
            ],
        };
        let words = words(&[page(1), page(2)], &[792.0, 792.0], true);
        let margin: Vec<bool> = words.iter().map(|w| w.margin).collect();
        assert!(margin[0] && margin[1]);
        assert!(
            !words[2].margin && words[2].numbering && words[3].numbering && !words[4].numbering
        );
        assert!(words.iter().filter(|w| w.key == "Page").all(|w| w.margin));
        // A single page has nothing to repeat.
        let words = super::words(&[page(1)], &[792.0], true);
        assert!(words.iter().all(|w| !w.margin));
    }
}
