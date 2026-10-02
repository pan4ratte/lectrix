//! Stitching: combine several PDFs into a new one (AGENTS.md section 6.4).
//!
//! Phase 0 combines whole documents. Page selection, internal-link and named-destination
//! remapping, and renaming of colliding names arrive in Phase 4 on top of this.

use mupdf::pdf::PdfDocument;

use crate::error::{Error, Result};
use crate::ffi;
use crate::labels::{self, LabelRule};
use crate::outline::{self, OutlineItem, OutlineTarget, ReadOutlineItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookmarkMode {
    /// Each source's outline under a new top-level bookmark named after the source.
    #[default]
    NestUnderSource,
    /// All sources' outlines at the top level.
    Flat,
    /// No bookmarks.
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelMode {
    /// Each source keeps its labels; a source without labels is numbered 1, 2, 3…
    #[default]
    KeepSources,
    /// One decimal sequence over the whole result (same as no labels in most readers,
    /// but explicit).
    Continuous,
    /// No `/PageLabels`.
    None,
}

pub struct MergeSource {
    pub doc: PdfDocument,
    /// Shown as the top-level bookmark title (document title or file name).
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MergeOptions {
    pub bookmarks: BookmarkMode,
    pub labels: LabelMode,
}

/// Combines `sources` in order into a new document.
pub fn merge(sources: &[MergeSource], options: MergeOptions) -> Result<PdfDocument> {
    if sources.is_empty() {
        return Err(Error::InvalidArgument("nothing to combine".into()));
    }
    let mut out = PdfDocument::new();
    let mut outline_items = Vec::new();
    let mut label_rules = Vec::new();
    let mut offset = 0usize;

    for source in sources {
        let count = usize::try_from(source.doc.page_count()?).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let pages: Vec<usize> = (0..count).collect();
        ffi::graft_pages(&mut out, None, &source.doc, &pages)?;

        if options.bookmarks != BookmarkMode::Drop {
            let items: Vec<OutlineItem> = outline::read_outline(&source.doc)?
                .into_iter()
                .map(|i| shift(i, offset))
                .collect();
            match options.bookmarks {
                BookmarkMode::NestUnderSource => outline_items.push(OutlineItem {
                    title: source.name.clone(),
                    target: OutlineTarget::Xyz {
                        page: offset,
                        left: None,
                        top: None,
                    },
                    open: false,
                    children: items,
                }),
                BookmarkMode::Flat => outline_items.extend(items),
                BookmarkMode::Drop => {}
            }
        }

        if options.labels == LabelMode::KeepSources {
            let rules = labels::read_rules(&source.doc)?;
            if rules.first().is_none_or(|r| r.start_page != 0) {
                label_rules.push(LabelRule::decimal_from_one(offset));
            }
            label_rules.extend(rules.into_iter().filter(|r| r.start_page < count).map(|r| {
                LabelRule {
                    start_page: r.start_page + offset,
                    ..r
                }
            }));
        }
        offset += count;
    }

    if !outline_items.is_empty() {
        outline::write_outline(&mut out, &outline_items)?;
    }
    match options.labels {
        LabelMode::KeepSources if !label_rules.is_empty() => {
            labels::write_rules(&mut out, label_rules)?
        }
        LabelMode::Continuous => {
            labels::write_rules(&mut out, vec![LabelRule::decimal_from_one(0)])?
        }
        _ => {}
    }
    Ok(out)
}

/// Converts a source bookmark to one pointing into the combined document.
fn shift(item: ReadOutlineItem, offset: usize) -> OutlineItem {
    OutlineItem {
        title: item.title,
        target: match item.page {
            Some(page) => OutlineTarget::Xyz {
                page: page + offset,
                left: item.left,
                top: item.top,
            },
            None => OutlineTarget::None,
        },
        open: item.open,
        children: item
            .children
            .into_iter()
            .map(|c| shift(c, offset))
            .collect(),
    }
}
