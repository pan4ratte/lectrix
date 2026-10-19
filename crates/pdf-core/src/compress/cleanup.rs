//! What Compress removes when the user chooses to (ADR 0027): page thumbnails, other apps'
//! private data, attached files, JavaScript and tags. What becomes unreferenced goes when
//! the file is written (garbage collection).

use mupdf::pdf::{PdfDocument, PdfObject};

use super::{Removals, RemovedReport};
use crate::error::Result;

pub(super) fn remove(doc: &mut PdfDocument, what: &Removals) -> Result<RemovedReport> {
    let mut report = RemovedReport::default();
    let mut catalog = doc.catalog()?;

    if what.thumbnails {
        for page in 0..doc.page_count()? {
            let mut page = doc.find_page(page)?;
            if page.get_dict("Thumb")?.is_some() {
                page.dict_delete("Thumb")?;
                report.thumbnails += 1;
            }
        }
    }
    if what.attachments {
        if let Some(mut names) = catalog.get_dict("Names")?
            && let Some(files) = names.get_dict("EmbeddedFiles")?
        {
            report.attachments = name_tree_len(&files, 0)?;
            names.dict_delete("EmbeddedFiles")?;
        }
        // Associated files (PDF/A-3, PDF 2.0) and a portfolio's layout go with them.
        for key in ["AF", "Collection"] {
            catalog.dict_delete(key)?;
        }
    }
    if what.javascript
        && let Some(mut names) = catalog.get_dict("Names")?
        && let Some(scripts) = names.get_dict("JavaScript")?
    {
        report.javascript += name_tree_len(&scripts, 0)?;
        names.dict_delete("JavaScript")?;
    }
    if what.tags && catalog.get_dict("StructTreeRoot")?.is_some() {
        catalog.dict_delete("StructTreeRoot")?;
        catalog.dict_delete("MarkInfo")?;
        report.tags = true;
    }

    if what.private_data || what.javascript {
        let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
        for num in 1..len {
            let mut obj = doc.new_indirect(num, 0)?;
            if !obj.is_dict()? {
                continue;
            }
            if what.private_data && obj.get_dict("PieceInfo")?.is_some() {
                obj.dict_delete("PieceInfo")?;
                report.private_data += 1;
            }
            if what.javascript {
                report.javascript += remove_scripts(&mut obj)?;
            }
        }
    }
    Ok(report)
}

/// The number of entries in a name tree.
pub(super) fn name_tree_len(node: &PdfObject, depth: u8) -> Result<usize> {
    if depth > 32 {
        return Ok(0);
    }
    let mut count = 0;
    if let Some(names) = node.get_dict("Names")?
        && names.is_array()?
    {
        count += names.len()? / 2;
    }
    if let Some(kids) = node.get_dict("Kids")?
        && kids.is_array()?
    {
        for i in 0..kids.len()? {
            if let Some(kid) = kids.get_array(i as i32)? {
                count += name_tree_len(&kid, depth + 1)?;
            }
        }
    }
    Ok(count)
}

pub(super) fn is_script(action: &PdfObject) -> Result<bool> {
    Ok(action.is_dict()?
        && matches!(action.get_dict("S")?, Some(s) if s.is_name()? && s.as_name()? == b"JavaScript"))
}

/// Removes the JavaScript actions `obj` runs (`/OpenAction`, `/A`, and those of `/AA`), and
/// says how many.
fn remove_scripts(obj: &mut PdfObject) -> Result<usize> {
    let mut removed = 0;
    for key in ["OpenAction", "A"] {
        if let Some(action) = obj.get_dict(key)?
            && is_script(&action)?
        {
            obj.dict_delete(key)?;
            removed += 1;
        }
    }
    if let Some(mut actions) = obj.get_dict("AA")?
        && actions.is_dict()?
    {
        let mut scripts = Vec::new();
        for i in 0..actions.dict_len()? {
            if let (Some(key), Some(action)) = (
                actions.get_dict_key(i as i32)?,
                actions.get_dict_val(i as i32)?,
            ) && is_script(&action)?
            {
                scripts.push(String::from_utf8_lossy(&key.as_name()?).into_owned());
            }
        }
        for key in &scripts {
            actions.dict_delete(key.as_str())?;
        }
        removed += scripts.len();
        if actions.dict_len()? == 0 {
            obj.dict_delete("AA")?;
        }
    }
    Ok(removed)
}
