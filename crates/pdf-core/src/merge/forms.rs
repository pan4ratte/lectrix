//! Form fields when pages move between documents.
//!
//! Widgets travel with their pages like any other annotation, and copying a widget copies
//! its field (through `/Parent`). After a source's pages are copied, its top-level fields
//! that came along become top-level fields of the result. Widgets on pages that were not
//! picked are left out of the copied fields' `/Kids`, and fields left without widgets are
//! removed. A top-level field whose name is already taken is renamed with the source's
//! prefix: two fields with one name would be one field to every reader, sharing a value.

use std::collections::HashSet;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::copy::Copier;
use crate::error::Result;
use crate::objects::{array_items, child_array, dict_entries, text_string};

/// Nesting limit for field trees in malformed files.
const MAX_DEPTH: u32 = 32;

/// The names of the document's top-level fields.
pub(crate) fn top_level_names(doc: &PdfDocument) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    if let Some(form) = doc.catalog()?.get_dict("AcroForm")?
        && let Some(fields) = form.get_dict("Fields")?
        && fields.is_array()?
    {
        for field in array_items(&fields)? {
            if let Some(name) = field_name(&field)? {
                names.insert(name);
            }
        }
    }
    Ok(names)
}

fn field_name(field: &PdfObject) -> Result<Option<String>> {
    match field.get_dict("T")? {
        Some(t) if t.is_string()? => Ok(Some(t.as_string_lossy()?)),
        _ => Ok(None),
    }
}

/// Adds the source's copied fields to `dst`'s form. Call after the source's pages are
/// copied and the copier is finished. Returns how many fields were renamed.
pub(crate) fn merge_fields(
    dst: &mut PdfDocument,
    src: &PdfDocument,
    copier: &mut Copier,
    prefix: &str,
    taken: &mut HashSet<String>,
) -> Result<usize> {
    let Some(src_form) = src.catalog()?.get_dict("AcroForm")? else {
        return Ok(0);
    };
    let Some(src_fields) = src_form
        .get_dict("Fields")?
        .filter(|f| f.is_array().unwrap_or(false))
    else {
        return Ok(0);
    };
    let mut roots = Vec::new();
    for field in array_items(&src_fields)? {
        if !field.is_indirect()? {
            continue;
        }
        if let Some(num) = copier.mapped(field.as_indirect()?) {
            let root = dst.new_indirect(num, 0)?;
            if prune(&root, 0)? {
                roots.push(root);
            }
        }
    }
    if roots.is_empty() {
        return Ok(0);
    }

    let mut renamed = 0;
    for root in &mut roots {
        if let Some(name) = field_name(root)? {
            if taken.contains(&name) {
                let new = unique_field_name(&name, prefix, taken);
                root.dict_put("T", text_string(dst, &new)?)?;
                taken.insert(new);
                renamed += 1;
            } else {
                taken.insert(name);
            }
        }
    }

    let mut form = dst_form(dst)?;
    let mut fields = child_array(dst, &mut form, "Fields")?;
    for root in roots {
        fields.array_push(root)?;
    }

    if form.get_dict("DA")?.is_none()
        && let Some(da) = src_form.get_dict("DA")?
        && da.is_string()?
    {
        form.dict_put("DA", da)?;
    }
    if src_form
        .get_dict("NeedAppearances")?
        .is_some_and(|v| v.as_bool().unwrap_or(false))
    {
        form.dict_put("NeedAppearances", PdfObject::new_bool(true))?;
    }
    if let Some(dr) = src_form.get_dict("DR")?
        && let Some(copied) = copier.copy(dst, &dr)?
    {
        copier.finish(dst)?;
        merge_resources(&mut form, &copied)?;
    }
    Ok(renamed)
}

/// Removes widgets and subfields that were not copied (`null` kids) and fields left with
/// no widgets. Returns false if `field` itself is left empty.
fn prune(field: &PdfObject, depth: u32) -> Result<bool> {
    if depth > MAX_DEPTH || !field.is_dict()? {
        return Ok(false);
    }
    let is_widget = field
        .get_dict("Subtype")?
        .is_some_and(|s| s.as_name().ok().as_deref() == Some(b"Widget"));
    let Some(mut kids) = field
        .get_dict("Kids")?
        .filter(|k| k.is_array().unwrap_or(false))
    else {
        return Ok(true);
    };
    let len = i32::try_from(kids.len()?).unwrap_or(i32::MAX);
    for i in (0..len).rev() {
        let keep = match kids.get_array(i)? {
            Some(kid) if kid.is_dict()? => prune(&kid, depth + 1)?,
            _ => false,
        };
        if !keep {
            kids.array_delete(i)?;
        }
    }
    Ok(is_widget || kids.len()? > 0)
}

fn unique_field_name(name: &str, prefix: &str, taken: &HashSet<String>) -> String {
    let mut candidate = format!("{prefix}{name}");
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{prefix}{n}_{name}");
        n += 1;
    }
    candidate
}

/// The destination's `/AcroForm` dictionary, created if missing.
fn dst_form(dst: &mut PdfDocument) -> Result<PdfObject> {
    let mut catalog = dst.catalog()?;
    if let Some(form) = catalog.get_dict("AcroForm")?
        && form.is_dict()?
    {
        return Ok(form);
    }
    let form = dst.new_dict()?;
    let form = dst.add_object(&form)?;
    catalog.dict_put("AcroForm", form.try_clone()?)?;
    Ok(form)
}

/// Adds the copied `/DR` entries the form's `/DR` lacks (by category and name; the
/// destination's own entries win).
fn merge_resources(form: &mut PdfObject, copied: &PdfObject) -> Result<()> {
    let Some(mut dr) = form
        .get_dict("DR")?
        .filter(|d| d.is_dict().unwrap_or(false))
    else {
        form.dict_put("DR", copied.try_clone()?)?;
        return Ok(());
    };
    for (category, values) in dict_entries(copied)? {
        match dr.get_dict(category.try_clone()?)? {
            Some(mut existing) if existing.is_dict()? && values.is_dict()? => {
                for (name, value) in dict_entries(&values)? {
                    if existing.get_dict(name.try_clone()?)?.is_none() {
                        existing.dict_put(name, value)?;
                    }
                }
            }
            Some(_) => {}
            None => dr.dict_put(category, values)?,
        }
    }
    Ok(())
}
