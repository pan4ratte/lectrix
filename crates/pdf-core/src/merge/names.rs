//! Named destinations when pages move between documents.
//!
//! Every source keeps its names. A name that is already taken in the result (by an earlier
//! source, or by the document pages are inserted into) is renamed with the source's
//! prefix, and the links, bookmarks and actions copied from that source are rewritten to
//! use the new name. Names whose page was not picked are left out.
//!
//! The result's names live in the `/Dests` name tree (PDF 1.2), whose keys are strings.
//! References written as name objects are a PDF 1.1 form that readers resolve through the
//! catalog's `/Dests` dictionary only, so copied references become strings too.

use std::collections::{HashMap, HashSet};

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::Result;
use crate::objects::{array_items, dict_entries};
use crate::outline::names::name_bytes;

/// Nesting limit for name trees and action chains in malformed files.
const MAX_DEPTH: u32 = 32;

/// Every named destination of `doc` with its value (a destination array, or a dictionary
/// whose `/D` is one), in document order. A name defined twice keeps its first value, as
/// readers resolve it: the catalog's `/Dests` dictionary first, then the name tree.
pub(crate) fn collect(doc: &PdfDocument) -> Result<Vec<(Vec<u8>, PdfObject)>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let catalog = doc.catalog()?;
    if let Some(dests) = catalog.get_dict("Dests")?
        && dests.is_dict()?
    {
        for (key, value) in dict_entries(&dests)? {
            let name = key.as_name()?;
            if seen.insert(name.clone()) {
                out.push((name, value));
            }
        }
    }
    if let Some(names) = catalog.get_dict("Names")?
        && let Some(tree) = names.get_dict("Dests")?
    {
        let mut visited = HashSet::new();
        walk_tree(&tree, 0, &mut visited, &mut |name, value| {
            if seen.insert(name.clone()) {
                out.push((name, value));
            }
        })?;
    }
    Ok(out)
}

/// The entries of one of the name trees under the catalog's `/Names` (`Dests`,
/// `EmbeddedFiles`), in tree order; a name defined twice keeps its first value.
pub(crate) fn tree_entries(doc: &PdfDocument, tree: &str) -> Result<Vec<(Vec<u8>, PdfObject)>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Some(names) = doc.catalog()?.get_dict("Names")?
        && let Some(tree) = names.get_dict(tree)?
    {
        let mut visited = HashSet::new();
        walk_tree(&tree, 0, &mut visited, &mut |name, value| {
            if seen.insert(name.clone()) {
                out.push((name, value));
            }
        })?;
    }
    Ok(out)
}

fn walk_tree(
    node: &PdfObject,
    depth: u32,
    visited: &mut HashSet<i32>,
    f: &mut impl FnMut(Vec<u8>, PdfObject),
) -> Result<()> {
    if depth > MAX_DEPTH || !node.is_dict()? {
        return Ok(());
    }
    if node.is_indirect()? && !visited.insert(node.as_indirect()?) {
        return Ok(());
    }
    if let Some(names) = node.get_dict("Names")?
        && names.is_array()?
    {
        let len = i32::try_from(names.len()?).unwrap_or(i32::MAX);
        let mut i = 0;
        while i + 1 < len {
            if let (Some(key), Some(value)) = (names.get_array(i)?, names.get_array(i + 1)?)
                && let Some(name) = name_bytes(&key)?
            {
                f(name, value);
            }
            i += 2;
        }
    }
    if let Some(kids) = node.get_dict("Kids")?
        && kids.is_array()?
    {
        for kid in array_items(&kids)? {
            walk_tree(&kid, depth + 1, visited, f)?;
        }
    }
    Ok(())
}

/// A document's named destinations, by name. Looking a name up in a name tree is a linear
/// search, and a file can have thousands of links and bookmarks using names, so each
/// source's names are read once.
pub(crate) struct DestIndex(HashMap<Vec<u8>, PdfObject>);

impl DestIndex {
    pub fn empty() -> DestIndex {
        DestIndex(HashMap::new())
    }

    pub fn new(doc: &PdfDocument) -> Result<DestIndex> {
        let mut map = HashMap::new();
        for (name, value) in collect(doc)? {
            // A named destination is an array, or a dictionary whose /D is the array.
            let array = if value.is_dict()? {
                value.get_dict("D")?
            } else {
                Some(value)
            };
            if let Some(array) = array.filter(|a| a.is_array().unwrap_or(false)) {
                map.insert(name, array);
            }
        }
        Ok(DestIndex(map))
    }

    /// Where a destination value (an array, a name or string, or a dictionary with `/D`)
    /// leads.
    pub fn page(&self, value: &PdfObject) -> Result<PageRef> {
        if value.is_array()? {
            return dest_page(value);
        }
        if let Some(name) = name_bytes(value)? {
            return match self.0.get(&name) {
                Some(array) => dest_page(array),
                None => Ok(PageRef::Unknown),
            };
        }
        if value.is_dict()?
            && let Some(array) = value.get_dict("D")?
            && array.is_array()?
        {
            return dest_page(&array);
        }
        Ok(PageRef::Unknown)
    }
}

/// Where a destination array points: a page object, or (in some files) a page index.
pub(crate) enum PageRef {
    Object(i32),
    Index(usize),
    Unknown,
}

pub(crate) fn dest_page(dest: &PdfObject) -> Result<PageRef> {
    match dest.get_array(0)? {
        Some(p) if p.is_indirect()? => Ok(PageRef::Object(p.as_indirect()?)),
        Some(p) if p.is_int()? => Ok(usize::try_from(p.as_int()?)
            .map(PageRef::Index)
            .unwrap_or(PageRef::Unknown)),
        _ => Ok(PageRef::Unknown),
    }
}

/// The destination of a link or bookmark (its `/Dest`, or the `/D` of a GoTo action), not
/// yet resolved.
pub(crate) fn goto_dest(item: &PdfObject) -> Result<Option<PdfObject>> {
    if let Some(dest) = item.get_dict("Dest")? {
        return Ok(Some(dest));
    }
    if let Some(action) = item.get_dict("A")?
        && action.is_dict()?
        && action
            .get_dict("S")?
            .is_some_and(|s| s.as_name().ok().as_deref() == Some(b"GoTo"))
    {
        return Ok(action.get_dict("D")?);
    }
    Ok(None)
}

/// A name for `name` that is not in `taken`: `prefix` + `name`, with a number added if
/// that is taken too.
pub(crate) fn unique_name(name: &[u8], prefix: &str, taken: &HashSet<Vec<u8>>) -> Vec<u8> {
    let mut candidate: Vec<u8> = prefix.bytes().chain(name.iter().copied()).collect();
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{prefix}{n}_").into_bytes();
        candidate.extend_from_slice(name);
        n += 1;
    }
    candidate
}

/// A PDF string holding exactly `bytes`.
pub(crate) fn byte_string(doc: &PdfDocument, bytes: &[u8]) -> Result<PdfObject> {
    let mut hex = String::with_capacity(bytes.len() * 2 + 2);
    hex.push('<');
    for b in bytes {
        hex.push_str(&format!("{b:02X}"));
    }
    hex.push('>');
    Ok(doc.new_object_from_str(&hex)?)
}

/// Rewrites destination references in objects copied from one source: renamed names get
/// their new name, name objects become strings, and page indices (written by some
/// producers instead of page references) become references to the copied pages.
pub(crate) struct DestFixer<'a> {
    pub renames: &'a HashMap<Vec<u8>, Vec<u8>>,
    /// Source page index to destination page object number, for picked pages.
    pub pages: &'a HashMap<usize, i32>,
    /// Action dictionaries already rewritten (they may be shared).
    pub seen: HashSet<i32>,
}

impl DestFixer<'_> {
    /// Fixes an annotation's or bookmark's `/Dest`, `/A` and `/AA`.
    pub fn fix_item(&mut self, dst: &PdfDocument, item: &mut PdfObject) -> Result<()> {
        if let Some(mut dest) = item.get_dict("Dest")?
            && let Some(fixed) = self.fix_dest(dst, &mut dest)?
        {
            item.dict_put("Dest", fixed)?;
        }
        if let Some(action) = item.get_dict("A")? {
            self.fix_action(dst, action, 0)?;
        }
        if let Some(aa) = item.get_dict("AA")?
            && aa.is_dict()?
        {
            for (_, action) in dict_entries(&aa)? {
                self.fix_action(dst, action, 0)?;
            }
        }
        Ok(())
    }

    /// Fixes a copied named destination's value (an array, or a dictionary whose `/D` is
    /// one).
    pub fn fix_value(&self, dst: &PdfDocument, value: &mut PdfObject) -> Result<()> {
        if value.is_dict()? {
            if let Some(mut array) = value.get_dict("D")? {
                self.fix_dest(dst, &mut array)?;
            }
        } else {
            self.fix_dest(dst, value)?;
        }
        Ok(())
    }

    fn fix_action(&mut self, dst: &PdfDocument, action: PdfObject, depth: u32) -> Result<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        if action.is_array()? {
            for next in array_items(&action)? {
                self.fix_action(dst, next, depth + 1)?;
            }
            return Ok(());
        }
        if !action.is_dict()? {
            return Ok(());
        }
        if action.is_indirect()? && !self.seen.insert(action.as_indirect()?) {
            return Ok(());
        }
        let mut action = action;
        let goto = action
            .get_dict("S")?
            .is_some_and(|s| s.as_name().ok().as_deref() == Some(b"GoTo"));
        if goto
            && let Some(mut dest) = action.get_dict("D")?
            && let Some(fixed) = self.fix_dest(dst, &mut dest)?
        {
            action.dict_put("D", fixed)?;
        }
        if let Some(next) = action.get_dict("Next")? {
            self.fix_action(dst, next, depth + 1)?;
        }
        Ok(())
    }

    /// The replacement for a destination value, if it needs one. A destination array is
    /// fixed in place (`dest` is a handle to the array in its container).
    fn fix_dest(&self, dst: &PdfDocument, dest: &mut PdfObject) -> Result<Option<PdfObject>> {
        if dest.is_array()? {
            if let PageRef::Index(i) = dest_page(dest)?
                && let Some(&page) = self.pages.get(&i)
            {
                dest.array_put(0, dst.new_indirect(page, 0)?)?;
            }
            return Ok(None);
        }
        let is_name = dest.is_name()?;
        let Some(name) = name_bytes(dest)? else {
            return Ok(None);
        };
        match self.renames.get(&name) {
            Some(new) => byte_string(dst, new).map(Some),
            None if is_name => byte_string(dst, &name).map(Some),
            None => Ok(None),
        }
    }
}

/// Adds `entries` (name, value in `doc`) to the name tree `tree` under the catalog's
/// `/Names` (`Dests`, `EmbeddedFiles`), which is rewritten as one sorted leaf. Names
/// already in the tree keep their values; the caller has made the new names unique.
pub(crate) fn add_to_tree(
    doc: &mut PdfDocument,
    tree: &str,
    entries: Vec<(Vec<u8>, PdfObject)>,
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut all = tree_entries(doc, tree)?;
    all.extend(entries);
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all.dedup_by(|b, a| a.0 == b.0);

    let mut array = doc.new_array_with_capacity(i32::try_from(all.len() * 2).unwrap_or(0))?;
    for (name, value) in all {
        array.array_push(byte_string(doc, &name)?)?;
        array.array_push(value)?;
    }
    // A single-node tree is its own root, which has no /Limits (PDF 32000-1, 7.9.6).
    let mut leaf = doc.new_dict()?;
    leaf.dict_put("Names", array)?;
    let leaf = doc.add_object(&leaf)?;

    let mut catalog = doc.catalog()?;
    match catalog.get_dict("Names")? {
        Some(mut names) if names.is_dict()? => names.dict_put(tree, leaf)?,
        _ => {
            let mut names = doc.new_dict()?;
            names.dict_put(tree, leaf)?;
            let names = doc.add_object(&names)?;
            catalog.dict_put("Names", names)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names_take_the_prefix_then_a_number() {
        let mut taken = HashSet::new();
        taken.insert(b"intro".to_vec());
        assert_eq!(
            unique_name(b"intro", "src2_", &taken),
            b"src2_intro".to_vec()
        );
        taken.insert(b"src2_intro".to_vec());
        assert_eq!(
            unique_name(b"intro", "src2_", &taken),
            b"src2_2_intro".to_vec()
        );
    }

    #[test]
    fn byte_strings_keep_every_byte() {
        let doc = PdfDocument::new();
        let s = byte_string(&doc, &[0, b'a', 0xFF]).unwrap();
        assert_eq!(s.as_bytes().unwrap(), vec![0, b'a', 0xFF]);
    }
}
