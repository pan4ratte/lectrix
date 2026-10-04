//! Bookmarks (the document outline).
//!
//! - [`read_bookmarks`] reads the tree the bookmarks panel shows, with stable ids and
//!   resolved targets (`tree.rs`, named destinations in `names.rs`).
//! - [`edit`] changes it in place, touching only what each edit must (AGENTS.md
//!   section 6.2: untouched bookmarks keep their destinations and actions exactly).
//! - [`write_outline`] replaces the whole outline (pdf-cli, merging).

pub mod edit;
pub(crate) mod names;
pub(crate) mod tree;

pub use names::lookup_dest;
pub use tree::{Bookmark, Outline, Target, read_bookmarks};

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::{PageGeometry, Point, read_page_boxes};
use crate::objects::{self, text_string};

/// A place to point a new or retargeted bookmark at: the point at the top-left of the
/// view, in view space (points from the top-left of the visible page at zoom 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewDest {
    pub page: usize,
    pub x: f64,
    pub y: f64,
}

/// `[page /XYZ left top null]` for a point in view space (AGENTS.md section 6.2: explicit
/// destination, left and top of the view, null zoom so the reader keeps its zoom). The
/// point is converted to user space through `geometry.rs`, clamped to the visible page.
pub fn destination_from_view(doc: &PdfDocument, dest: &ViewDest) -> Result<PdfObject> {
    let page_no = i32::try_from(dest.page).map_err(|_| Error::PageOutOfRange(dest.page))?;
    if page_no >= doc.page_count()? || !dest.x.is_finite() || !dest.y.is_finite() {
        return Err(Error::PageOutOfRange(dest.page));
    }
    let page_ref = doc.find_page(page_no)?;
    let g = PageGeometry::new(&read_page_boxes(&page_ref)?);
    let view = Point::new(dest.x.clamp(0.0, g.width), dest.y.clamp(0.0, g.height));
    let user = g.view_to_user(view);
    let mut array = doc.new_array_with_capacity(5)?;
    array.array_push(page_ref)?;
    array.array_push(PdfObject::new_name("XYZ")?)?;
    // Whole points are plenty for a scroll position and keep the numbers short.
    array.array_push(PdfObject::new_real(user.x.round() as f32)?)?;
    array.array_push(PdfObject::new_real(user.y.round() as f32)?)?;
    array.array_push(PdfObject::new_null())?;
    Ok(array)
}

/// Where a bookmark points.
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineTarget {
    /// No destination (a heading-only bookmark).
    None,
    /// `[page /XYZ left top null]`, in PDF user space. `None` coordinates are written as
    /// null ("keep current"). The zoom is always null so the reader keeps its zoom.
    Xyz {
        page: usize,
        left: Option<f64>,
        top: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    pub target: OutlineTarget,
    /// Expanded in the reader's bookmark panel.
    pub open: bool,
    pub children: Vec<OutlineItem>,
}

impl OutlineItem {
    pub fn new(title: impl Into<String>, page: usize) -> OutlineItem {
        OutlineItem {
            title: title.into(),
            target: OutlineTarget::Xyz {
                page,
                left: None,
                top: None,
            },
            open: false,
            children: Vec::new(),
        }
    }
}

/// Number of items visible below `items` when their parent is open: each item counts
/// itself, plus its own visible descendants if it is open.
fn visible_count(items: &[OutlineItem]) -> i32 {
    items
        .iter()
        .map(|i| {
            1 + if i.open {
                visible_count(&i.children)
            } else {
                0
            }
        })
        .sum()
}

/// The `/Count` value of an item with children: positive if open, negative if closed,
/// magnitude = descendants visible when it is open. `None` for a leaf.
pub fn count_for(item: &OutlineItem) -> Option<i32> {
    if item.children.is_empty() {
        return None;
    }
    let n = visible_count(&item.children);
    Some(if item.open { n } else { -n })
}

/// Replaces the document outline with `items`. An empty list removes the outline.
pub fn write_outline(doc: &mut PdfDocument, items: &[OutlineItem]) -> Result<()> {
    let mut catalog = doc.catalog()?;
    // Unlink the old tree; its objects become unreferenced and are dropped by a full save
    // with garbage collection. An incremental save leaves them in place, untouched.
    catalog.dict_delete("Outlines")?;
    if items.is_empty() {
        return Ok(());
    }
    let mut root = doc.new_dict()?;
    root.dict_put("Type", PdfObject::new_name("Outlines")?)?;
    let mut root = doc.add_object(&root)?;
    write_level(doc, items, &mut root)?;
    root.dict_put("Count", PdfObject::new_int(visible_count(items))?)?;
    catalog.dict_put("Outlines", root)?;
    Ok(())
}

fn write_level(doc: &mut PdfDocument, items: &[OutlineItem], parent: &mut PdfObject) -> Result<()> {
    let mut refs: Vec<PdfObject> = Vec::with_capacity(items.len());
    for item in items {
        let mut dict = doc.new_dict()?;
        dict.dict_put("Title", text_string(doc, &item.title)?)?;
        dict.dict_put("Parent", parent.clone())?;
        if let Some(dest) = destination(doc, &item.target)? {
            dict.dict_put("Dest", dest)?;
        }
        if let Some(count) = count_for(item) {
            dict.dict_put("Count", PdfObject::new_int(count)?)?;
        }
        let mut indirect = doc.add_object(&dict)?;
        if !item.children.is_empty() {
            write_level(doc, &item.children, &mut indirect)?;
        }
        refs.push(indirect);
    }
    for i in 1..refs.len() {
        let (before, after) = refs.split_at_mut(i);
        let (prev, next) = (&mut before[i - 1], &mut after[0]);
        prev.dict_put("Next", next.clone())?;
        next.dict_put("Prev", prev.clone())?;
    }
    if let (Some(first), Some(last)) = (refs.first(), refs.last()) {
        parent.dict_put("First", first.clone())?;
        parent.dict_put("Last", last.clone())?;
    }
    Ok(())
}

fn destination(doc: &PdfDocument, target: &OutlineTarget) -> Result<Option<PdfObject>> {
    let OutlineTarget::Xyz { page, left, top } = target else {
        return Ok(None);
    };
    let page_no = i32::try_from(*page).map_err(|_| Error::PageOutOfRange(*page))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(*page));
    }
    let page_ref = doc.find_page(page_no)?;
    let mut dest = doc.new_array_with_capacity(5)?;
    dest.array_push(page_ref)?;
    dest.array_push(PdfObject::new_name("XYZ")?)?;
    for coord in [left, top] {
        dest.array_push(match coord {
            Some(v) => PdfObject::new_real(*v as f32)?,
            None => PdfObject::new_null(),
        })?;
    }
    dest.array_push(PdfObject::new_null())?;
    Ok(Some(dest))
}

/// A bookmark as read back from a file: title, open state, and target page if the
/// destination is an explicit page reference (directly or through a GoTo action).
#[derive(Debug, Clone, PartialEq)]
pub struct ReadOutlineItem {
    pub title: String,
    pub page: Option<usize>,
    /// `/XYZ` left and top, when the destination is an `/XYZ` destination.
    pub left: Option<f64>,
    pub top: Option<f64>,
    pub open: bool,
    pub children: Vec<ReadOutlineItem>,
}

/// Reads the outline tree (explicit destinations only; named destinations resolve to
/// `page: None`).
pub fn read_outline(doc: &PdfDocument) -> Result<Vec<ReadOutlineItem>> {
    let catalog = doc.catalog()?;
    let Some(root) = catalog.get_dict("Outlines")? else {
        return Ok(Vec::new());
    };
    read_level(doc, root.get_dict("First")?, 0)
}

fn read_level(
    doc: &PdfDocument,
    mut node: Option<PdfObject>,
    depth: u32,
) -> Result<Vec<ReadOutlineItem>> {
    let mut out = Vec::new();
    // Bounds protect against cycles in malformed outlines.
    let mut guard = 0u32;
    while let Some(item) = node {
        guard += 1;
        if depth > 64 || guard > 100_000 {
            break;
        }
        let title = match item.get_dict("Title")? {
            Some(t) if t.is_string()? => t.as_string_lossy()?,
            _ => String::new(),
        };
        let open = match item.get_dict("Count")? {
            Some(c) if c.is_number()? => c.as_int()? > 0,
            _ => false,
        };
        let dest = match item.get_dict("Dest")? {
            Some(d) => Some(d),
            None => match item.get_dict("A")? {
                Some(a) => a.get_dict("D")?,
                None => None,
            },
        };
        let (mut page, mut left, mut top) = (None, None, None);
        if let Some(d) = dest.filter(|d| d.is_array().unwrap_or(false)) {
            page = match d.get_array(0)? {
                Some(p) if p.is_dict()? => usize::try_from(doc.lookup_page_number(&p)?).ok(),
                Some(p) if p.is_number()? => usize::try_from(p.as_int()?).ok(),
                _ => None,
            };
            let is_xyz = match d.get_array(1)? {
                Some(kind) if kind.is_name()? => kind.as_name()? == b"XYZ",
                _ => false,
            };
            if is_xyz {
                left = d
                    .get_array(2)?
                    .map(|v| objects::number(&v))
                    .transpose()?
                    .flatten();
                top = d
                    .get_array(3)?
                    .map(|v| objects::number(&v))
                    .transpose()?
                    .flatten();
            }
        }
        let children = read_level(doc, item.get_dict("First")?, depth + 1)?;
        out.push(ReadOutlineItem {
            title,
            page,
            left,
            top,
            open,
            children,
        });
        node = item.get_dict("Next")?;
    }
    Ok(out)
}

/// Raw `/Dest` array values of an item, for tests: `[page_index, "XYZ", left, top, zoom]`.
pub fn dest_numbers(item: &PdfObject) -> Result<Option<Vec<Option<f64>>>> {
    let Some(dest) = item.get_dict("Dest")? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for i in 2..5 {
        out.push(match dest.get_array(i)? {
            Some(v) => objects::number(&v)?,
            None => None,
        });
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, open: bool, children: Vec<OutlineItem>) -> OutlineItem {
        OutlineItem {
            open,
            children,
            ..OutlineItem::new(title, 0)
        }
    }

    #[test]
    fn counts_follow_open_state() {
        let leaf = || item("leaf", false, vec![]);
        let open_mid = item("mid", true, vec![leaf(), leaf()]);
        let closed_mid = item("mid", false, vec![leaf(), leaf(), leaf()]);
        assert_eq!(count_for(&leaf()), None);
        assert_eq!(count_for(&open_mid), Some(2));
        assert_eq!(count_for(&closed_mid), Some(-3));
        // Open item with an open child (2 visible) and a closed child (hides 3): 1 + 2 + 1.
        let top = item("top", true, vec![open_mid.clone(), closed_mid.clone()]);
        assert_eq!(count_for(&top), Some(4));
        // Closed: same magnitude, negative.
        let top_closed = item("top", false, vec![open_mid, closed_mid]);
        assert_eq!(count_for(&top_closed), Some(-4));
    }
}
