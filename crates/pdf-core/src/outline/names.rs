//! Named destinations: the catalog's `/Dests` dictionary (PDF 1.1) and the `/Dests` name
//! tree under `/Names` (PDF 1.2 and later), looked up the way MuPDF's `pdf_lookup_dest`
//! does it.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::Result;

/// Nesting limit for name trees; deeper trees in malformed files are not searched.
const MAX_DEPTH: u32 = 32;

/// The bytes of a destination name, whether it is written as a name or a string.
pub fn name_bytes(obj: &PdfObject) -> Result<Option<Vec<u8>>> {
    if obj.is_name()? {
        Ok(Some(obj.as_name()?))
    } else if obj.is_string()? {
        Ok(Some(obj.as_bytes()?))
    } else {
        Ok(None)
    }
}

/// The explicit destination (an array) that `name` stands for, if the document defines it.
pub fn lookup_dest(doc: &PdfDocument, name: &[u8]) -> Result<Option<PdfObject>> {
    let catalog = doc.catalog()?;
    let mut found = None;
    if let Ok(key) = std::str::from_utf8(name)
        && let Some(dests) = catalog.get_dict("Dests")?
        && dests.is_dict()?
    {
        found = dests.get_dict(key)?;
    }
    if found.is_none()
        && let Some(names) = catalog.get_dict("Names")?
        && let Some(tree) = names.get_dict("Dests")?
    {
        found = match tree_lookup(&tree, name, 0, true)? {
            Some(v) => Some(v),
            // Limits in some files are wrong; search the whole tree before giving up.
            None => tree_lookup(&tree, name, 0, false)?,
        };
    }
    // A named destination is an array, or a dictionary whose /D is the array.
    match found {
        Some(v) if v.is_array()? => Ok(Some(v)),
        Some(v) if v.is_dict()? => Ok(v.get_dict("D")?.filter(|d| d.is_array().unwrap_or(false))),
        _ => Ok(None),
    }
}

fn tree_lookup(
    node: &PdfObject,
    key: &[u8],
    depth: u32,
    use_limits: bool,
) -> Result<Option<PdfObject>> {
    if depth > MAX_DEPTH || !node.is_dict()? {
        return Ok(None);
    }
    if let Some(kids) = node.get_dict("Kids")?
        && kids.is_array()?
    {
        for i in 0..i32::try_from(kids.len()?).unwrap_or(i32::MAX) {
            let Some(kid) = kids.get_array(i)? else {
                continue;
            };
            if use_limits && !within_limits(&kid, key)? {
                continue;
            }
            if let Some(found) = tree_lookup(&kid, key, depth + 1, use_limits)? {
                return Ok(Some(found));
            }
        }
    }
    if let Some(names) = node.get_dict("Names")?
        && names.is_array()?
    {
        let len = i32::try_from(names.len()?).unwrap_or(i32::MAX);
        // Keys in a leaf are sorted (PDF 32000-1, 7.9.6), so search by halves first; a
        // leaf can hold thousands of names (combined files write one leaf). Unsorted
        // leaves in damaged files are still searched one by one below.
        let (mut lo, mut hi) = (0, len / 2);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let Some(k) = names
                .get_array(2 * mid)?
                .map(|k| name_bytes(&k))
                .transpose()?
                .flatten()
            else {
                break;
            };
            match k.as_slice().cmp(key) {
                std::cmp::Ordering::Equal => return Ok(names.get_array(2 * mid + 1)?),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        let mut i = 0;
        while i + 1 < len {
            if let Some(k) = names.get_array(i)?
                && name_bytes(&k)?.as_deref() == Some(key)
            {
                return Ok(names.get_array(i + 1)?);
            }
            i += 2;
        }
    }
    Ok(None)
}

/// False only when the node's `/Limits` say `key` is outside it.
fn within_limits(node: &PdfObject, key: &[u8]) -> Result<bool> {
    let Some(limits) = node.get_dict("Limits")? else {
        return Ok(true);
    };
    let (Some(lo), Some(hi)) = (limits.get_array(0)?, limits.get_array(1)?) else {
        return Ok(true);
    };
    match (name_bytes(&lo)?, name_bytes(&hi)?) {
        (Some(lo), Some(hi)) => Ok(lo.as_slice() <= key && key <= hi.as_slice()),
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    #[test]
    fn finds_names_in_dests_dict_and_name_tree() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let page = doc.find_page(1).unwrap();
        let num = page.as_indirect().unwrap();
        // /Dests dictionary (PDF 1.1), and a two-level name tree with one wrong /Limits.
        let old = doc
            .new_object_from_str(&format!("<< /Old [{num} 0 R /Fit] >>"))
            .unwrap();
        let tree = doc
            .new_object_from_str(&format!(
                "<< /Kids [ << /Limits [(a) (b)] /Names [(a) [{num} 0 R /Fit] (b) << /D [{num} 0 R /XYZ 0 0 null] >>] >> \
                 << /Limits [(x) (y)] /Names [(m) [{num} 0 R /FitH 10]] >> ] >>"
            ))
            .unwrap();
        let mut names = doc.new_dict().unwrap();
        names.dict_put("Dests", tree).unwrap();
        let mut catalog = doc.catalog().unwrap();
        catalog.dict_put("Dests", old).unwrap();
        catalog.dict_put("Names", names).unwrap();

        for key in [&b"Old"[..], b"a", b"b", b"m"] {
            let dest = lookup_dest(&doc, key).unwrap();
            let dest = dest.unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(key)));
            assert_eq!(
                dest.get_array(0).unwrap().unwrap().as_indirect().unwrap(),
                num
            );
        }
        assert!(lookup_dest(&doc, b"missing").unwrap().is_none());
    }
}
