//! A whole comparison of two open documents (section 6.13): their text read, compared,
//! and pages without text compared as images, with progress, and stopping when asked.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::{Error, Result};
use crate::session::Session;
use crate::text::PageText;

use super::{Caret, Comparison, Difference, DocText, Group, Kind, Options, PageRect, Side, visual};

/// How pages are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// By their text, and as images where a page has none (a scan, a drawing).
    Automatic,
    /// By their text alone.
    Text,
    /// As images alone, page by page.
    Visual,
}

/// What is being done, for a progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Pages read of both documents, and how many they have together.
    Reading { done: usize, total: usize },
    /// The texts are being lined up.
    Comparing,
    /// Pages compared as images, of how many.
    Images { done: usize, total: usize },
}

/// A page with fewer words than this is compared as an image in [`Mode::Automatic`].
const FEW_WORDS: usize = 3;
/// The most areas one page pair reports; more are taken together as one.
const AREAS_MAX: usize = 40;

/// How many threads read one document's pages: MuPDF builds each page's display list on
/// the document's own thread, and its text is read from that on these.
fn readers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4))
}

/// Compares the document `old` holds with the one `new` holds.
pub fn run(
    old: &Session,
    new: &Session,
    options: Options,
    mode: Mode,
    progress: &(dyn Fn(Progress) + Sync),
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Comparison> {
    let (old_info, new_info) = (old.info()?, new.info()?);
    let heights = |info: &crate::session::DocumentInfo| -> Vec<f32> {
        info.pages.iter().map(|p| p.height).collect()
    };
    let (old_heights, new_heights) = (heights(&old_info), heights(&new_info));
    let total = old_heights.len() + new_heights.len();
    let done = AtomicUsize::new(0);
    let read = |session: &Session, pages: usize| -> Result<Vec<PageText>> {
        if mode == Mode::Visual {
            return Ok(vec![PageText::default(); pages]);
        }
        read_text(
            session,
            pages,
            &|| {
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(Progress::Reading { done: n, total });
            },
            cancelled,
        )
    };
    progress(Progress::Reading { done: 0, total });
    let (old_text, new_text) = std::thread::scope(|scope| {
        let other = scope.spawn(|| read(new, new_heights.len()));
        let mine = read(old, old_heights.len());
        (mine, other.join().unwrap_or(Err(Error::ActorGone)))
    });
    let (old_text, new_text) = (old_text?, new_text?);
    if cancelled() {
        return Err(Error::Cancelled);
    }
    progress(Progress::Comparing);
    let old_doc = DocText {
        pages: &old_text,
        heights: &old_heights,
    };
    let new_doc = DocText {
        pages: &new_text,
        heights: &new_heights,
    };
    let mut comparison = if mode == Mode::Visual {
        Comparison::default()
    } else {
        super::compare(old_doc, new_doc, options, cancelled)?
    };
    if mode == Mode::Text {
        return Ok(comparison);
    }
    let pairs = image_pairs(&comparison, &old_text, &new_text, mode);
    if !pairs.is_empty() {
        progress(Progress::Images {
            done: 0,
            total: pairs.len(),
        });
    }
    // Rendered and compared on a few threads; each document builds its display lists on
    // its own thread meanwhile.
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let found: Mutex<Vec<(usize, Vec<Difference>)>> = Mutex::new(Vec::new());
    let failed: Mutex<Option<Error>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..(readers() * 2).min(pairs.len()) {
            scope.spawn(|| {
                loop {
                    let n = next.fetch_add(1, Ordering::Relaxed);
                    let Some(pair) = pairs.get(n) else { return };
                    if cancelled() {
                        return;
                    }
                    match compare_images(old, new, *pair, &old_info, &new_info) {
                        Ok(differences) => {
                            if let Ok(mut found) = found.lock() {
                                found.push((n, differences));
                            }
                            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                            progress(Progress::Images {
                                done: d,
                                total: pairs.len(),
                            });
                        }
                        Err(e) => {
                            if let Ok(mut failed) = failed.lock() {
                                failed.get_or_insert(e);
                            }
                            next.store(pairs.len(), Ordering::Relaxed);
                            return;
                        }
                    }
                }
            });
        }
    });
    if let Some(e) = failed.into_inner().ok().flatten() {
        return Err(e);
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut found = found.into_inner().unwrap_or_default();
    found.sort_by_key(|(n, _)| *n);
    comparison
        .differences
        .extend(found.into_iter().flat_map(|(_, d)| d));
    comparison
        .differences
        .sort_by_key(|d| (d.old.page(), d.new.page(), d.group == Group::Margins));
    Ok(comparison)
}

/// Every page's text of the document `session` holds (its `pages` pages), read on a few
/// threads; `page_done` after each page.
pub fn read_text(
    session: &Session,
    pages: usize,
    page_done: &(dyn Fn() + Sync),
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<PageText>> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<PageText>>> = Mutex::new(vec![None; pages]);
    let failed: Mutex<Option<Error>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..readers().min(pages.max(1)) {
            scope.spawn(|| {
                loop {
                    let page = next.fetch_add(1, Ordering::Relaxed);
                    if page >= pages || cancelled() {
                        return;
                    }
                    match session.content_text(page) {
                        Ok(text) => {
                            if let Ok(mut out) = out.lock() {
                                out[page] = Some(text);
                            }
                            page_done();
                        }
                        Err(e) => {
                            if let Ok(mut failed) = failed.lock() {
                                failed.get_or_insert(e);
                            }
                            next.store(pages, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            });
        }
    });
    if let Some(e) = failed.into_inner().ok().flatten() {
        return Err(e);
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    Ok(out
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect())
}

/// A page of the old document and a page of the new to compare as images; either may be
/// missing (the other document has fewer pages).
type Pair = (Option<usize>, Option<usize>);

fn words_on(text: &PageText) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text.lines
        .iter()
        .map(|l| l.text.unicode_words().count())
        .sum()
}

/// The pages to compare as images: in [`Mode::Visual`] every page with the page of the
/// same number; in [`Mode::Automatic`] each page with (almost) no text with the page the
/// text lines it up with (or the page of the same number, with no text to go by).
fn image_pairs(c: &Comparison, old: &[PageText], new: &[PageText], mode: Mode) -> Vec<Pair> {
    let (no, nn) = (old.len(), new.len());
    if mode == Mode::Visual {
        return (0..no.max(nn))
            .map(|i| ((i < no).then_some(i), (i < nn).then_some(i)))
            .collect();
    }
    // The page of the other document at the same place, by the anchors.
    let partner = |page: usize, from_old: bool, count: usize| -> Option<usize> {
        if count == 0 {
            return None;
        }
        let place = page as f64 + 0.5;
        // With no text to go by, the page of the same number; past the other's last page,
        // none: the page is one more than the other has.
        if c.anchors.is_empty() {
            return (page < count).then_some(page);
        }
        let mapped = {
            let (from, to): (Vec<f64>, Vec<f64>) = c
                .anchors
                .iter()
                .map(|a| {
                    if from_old {
                        (a.old, a.new)
                    } else {
                        (a.new, a.old)
                    }
                })
                .unzip();
            let at = from.partition_point(|&p| p <= place);
            match (at.checked_sub(1), from.get(at)) {
                (Some(i), Some(&next)) if next > from[i] => {
                    to[i] + (place - from[i]) / (next - from[i]) * (to[at] - to[i])
                }
                (Some(i), _) => to[i] + (place - from[i]),
                (None, _) => to[0] - (from[0] - place),
            }
        };
        Some((mapped.max(0.0) as usize).min(count - 1))
    };
    let mut pairs: Vec<Pair> = Vec::new();
    for (i, text) in old.iter().enumerate() {
        if words_on(text) < FEW_WORDS {
            let j = partner(i, true, nn);
            if j.is_none_or(|j| words_on(&new[j]) < FEW_WORDS) {
                pairs.push((Some(i), j));
            }
        }
    }
    for (j, text) in new.iter().enumerate() {
        if words_on(text) < FEW_WORDS && !pairs.iter().any(|p| p.1 == Some(j)) {
            let i = partner(j, false, no);
            if i.is_none_or(|i| words_on(&old[i]) < FEW_WORDS) {
                pairs.push((i, Some(j)));
            }
        }
    }
    pairs.sort();
    pairs.dedup();
    pairs
}

fn whole(info: &crate::session::DocumentInfo, page: usize) -> [f32; 4] {
    info.pages
        .get(page)
        .map_or([0.0; 4], |p| [0.0, 0.0, p.width, p.height])
}

/// Where a page missing from a document would be: after its last page.
fn after_last(info: &crate::session::DocumentInfo) -> Option<Caret> {
    let last = info.pages.len().checked_sub(1)?;
    let p = &info.pages[last];
    Some(Caret {
        page: last,
        x: 0.0,
        y0: p.height - 1.0,
        y1: p.height,
    })
}

/// The differences between two pages compared as images.
fn compare_images(
    old: &Session,
    new: &Session,
    (a, b): Pair,
    old_info: &crate::session::DocumentInfo,
    new_info: &crate::session::DocumentInfo,
) -> Result<Vec<Difference>> {
    let area = |page: usize, rect: [f32; 4]| Side {
        rects: vec![PageRect { page, rect }],
        ..Side::default()
    };
    let missing = |info: &crate::session::DocumentInfo| Side {
        caret: after_last(info),
        ..Side::default()
    };
    match (a, b) {
        (Some(a), None) => Ok(vec![Difference {
            kind: Kind::Deleted,
            group: Group::Body,
            old: area(a, whole(old_info, a)),
            new: missing(new_info),
        }]),
        (None, Some(b)) => Ok(vec![Difference {
            kind: Kind::Inserted,
            group: Group::Body,
            old: missing(old_info),
            new: area(b, whole(new_info, b)),
        }]),
        (None, None) => Ok(Vec::new()),
        (Some(a), Some(b)) => {
            let (la, _) = old.content_list(a)?;
            let (lb, _) = new.content_list(b)?;
            let (ga, gb) = (visual::render(&la)?, visual::render(&lb)?);
            let mut rects = visual::differences(&ga, &gb);
            if rects.len() > AREAS_MAX {
                let all = rects.iter().fold(rects[0], |u, r| {
                    [
                        u[0].min(r[0]),
                        u[1].min(r[1]),
                        u[2].max(r[2]),
                        u[3].max(r[3]),
                    ]
                });
                rects = vec![all];
            }
            // In the order they are on the page.
            rects.sort_by(|p, q| p[1].total_cmp(&q[1]).then(p[0].total_cmp(&q[0])));
            Ok(rects
                .into_iter()
                .map(|r| Difference {
                    kind: Kind::Changed,
                    group: Group::Body,
                    old: area(a, r),
                    new: area(b, r),
                })
                .collect())
        }
    }
}
