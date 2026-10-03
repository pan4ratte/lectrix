//! A source's bookmarks, copied for the combined document.
//!
//! Each bookmark keeps its title, style and destination or action (copied, so named
//! destinations, web links and scripts stay as they were). A bookmark whose page was not
//! picked loses its destination; it is left out unless it has children, which keep it as a
//! heading.

use std::collections::HashMap;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::copy::Copier;
use super::names::{self, DestIndex, PageRef};
use crate::error::Result;
use crate::outline::edit::NewBookmark;
use crate::outline::tree::Tree;

/// Keys copied from a source bookmark (the links are written anew).
const COPIED: [&str; 3] = ["Title", "C", "F"];
const TARGET: [&str; 2] = ["Dest", "A"];

pub(crate) struct Copied {
    pub items: Vec<NewBookmark>,
    /// Bookmarks left out because their page was not picked.
    pub dropped: usize,
}

/// What decides whether a source's link or bookmark still has its page.
pub(crate) struct Targets<'a> {
    pub src: &'a PdfDocument,
    /// Source page index to destination page object number, for picked pages.
    pub picked: &'a HashMap<usize, i32>,
    pub dests: &'a DestIndex,
}

pub(crate) fn copy(
    dst: &mut PdfDocument,
    copier: &mut Copier,
    targets: &Targets<'_>,
) -> Result<Copied> {
    let tree = Tree::load(targets.src)?;
    let mut out = Copied {
        items: Vec::new(),
        dropped: 0,
    };
    for &i in &tree.top {
        if let Some(item) = copy_node(dst, &tree, i, copier, targets, &mut out.dropped)? {
            out.items.push(item);
        }
    }
    Ok(out)
}

fn copy_node(
    dst: &mut PdfDocument,
    tree: &Tree,
    index: usize,
    copier: &mut Copier,
    targets: &Targets<'_>,
    dropped: &mut usize,
) -> Result<Option<NewBookmark>> {
    let node = &tree.nodes[index];
    let mut children = Vec::new();
    for &c in &node.children {
        if let Some(child) = copy_node(dst, tree, c, copier, targets, dropped)? {
            children.push(child);
        }
    }
    let removed = target_removed(&node.obj, copier, targets)?;
    if removed && children.is_empty() {
        *dropped += 1;
        return Ok(None);
    }
    let mut dict = dst.new_dict()?;
    let keys = COPIED
        .iter()
        .chain(if removed { [].iter() } else { TARGET.iter() });
    for key in keys {
        if let Some(value) = node.obj.get_dict(*key)?
            && let Some(copied) = copier.copy(dst, &value)?
        {
            dict.dict_put(*key, copied)?;
        }
    }
    if dict.get_dict("Title")?.is_none() {
        dict.dict_put("Title", PdfObject::new_string("")?)?;
    }
    Ok(Some(NewBookmark {
        dict,
        open: node.open,
        children,
    }))
}

/// True if the link or bookmark leads to a page of `src` that is not being copied.
/// Destinations that lead nowhere in the source stay as they are.
pub(crate) fn target_removed(
    item: &PdfObject,
    copier: &Copier,
    targets: &Targets<'_>,
) -> Result<bool> {
    let Some(dest) = names::goto_dest(item)? else {
        return Ok(false);
    };
    Ok(match targets.dests.page(&dest)? {
        PageRef::Object(num) => copier.is_dropped(num),
        PageRef::Index(i) => {
            let count = usize::try_from(targets.src.page_count()?).unwrap_or(0);
            i < count && !targets.picked.contains_key(&i)
        }
        PageRef::Unknown => false,
    })
}
