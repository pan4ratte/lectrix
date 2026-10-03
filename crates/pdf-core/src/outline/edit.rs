//! Editing bookmarks in place (AGENTS.md section 6.2).
//!
//! Every edit changes only the keys it must: the edited item, the `/Prev`, `/Next`,
//! `/Parent`, `/First` and `/Last` links of the sibling lists it changes, and the `/Count`
//! of the items above the change. A key is written only when its value differs, because
//! MuPDF marks any object written to as changed, and an incremental save then rewrites it.
//! Items the user did not edit therefore keep their objects as they are, and nothing ever
//! touches their `/Dest`, `/A`, or any other key.

use mupdf::pdf::{PdfDocument, PdfObject};

use super::tree::Tree;
use super::{ViewDest, destination_from_view};
use crate::error::{Error, Result};
use crate::objects::text_string;

/// Adds a bookmark as child number `index` of `parent` (`None`: the top level; an index
/// past the end appends). Returns the new bookmark's id.
pub fn add(
    doc: &mut PdfDocument,
    parent: Option<u32>,
    index: usize,
    title: &str,
    dest: &ViewDest,
) -> Result<u32> {
    let mut tree = editable(doc)?;
    let parent = find_parent(&tree, parent)?;
    let title = clean_title(title)?;
    ensure_root(doc, &mut tree)?;

    // /Parent, /Prev and /Next are set by `relink`.
    let mut dict = doc.new_dict()?;
    dict.dict_put("Title", text_string(doc, &title)?)?;
    dict.dict_put("Dest", destination_from_view(doc, dest)?)?;
    let obj = doc.add_object(&dict)?;
    let num = obj.as_indirect()?;

    let node = tree.nodes.len();
    tree.nodes.push(super::tree::Node {
        obj,
        num,
        parent,
        children: Vec::new(),
        open: false,
    });
    insert(&mut tree, parent, index, node);
    relink(&tree, parent)?;
    fix_counts(&tree, parent)?;
    u32::try_from(num).map_err(|_| Error::InvalidArgument("bad object number".into()))
}

/// Gives a bookmark a new title.
pub fn rename(doc: &mut PdfDocument, id: u32, title: &str) -> Result<()> {
    let tree = editable(doc)?;
    let node = find(&tree, id)?;
    let title = clean_title(title)?;
    let mut obj = tree.nodes[node].obj.clone();
    let current = match obj.get_dict("Title")? {
        Some(t) if t.is_string()? => Some(t.as_string_lossy()?),
        _ => None,
    };
    if current.as_deref() != Some(title.as_str()) {
        obj.dict_put("Title", text_string(doc, &title)?)?;
    }
    Ok(())
}

/// Moves a bookmark (with its children) to child number `index` of `parent`, counted
/// after the bookmark has left its old place.
pub fn move_to(doc: &mut PdfDocument, id: u32, parent: Option<u32>, index: usize) -> Result<()> {
    let mut tree = editable(doc)?;
    let node = find(&tree, id)?;
    let new_parent = find_parent(&tree, parent)?;
    if let Some(p) = new_parent
        && tree.is_within(p, node)
    {
        return Err(Error::InvalidArgument(
            "a bookmark cannot be moved inside itself".into(),
        ));
    }
    let old_parent = tree.nodes[node].parent;
    tree.children_mut(old_parent).retain(|&c| c != node);
    insert(&mut tree, new_parent, index, node);
    tree.nodes[node].parent = new_parent;
    relink(&tree, old_parent)?;
    if new_parent != old_parent {
        relink(&tree, new_parent)?;
    }
    fix_counts(&tree, old_parent)?;
    if new_parent != old_parent {
        fix_counts(&tree, new_parent)?;
    }
    Ok(())
}

/// Removes a bookmark and its children from the outline. Their objects stay in the file
/// unreferenced (an optimized save drops them).
pub fn delete(doc: &mut PdfDocument, id: u32) -> Result<()> {
    let mut tree = editable(doc)?;
    let node = find(&tree, id)?;
    let parent = tree.nodes[node].parent;
    tree.children_mut(parent).retain(|&c| c != node);
    relink(&tree, parent)?;
    fix_counts(&tree, parent)
}

/// Points a bookmark at a new place: an explicit `/XYZ` destination replaces whatever
/// destination or action it had.
pub fn set_destination(doc: &mut PdfDocument, id: u32, dest: &ViewDest) -> Result<()> {
    let tree = editable(doc)?;
    let node = find(&tree, id)?;
    let mut obj = tree.nodes[node].obj.clone();
    obj.dict_put("Dest", destination_from_view(doc, dest)?)?;
    if obj.get_dict("A")?.is_some() {
        obj.dict_delete("A")?;
    }
    Ok(())
}

/// Writes which bookmarks are expanded. Items that are gone, have no children, or already
/// have the requested state are skipped. Returns how many items changed.
pub fn set_open(doc: &mut PdfDocument, states: &[(u32, bool)]) -> Result<usize> {
    let mut tree = editable(doc)?;
    let mut changed = Vec::new();
    for &(id, open) in states {
        if let Some(node) = tree.find(id)
            && !tree.nodes[node].children.is_empty()
            && tree.nodes[node].open != open
        {
            tree.nodes[node].open = open;
            changed.push(node);
        }
    }
    for &node in &changed {
        fix_counts(&tree, Some(node))?;
    }
    Ok(changed.len())
}

/// A bookmark built by the caller (combining files): its dictionary holds the title,
/// destination or action and style, and this module writes the links (`/Parent`,
/// `/Prev`, `/Next`, `/First`, `/Last`, `/Count`).
pub struct NewBookmark {
    /// A direct dictionary in the document the bookmark is added to.
    pub dict: PdfObject,
    pub open: bool,
    pub children: Vec<NewBookmark>,
}

/// Adds `items` (with their children) as children number `index`, `index + 1`... of
/// `parent` (`None`: the top level; an index past the end appends). The outline is created
/// if the document has none.
///
/// Every new item gets its object number first and is then written once, links and
/// `/Count` included. Only the existing items next to them, and the counts above, are
/// changed in place: in an undo step, each change to an existing object is recorded at a
/// cost that grows with the step (see `ffi::set_new_stream`), so thousands of new
/// bookmarks must not be patched key by key.
pub fn insert_items(
    doc: &mut PdfDocument,
    parent: Option<u32>,
    index: usize,
    items: Vec<NewBookmark>,
) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let mut tree = editable(doc)?;
    let parent = find_parent(&tree, parent)?;
    ensure_root(doc, &mut tree)?;
    let mut dicts = Vec::new();
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        nodes.push(reserve(doc, &mut tree, parent, item, &mut dicts)?);
    }
    for (k, &node) in nodes.iter().enumerate() {
        insert(&mut tree, parent, index.saturating_add(k), node);
    }
    let parent_obj = parent_obj(&tree, parent)?;
    for (node, mut dict) in dicts {
        let item = &tree.nodes[node];
        let up = match item.parent {
            Some(p) => tree.nodes[p].obj.clone(),
            None => parent_obj.clone(),
        };
        dict.dict_put("Parent", up)?;
        let siblings = tree.children(item.parent);
        let at = siblings.iter().position(|&n| n == node).unwrap_or(0);
        if at > 0 {
            dict.dict_put("Prev", tree.nodes[siblings[at - 1]].obj.clone())?;
        }
        if let Some(&next) = siblings.get(at + 1) {
            dict.dict_put("Next", tree.nodes[next].obj.clone())?;
        }
        if let (Some(&first), Some(&last)) = (item.children.first(), item.children.last()) {
            dict.dict_put("First", tree.nodes[first].obj.clone())?;
            dict.dict_put("Last", tree.nodes[last].obj.clone())?;
            let n = visible(&tree, Some(node));
            dict.dict_put("Count", PdfObject::new_int(if item.open { n } else { -n })?)?;
        }
        tree.nodes[node].obj.clone().write_object(&dict)?;
    }
    // The new items already hold the right values, so this changes only their existing
    // neighbours, the parent's /First and /Last, and the counts above.
    relink(&tree, parent)?;
    fix_counts(&tree, parent)
}

/// Gives a new item (and its children) an object number and a place in `tree`; its
/// dictionary is written later, once all links are known.
fn reserve(
    doc: &mut PdfDocument,
    tree: &mut Tree,
    parent: Option<usize>,
    item: NewBookmark,
    dicts: &mut Vec<(usize, PdfObject)>,
) -> Result<usize> {
    let obj = doc.create_object()?;
    let num = obj.as_indirect()?;
    let node = tree.nodes.len();
    tree.nodes.push(super::tree::Node {
        obj,
        num,
        parent,
        children: Vec::new(),
        open: item.open,
    });
    dicts.push((node, item.dict));
    for child in item.children {
        let c = reserve(doc, tree, Some(node), child, dicts)?;
        tree.nodes[node].children.push(c);
    }
    Ok(node)
}

/// Loads the tree, refusing damaged outlines.
fn editable(doc: &PdfDocument) -> Result<Tree> {
    let tree = Tree::load(doc)?;
    if tree.damaged {
        return Err(Error::DamagedOutline);
    }
    Ok(tree)
}

fn find(tree: &Tree, id: u32) -> Result<usize> {
    tree.find(id)
        .ok_or_else(|| Error::InvalidArgument(format!("bookmark {id} does not exist")))
}

fn find_parent(tree: &Tree, parent: Option<u32>) -> Result<Option<usize>> {
    parent.map(|id| find(tree, id)).transpose()
}

/// Bookmark titles are one line of text.
pub(crate) fn clean_title(title: &str) -> Result<String> {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return Err(Error::InvalidArgument("a bookmark needs a title".into()));
    }
    Ok(cleaned.to_owned())
}

fn insert(tree: &mut Tree, parent: Option<usize>, index: usize, node: usize) {
    if let Some(p) = parent
        && tree.nodes[p].children.is_empty()
    {
        // A bookmark that gains its first child opens, so the new child is visible.
        tree.nodes[p].open = true;
    }
    let list = tree.children_mut(parent);
    let index = index.min(list.len());
    list.insert(index, node);
}

/// The outline dictionary, created (and linked from the catalog) if the document has none.
fn ensure_root(doc: &mut PdfDocument, tree: &mut Tree) -> Result<()> {
    if tree.root.is_some() {
        return Ok(());
    }
    let mut root = doc.new_dict()?;
    root.dict_put("Type", PdfObject::new_name("Outlines")?)?;
    let root = doc.add_object(&root)?;
    doc.catalog()?.dict_put("Outlines", root.clone())?;
    tree.root = Some(root);
    Ok(())
}

fn parent_obj(tree: &Tree, parent: Option<usize>) -> Result<PdfObject> {
    match parent {
        Some(p) => Ok(tree.nodes[p].obj.clone()),
        None => tree
            .root
            .clone()
            .ok_or_else(|| Error::InvalidArgument("the document has no outline".into())),
    }
}

/// Makes the links of `parent`'s child list match the tree: the parent's `/First` and
/// `/Last`, and each child's `/Parent`, `/Prev` and `/Next`.
fn relink(tree: &Tree, parent: Option<usize>) -> Result<()> {
    let mut parent_obj = parent_obj(tree, parent)?;
    let kids = tree.children(parent);
    let obj = |i: usize| &tree.nodes[kids[i]].obj;
    set_ref(&mut parent_obj, "First", kids.first().map(|_| obj(0)))?;
    set_ref(
        &mut parent_obj,
        "Last",
        kids.last().map(|_| obj(kids.len() - 1)),
    )?;
    for i in 0..kids.len() {
        let mut item = obj(i).clone();
        set_ref(&mut item, "Parent", Some(&parent_obj))?;
        set_ref(&mut item, "Prev", (i > 0).then(|| obj(i - 1)))?;
        set_ref(&mut item, "Next", (i + 1 < kids.len()).then(|| obj(i + 1)))?;
    }
    Ok(())
}

/// Number of items visible below `parent` when it is open.
fn visible(tree: &Tree, parent: Option<usize>) -> i32 {
    tree.children(parent)
        .iter()
        .map(|&c| {
            1 + if tree.nodes[c].open {
                visible(tree, Some(c))
            } else {
                0
            }
        })
        .sum()
}

/// Recomputes `/Count` for `from` and every item above it, and for the outline
/// dictionary: the visible descendants, negative when the item is closed (no `/Count` for
/// an item without children).
fn fix_counts(tree: &Tree, from: Option<usize>) -> Result<()> {
    let mut current = from;
    while let Some(i) = current {
        write_count(tree, i)?;
        current = tree.nodes[i].parent;
    }
    let mut root = parent_obj(tree, None)?;
    let count = (!tree.top.is_empty()).then(|| visible(tree, None));
    set_int(&mut root, "Count", count)
}

/// Writes one item's `/Count` (see [`fix_counts`]).
fn write_count(tree: &Tree, i: usize) -> Result<()> {
    let node = &tree.nodes[i];
    let count = if node.children.is_empty() {
        None
    } else {
        let n = visible(tree, Some(i));
        Some(if node.open { n } else { -n })
    };
    set_int(&mut node.obj.clone(), "Count", count)
}

/// Sets `dict[key]` to a reference to `target` (or removes it), unless it already is one.
fn set_ref(dict: &mut PdfObject, key: &str, target: Option<&PdfObject>) -> Result<()> {
    let current = dict.get_dict(key)?;
    match (current, target) {
        (None, None) => Ok(()),
        (Some(c), Some(t)) if c.is_indirect()? && c.as_indirect()? == t.as_indirect()? => Ok(()),
        (_, Some(t)) => Ok(dict.dict_put(key, t.clone())?),
        (Some(_), None) => Ok(dict.dict_delete(key)?),
    }
}

/// Sets `dict[key]` to an integer (or removes it), unless it already has that value.
fn set_int(dict: &mut PdfObject, key: &str, value: Option<i32>) -> Result<()> {
    let current = match dict.get_dict(key)? {
        Some(c) if c.is_int()? => Some(Some(c.as_int()?)),
        Some(_) => Some(None),
        None => None,
    };
    match (current, value) {
        (None, None) => Ok(()),
        (Some(Some(c)), Some(v)) if c == v => Ok(()),
        (_, Some(v)) => Ok(dict.dict_put(key, PdfObject::new_int(v)?)?),
        (Some(_), None) => Ok(dict.dict_delete(key)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_one_trimmed_line() {
        assert_eq!(clean_title("  Chapter\r\n1\t").unwrap(), "Chapter  1");
        assert!(clean_title(" \n ").is_err());
    }
}
