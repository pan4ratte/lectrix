//! Moving and deleting pages of a document (ADR 0018).
//!
//! Both edit the page tree in place and change nothing else in the catalog. MuPDF's own
//! `pdf_rearrange_pages` is not used: it writes a new catalog that keeps only the page
//! tree, the outline, the layers and the named destinations, so page labels, the form,
//! the metadata and attached files would be lost.
//!
//! The tree is read into a small model ([`PageTree`]), changed there, and written back:
//! only the `/Kids` of the nodes that lost or gained pages, the `/Count` of those nodes and
//! the ones above them, and the `/Parent` of pages that changed node. A page that changes
//! node first gets the values it inherited from its old one (`/MediaBox`, `/CropBox`,
//! `/Rotate`, `/Resources`) written on itself, so it looks the same in its new place.
//!
//! Links, bookmarks and named destinations refer to pages by their objects, so they follow
//! a moved page by themselves. When pages are deleted, what led to them goes too (section
//! 6.4 does the same for pages left out of a combined file): links are removed, a bookmark
//! is removed unless it has children (then it stays as a heading), named destinations are
//! removed, and so are the form fields whose widgets were on those pages. Page labels
//! belong to positions: moving pages leaves the rules alone, and deleting pages moves the
//! rules after them up, so the numbering stays continuous.
//!
//! Some producers write a destination with a page number where the format wants a
//! reference to the page, and MuPDF follows those: once pages change places they would
//! lead to whatever page has that number. So both edits give every such destination the
//! reference in its place, which means going through every link, bookmark and named
//! destination of the document. A document that has been gone through has none left, and
//! the caller can say so the next time ([`move_pages`]), which then moves its pages and
//! looks at nothing else.

use std::cell::Cell;
use std::collections::HashSet;

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::Rect;
use crate::labels::{self, LabelRule};
use crate::merge::names::{self, DestIndex, PageRef};
use crate::objects::{dict_entries, rect_array};
use crate::outline;

/// Deepest page tree read; deeper ones (in malformed files) are refused.
const MAX_DEPTH: usize = 64;
/// Nesting limit for form field trees in malformed files.
const MAX_FIELD_DEPTH: u32 = 32;

/// The page keys a page can take from the nodes above it.
const INHERITED: [&str; 4] = ["MediaBox", "CropBox", "Rotate", "Resources"];

/// What deleting pages took out with them, for telling the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeleteReport {
    pub pages: usize,
    /// Links on other pages that led to the deleted ones.
    pub links: usize,
    /// Bookmarks that led to them (ones with children stay as headings).
    pub bookmarks: usize,
    /// Form fields that were on them.
    pub fields: usize,
}

/// What moving pages did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveReport {
    /// False, with nothing written, if the move leaves every page where it is.
    pub moved: bool,
    /// How many destinations written with a page number got a reference to their page in
    /// its place; `None` if they were not looked for.
    pub renumbered: Option<usize>,
}

/// Moves `pages` (0-based; they keep their order in the document) into the gap `to` of the
/// current order: 0 is before the first page, the page count after the last. With
/// `by_reference`, the caller knows that no destination in the document is written with a
/// page number (an earlier move or deletion went through them all), and none is looked
/// for. The caller wraps this in one journal step.
pub fn move_pages(
    doc: &mut PdfDocument,
    pages: &[usize],
    to: usize,
    by_reference: bool,
) -> Result<MoveReport> {
    let unmoved = MoveReport {
        moved: false,
        renumbered: None,
    };
    let mut tree = PageTree::load(doc)?;
    let count = tree.leaves.len();
    let moved = checked(pages, count)?;
    if to > count {
        return Err(Error::PageOutOfRange(to));
    }
    let picked: HashSet<usize> = moved.iter().copied().collect();
    let rest: Vec<usize> = (0..count).filter(|p| !picked.contains(p)).collect();
    let before = rest.partition_point(|&p| p < to);
    let order = rest[..before]
        .iter()
        .chain(&moved)
        .chain(&rest[before..])
        .copied();
    if order.eq(0..count) {
        return Ok(unmoved);
    }
    tree.detach(&picked);
    // The pages go into the node of the page they land next to: before the page that
    // follows them, or after the last page.
    let (neighbour, after) = match rest.get(before) {
        Some(&next) => (next, false),
        None => match rest.last() {
            Some(&last) => (last, true),
            // Every page was picked, which leaves the order as it is (handled above).
            None => return Ok(unmoved),
        },
    };
    let node = tree.leaves[neighbour].node;
    let at = tree.nodes[node]
        .kids
        .iter()
        .position(|k| *k == Kid::Leaf(neighbour))
        .ok_or(Error::DamagedPageTree)?
        + usize::from(after);
    tree.nodes[node]
        .kids
        .splice(at..at, moved.iter().map(|&l| Kid::Leaf(l)));
    tree.changed[node] = true;

    let parent = tree.nodes[node].obj.clone();
    for &l in &moved {
        if tree.leaves[l].node == node {
            continue;
        }
        let mut page = tree.leaves[l].obj.clone();
        keep_inherited(doc, &mut page, &parent)?;
        page.dict_put("Parent", parent.clone())?;
        tree.leaves[l].node = node;
    }
    tree.write(doc)?;

    // Destinations written with a page number would now lead to whatever page has that
    // number (see the module documentation).
    let renumbered = if by_reference {
        None
    } else {
        let targets = Targets::new(doc, &tree, HashSet::new())?;
        targets.fix(doc, tree.leaves.iter().map(|l| &l.obj))?;
        Some(targets.renumbered.get())
    };
    Ok(MoveReport {
        moved: true,
        renumbered,
    })
}

/// Deletes `pages` (0-based) with what leads to them (see the module documentation). At
/// least one page must stay. Afterwards no destination in the document is written with a
/// page number. The caller wraps this in one journal step.
pub fn delete_pages(doc: &mut PdfDocument, pages: &[usize]) -> Result<DeleteReport> {
    let mut tree = PageTree::load(doc)?;
    let count = tree.leaves.len();
    let deleted = checked(pages, count)?;
    if deleted.len() == count {
        return Err(Error::InvalidArgument(
            "a document needs at least one page".into(),
        ));
    }
    let rules = labels::read_rules(doc)?;
    let picked: HashSet<usize> = deleted.iter().copied().collect();

    let mut widgets = Vec::new();
    for &l in &deleted {
        widgets.extend(widgets_on(&tree.leaves[l].obj)?);
    }
    tree.detach(&picked);
    tree.write(doc)?;

    let gone: HashSet<i32> = deleted.iter().map(|&l| tree.leaves[l].num).collect();
    let targets = Targets::new(doc, &tree, gone)?;
    let remaining = tree
        .leaves
        .iter()
        .enumerate()
        .filter(|(l, _)| !picked.contains(l))
        .map(|(_, leaf)| &leaf.obj);
    let (links, bookmarks) = targets.fix(doc, remaining)?;
    let fields = remove_widgets(doc, &widgets)?;
    // Whatever still refers to a deleted page (a tagged file's structure tree, an article
    // thread) must not keep its content in the file: an optimized save drops what nothing
    // refers to, and the page's content is reached through the page.
    for &l in &deleted {
        empty_page(&tree.leaves[l].obj)?;
    }

    let shifted = rules_without(&rules, &deleted, count - deleted.len());
    if shifted != rules {
        labels::set_rules(doc, shifted)?;
    }
    Ok(DeleteReport {
        pages: deleted.len(),
        links,
        bookmarks,
        fields,
    })
}

/// `pages` sorted, each once; an error if there are none or one does not exist.
fn checked(pages: &[usize], count: usize) -> Result<Vec<usize>> {
    let mut pages = pages.to_vec();
    pages.sort_unstable();
    pages.dedup();
    match pages.last() {
        None => Err(Error::InvalidArgument("no pages were picked".into())),
        Some(&last) if last >= count => Err(Error::PageOutOfRange(last)),
        Some(_) => Ok(pages),
    }
}

/// The label rules once the pages in `deleted` (sorted) are gone: a rule starts as many
/// pages earlier as were deleted before it. A rule left without pages (the next one now
/// starts where it does, or it starts after the last page) is dropped.
fn rules_without(rules: &[LabelRule], deleted: &[usize], page_count: usize) -> Vec<LabelRule> {
    let mut out: Vec<LabelRule> = Vec::with_capacity(rules.len());
    for rule in rules {
        let start_page = rule.start_page - deleted.partition_point(|&p| p < rule.start_page);
        if start_page >= page_count {
            continue;
        }
        if out.last().is_some_and(|last| last.start_page == start_page) {
            out.pop();
        }
        out.push(LabelRule {
            start_page,
            ..rule.clone()
        });
    }
    out
}

// ----- the page tree -----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kid {
    Node(usize),
    Leaf(usize),
}

/// A `/Pages` node.
struct Node {
    obj: PdfObject,
    parent: Option<usize>,
    kids: Vec<Kid>,
}

/// A page.
struct Leaf {
    obj: PdfObject,
    num: i32,
    /// The node whose `/Kids` lists it.
    node: usize,
}

/// The page tree as its `/Kids` arrays give it: nodes in the order they are reached (a
/// node's children come after it), pages in document order.
struct PageTree {
    nodes: Vec<Node>,
    leaves: Vec<Leaf>,
    /// Nodes whose list of kids differs from the file's.
    changed: Vec<bool>,
    /// Nodes taken out of the tree because no page is left below them.
    pruned: Vec<bool>,
}

impl PageTree {
    /// Reads the tree. Refused: a tree MuPDF reads differently (it decides which page has
    /// which number), a cycle, a page listed twice, and pages or nodes that are not
    /// indirect objects, which `/Parent` could not refer to.
    fn load(doc: &PdfDocument) -> Result<PageTree> {
        let root = doc
            .catalog()?
            .get_dict("Pages")?
            .ok_or(Error::DamagedPageTree)?;
        let mut tree = PageTree {
            nodes: Vec::new(),
            leaves: Vec::new(),
            changed: Vec::new(),
            pruned: Vec::new(),
        };
        let mut seen = HashSet::new();
        tree.read(root, None, 0, &mut seen)?;
        tree.changed = vec![false; tree.nodes.len()];
        tree.pruned = vec![false; tree.nodes.len()];

        let count = usize::try_from(doc.page_count()?).unwrap_or(0);
        if tree.leaves.len() != count {
            return Err(Error::DamagedPageTree);
        }
        for (i, leaf) in tree.leaves.iter().enumerate() {
            let index = i32::try_from(i).map_err(|_| Error::PageOutOfRange(i))?;
            let page = doc.find_page(index)?;
            if !page.is_indirect()? || page.as_indirect()? != leaf.num {
                return Err(Error::DamagedPageTree);
            }
        }
        Ok(tree)
    }

    fn read(
        &mut self,
        obj: PdfObject,
        parent: Option<usize>,
        depth: usize,
        seen: &mut HashSet<i32>,
    ) -> Result<usize> {
        if depth > MAX_DEPTH
            || !obj.is_indirect()?
            || !obj.is_dict()?
            || !seen.insert(obj.as_indirect()?)
        {
            return Err(Error::DamagedPageTree);
        }
        let index = self.nodes.len();
        let kids = obj.get_dict("Kids")?;
        self.nodes.push(Node {
            obj,
            parent,
            kids: Vec::new(),
        });
        let Some(kids) = kids.filter(|k| k.is_array().unwrap_or(false)) else {
            return Ok(index);
        };
        for i in 0..i32::try_from(kids.len()?).unwrap_or(i32::MAX) {
            let kid = kids.get_array(i)?.ok_or(Error::DamagedPageTree)?;
            if is_node(&kid)? {
                let child = self.read(kid, Some(index), depth + 1, seen)?;
                self.nodes[index].kids.push(Kid::Node(child));
            } else {
                if !kid.is_indirect()? || !kid.is_dict()? || !seen.insert(kid.as_indirect()?) {
                    return Err(Error::DamagedPageTree);
                }
                self.nodes[index].kids.push(Kid::Leaf(self.leaves.len()));
                self.leaves.push(Leaf {
                    num: kid.as_indirect()?,
                    obj: kid,
                    node: index,
                });
            }
        }
        Ok(index)
    }

    /// Takes the pages in `gone` out of their nodes. A node left without pages goes too
    /// (never the root), and so on upwards.
    fn detach(&mut self, gone: &HashSet<usize>) {
        for (i, node) in self.nodes.iter_mut().enumerate() {
            let before = node.kids.len();
            node.kids
                .retain(|k| !matches!(k, Kid::Leaf(l) if gone.contains(l)));
            if node.kids.len() != before {
                self.changed[i] = true;
            }
        }
        // A node's children come after it, so going backwards empties from the bottom up.
        for i in (1..self.nodes.len()).rev() {
            if !self.changed[i] || !self.nodes[i].kids.is_empty() {
                continue;
            }
            if let Some(parent) = self.nodes[i].parent {
                self.nodes[parent].kids.retain(|k| *k != Kid::Node(i));
                self.changed[parent] = true;
                self.pruned[i] = true;
            }
        }
    }

    /// Writes the model back: `/Kids` of the changed nodes, and `/Count` of those and of
    /// the nodes above them where it differs. Nothing else is touched.
    fn write(&self, doc: &PdfDocument) -> Result<()> {
        let n = self.nodes.len();
        let mut counts = vec![0usize; n];
        for i in (0..n).rev() {
            counts[i] = self.nodes[i]
                .kids
                .iter()
                .map(|k| match k {
                    Kid::Node(c) => counts[*c],
                    Kid::Leaf(_) => 1,
                })
                .sum();
        }
        let mut affected = vec![false; n];
        for i in 0..n {
            if !self.changed[i] || self.pruned[i] {
                continue;
            }
            let mut current = Some(i);
            while let Some(c) = current {
                if affected[c] {
                    break;
                }
                affected[c] = true;
                current = self.nodes[c].parent;
            }
        }
        for i in (0..n).filter(|&i| affected[i]) {
            let node = &self.nodes[i];
            let mut obj = node.obj.clone();
            if self.changed[i] {
                let mut kids =
                    doc.new_array_with_capacity(i32::try_from(node.kids.len()).unwrap_or(0))?;
                for kid in &node.kids {
                    kids.array_push(match kid {
                        Kid::Node(c) => self.nodes[*c].obj.clone(),
                        Kid::Leaf(l) => self.leaves[*l].obj.clone(),
                    })?;
                }
                obj.dict_put("Kids", kids)?;
            }
            let count = i32::try_from(counts[i])
                .map_err(|_| Error::InvalidArgument("too many pages".into()))?;
            let stored = match obj.get_dict("Count")? {
                Some(c) if c.is_int()? => Some(c.as_int()?),
                _ => None,
            };
            if stored != Some(count) {
                obj.dict_put("Count", PdfObject::new_int(count)?)?;
            }
        }
        Ok(())
    }
}

/// True if a kid of a `/Pages` node is another node, as MuPDF tells them apart
/// (`pdf_lookup_page_loc`): by `/Type`, and without one, by having kids and no media box.
fn is_node(kid: &PdfObject) -> Result<bool> {
    if !kid.is_dict()? {
        return Ok(false);
    }
    match kid.get_dict("Type")? {
        Some(t) if t.is_name()? => Ok(t.as_name()? == b"Pages"),
        _ => Ok(kid.get_dict("Kids")?.is_some() && kid.get_dict("MediaBox")?.is_none()),
    }
}

/// Writes on `page` the values it takes from the nodes above it, before it moves under
/// `new_parent`. Where it took none and its new place defines one, it gets the value a
/// page without one has (no rotation, the media box as crop box, no resources), so it does
/// not take its new neighbours'.
fn keep_inherited(doc: &PdfDocument, page: &mut PdfObject, new_parent: &PdfObject) -> Result<()> {
    for key in INHERITED {
        if page.get_dict(key)?.is_some() {
            continue;
        }
        let value = match page.get_dict_inheritable(key)? {
            // A reference: the page can refer to the same object.
            Some(v) if v.is_indirect()? => Some(v),
            // A copy (`try_clone` is deep): a direct object belongs to one container.
            Some(v) => Some(v.try_clone()?),
            None if new_parent.get_dict_inheritable(key)?.is_some() => Some(match key {
                "Rotate" => PdfObject::new_int(0)?,
                "Resources" => doc.new_dict()?,
                // "MediaBox" comes first, so a page that reaches "CropBox" has one.
                "CropBox" => match page.get_dict("MediaBox")? {
                    Some(media) => media.try_clone()?,
                    None => continue,
                },
                // What readers assume for a page without one (US Letter).
                _ => rect_array(doc, Rect::new(0.0, 0.0, 612.0, 792.0))?,
            }),
            None => None,
        };
        if let Some(value) = value {
            page.dict_put(key, value)?;
        }
    }
    Ok(())
}

/// Leaves a deleted page's object with its `/Type` alone.
fn empty_page(page: &PdfObject) -> Result<()> {
    let mut page = page.clone();
    for (key, _) in dict_entries(&page)? {
        if key.as_name()? != b"Type" {
            page.dict_delete(key)?;
        }
    }
    Ok(())
}

// ----- what leads to pages -----

/// Decides what happens to destinations after pages moved or were deleted.
struct Targets<'a> {
    /// The document's named destinations with their values, as they were before the
    /// change.
    names: Vec<(Vec<u8>, PdfObject)>,
    /// The same by name, to tell where a link or bookmark that uses a name leads. Empty
    /// when no page was deleted: none of them leads to a page that is gone then.
    dests: DestIndex,
    /// The pages by their index before the change.
    old_pages: &'a [Leaf],
    /// Object numbers of the deleted pages.
    gone: HashSet<i32>,
    /// Destinations written with a page number that got a reference in its place.
    renumbered: Cell<usize>,
}

impl<'a> Targets<'a> {
    fn new(doc: &PdfDocument, tree: &'a PageTree, gone: HashSet<i32>) -> Result<Targets<'a>> {
        let names = names::collect(doc)?;
        let dests = if gone.is_empty() {
            DestIndex::empty()
        } else {
            DestIndex::from_entries(&names)?
        };
        Ok(Targets {
            names,
            dests,
            old_pages: &tree.leaves,
            gone,
            renumbered: Cell::new(0),
        })
    }

    /// Goes through the links on `pages` (the pages still in the document), the
    /// bookmarks and the named destinations: what leads to a deleted page is removed, and
    /// a destination written with a page number gets a reference to that page in its
    /// place. Returns how many links and bookmarks were removed.
    fn fix<'p>(
        &self,
        doc: &mut PdfDocument,
        pages: impl Iterator<Item = &'p PdfObject>,
    ) -> Result<(usize, usize)> {
        let mut links = 0;
        for page in pages {
            let Some(mut annots) = page.get_dict("Annots")? else {
                continue;
            };
            if !annots.is_array()? {
                continue;
            }
            let mut i = i32::try_from(annots.len()?).unwrap_or(i32::MAX);
            while i > 0 {
                i -= 1;
                let Some(annot) = annots.get_array(i)? else {
                    continue;
                };
                if !annot.is_dict()? || !is_subtype(&annot, b"Link")? {
                    continue;
                }
                if self.dead(&annot)? {
                    annots.array_delete(i)?;
                    links += 1;
                }
            }
        }
        let bookmarks = match outline::edit::remove_dead(doc, &mut |item| self.dead(item)) {
            Ok(removed) => removed,
            // A damaged outline is shown but never edited (section 6.2).
            Err(Error::DamagedOutline) => 0,
            Err(e) => return Err(e),
        };
        let mut gone_names = HashSet::new();
        for (name, value) in &self.names {
            let array = if value.is_dict()? {
                value.get_dict("D")?
            } else {
                Some(value.clone())
            };
            if let Some(mut array) = array.filter(|a| a.is_array().unwrap_or(false))
                && self.dead_array(&mut array)?
            {
                gone_names.insert(name.clone());
            }
        }
        names::remove_dests(doc, &gone_names)?;
        Ok((links, bookmarks))
    }

    /// True if a link or bookmark leads to a deleted page.
    fn dead(&self, item: &PdfObject) -> Result<bool> {
        let Some(mut dest) = names::goto_dest(item)? else {
            return Ok(false);
        };
        if dest.is_array()? {
            return self.dead_array(&mut dest);
        }
        Ok(match self.dests.page(&dest)? {
            PageRef::Object(num) => self.gone.contains(&num),
            PageRef::Index(i) => self
                .old_pages
                .get(i)
                .is_some_and(|page| self.gone.contains(&page.num)),
            PageRef::Unknown => false,
        })
    }

    /// True if a destination array leads to a deleted page. One written with a page number
    /// is rewritten in place (`dest` is a handle to the array in its container).
    fn dead_array(&self, dest: &mut PdfObject) -> Result<bool> {
        match names::dest_page(dest)? {
            PageRef::Object(num) => Ok(self.gone.contains(&num)),
            PageRef::Index(i) => match self.old_pages.get(i) {
                Some(page) if self.gone.contains(&page.num) => Ok(true),
                Some(page) => {
                    dest.array_put(0, page.obj.clone())?;
                    self.renumbered.set(self.renumbered.get() + 1);
                    Ok(false)
                }
                None => Ok(false),
            },
            PageRef::Unknown => Ok(false),
        }
    }
}

fn is_subtype(annot: &PdfObject, subtype: &[u8]) -> Result<bool> {
    Ok(match annot.get_dict("Subtype")? {
        Some(s) if s.is_name()? => s.as_name()? == subtype,
        _ => false,
    })
}

// ----- form fields -----

/// The form widgets among a page's annotations.
fn widgets_on(page: &PdfObject) -> Result<Vec<PdfObject>> {
    let mut out = Vec::new();
    if let Some(annots) = page.get_dict("Annots")?
        && annots.is_array()?
    {
        for annot in crate::objects::array_items(&annots)? {
            if annot.is_indirect()? && annot.is_dict()? && is_subtype(&annot, b"Widget")? {
                out.push(annot);
            }
        }
    }
    Ok(out)
}

/// Takes the widgets of deleted pages out of the form. A field left without widgets is
/// removed, and so are the fields above it that are left with nothing below them. Returns
/// how many fields went.
fn remove_widgets(doc: &PdfDocument, widgets: &[PdfObject]) -> Result<usize> {
    if widgets.is_empty() {
        return Ok(0);
    }
    let Some(form) = doc
        .catalog()?
        .get_dict("AcroForm")?
        .filter(|f| f.is_dict().unwrap_or(false))
    else {
        return Ok(0);
    };
    let mut removed = HashSet::new();
    let mut fields = 0;
    for widget in widgets {
        // A widget with a name is its field; one without is one of a field's widgets.
        let is_field = widget.get_dict("T")?.is_some();
        let (emptied, top) = unlink(widget, &mut removed, 0)?;
        if is_field || emptied {
            fields += 1;
        }
        if let Some(top) = top
            && let Some(mut list) = form.get_dict("Fields")?
        {
            remove_ref(&mut list, top)?;
        }
    }
    // The calculation order lists fields too.
    if let Some(mut order) = form.get_dict("CO")? {
        for num in &removed {
            remove_ref(&mut order, *num)?;
        }
    }
    Ok(fields)
}

/// Takes `obj` out of its parent field's `/Kids`, and the parent out of its own parent's
/// if that left it without kids, and so on. Returns whether `obj`'s parent was emptied, and
/// the object number to take out of the form's `/Fields` when the top was reached.
fn unlink(obj: &PdfObject, removed: &mut HashSet<i32>, depth: u32) -> Result<(bool, Option<i32>)> {
    let num = obj.as_indirect()?;
    removed.insert(num);
    let parent = obj
        .get_dict("Parent")?
        .filter(|p| p.is_dict().unwrap_or(false) && p.is_indirect().unwrap_or(false));
    let Some(parent) = parent.filter(|_| depth < MAX_FIELD_DEPTH) else {
        return Ok((false, Some(num)));
    };
    let Some(mut kids) = parent.get_dict("Kids")? else {
        return Ok((false, None));
    };
    remove_ref(&mut kids, num)?;
    if kids.is_array()? && kids.len()? == 0 {
        let (_, top) = unlink(&parent, removed, depth + 1)?;
        return Ok((true, top));
    }
    Ok((false, None))
}

/// Removes the references to object `num` from `array`.
fn remove_ref(array: &mut PdfObject, num: i32) -> Result<()> {
    if !array.is_array()? {
        return Ok(());
    }
    let mut i = i32::try_from(array.len()?).unwrap_or(i32::MAX);
    while i > 0 {
        i -= 1;
        if let Some(item) = array.get_array(i)?
            && item.is_indirect()?
            && item.as_indirect()? == num
        {
            array.array_delete(i)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::LabelStyle;

    fn rule(start_page: usize, style: LabelStyle) -> LabelRule {
        LabelRule {
            start_page,
            style,
            prefix: String::new(),
            first_number: 1,
        }
    }

    #[test]
    fn label_rules_move_up_as_pages_before_them_go() {
        let rules = [
            rule(0, LabelStyle::LowerRoman),
            rule(4, LabelStyle::Decimal),
            rule(10, LabelStyle::UpperLetters),
        ];
        // Two front-matter pages and one body page: the body starts two pages earlier,
        // the appendix three.
        let starts = |deleted: &[usize], count| -> Vec<usize> {
            rules_without(&rules, deleted, count)
                .iter()
                .map(|r| r.start_page)
                .collect()
        };
        assert_eq!(starts(&[1, 2, 6], 9), [0, 2, 7]);
        // Nothing before a rule: it stays.
        assert_eq!(starts(&[11], 11), [0, 4, 10]);
    }

    #[test]
    fn label_rules_without_pages_are_dropped() {
        let rules = [
            rule(0, LabelStyle::LowerRoman),
            rule(2, LabelStyle::Decimal),
            rule(4, LabelStyle::UpperLetters),
        ];
        // The whole front matter: the body's rule takes the first page.
        let kept = rules_without(&rules, &[0, 1], 4);
        assert_eq!(kept.len(), 2);
        assert_eq!(
            (kept[0].start_page, kept[0].style),
            (0, LabelStyle::Decimal)
        );
        assert_eq!(kept[1].start_page, 2);
        // The whole body: its rule is gone, the appendix follows the front matter.
        let kept = rules_without(&rules, &[2, 3], 4);
        assert_eq!(
            kept.iter().map(|r| r.style).collect::<Vec<_>>(),
            [LabelStyle::LowerRoman, LabelStyle::UpperLetters]
        );
        // The last pages: the appendix's rule would start after the last page.
        let kept = rules_without(&rules, &[4, 5], 4);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn picked_pages_are_sorted_and_checked() {
        assert_eq!(checked(&[3, 1, 3], 4).unwrap(), [1, 3]);
        assert!(checked(&[], 4).is_err());
        assert!(matches!(checked(&[4], 4), Err(Error::PageOutOfRange(4))));
    }
}
