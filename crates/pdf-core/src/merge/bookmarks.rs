//! A source's bookmarks, copied for the combined document.
//!
//! Each bookmark keeps its title, style and destination or action (copied, so named
//! destinations, web links and scripts stay as they were). A bookmark whose page was not
//! picked loses its destination; it is left out unless it has children, which keep it as a
//! heading.

use std::collections::HashMap;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::copy::Copier;
use super::names::{self, PageRef};
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

pub(crate) fn copy(
    dst: &mut PdfDocument,
    src: &PdfDocument,
    copier: &mut Copier,
    picked: &HashMap<usize, i32>,
) -> Result<Copied> {
    let tree = Tree::load(src)?;
    let mut out = Copied {
        items: Vec::new(),
        dropped: 0,
    };
    for &i in &tree.top {
        if let Some(item) = copy_node(dst, src, &tree, i, copier, picked, &mut out.dropped)? {
            out.items.push(item);
        }
    }
    Ok(out)
}

fn copy_node(
    dst: &mut PdfDocument,
    src: &PdfDocument,
    tree: &Tree,
    index: usize,
    copier: &mut Copier,
    picked: &HashMap<usize, i32>,
    dropped: &mut usize,
) -> Result<Option<NewBookmark>> {
    let node = &tree.nodes[index];
    let mut children = Vec::new();
    for &c in &node.children {
        if let Some(child) = copy_node(dst, src, tree, c, copier, picked, dropped)? {
            children.push(child);
        }
    }
    let removed = target_removed(src, &node.obj, copier, picked)?;
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
    src: &PdfDocument,
    item: &PdfObject,
    copier: &Copier,
    picked: &HashMap<usize, i32>,
) -> Result<bool> {
    let Some(dest) = names::goto_dest(item)? else {
        return Ok(false);
    };
    let Some(array) = names::resolve_dest(src, &dest)? else {
        return Ok(false);
    };
    Ok(match names::dest_page(&array)? {
        PageRef::Object(num) => copier.is_dropped(num),
        PageRef::Index(i) => {
            let count = usize::try_from(src.page_count()?).unwrap_or(0);
            i < count && !picked.contains_key(&i)
        }
        PageRef::Unknown => false,
    })
}
