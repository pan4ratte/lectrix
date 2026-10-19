//! Subsetting embedded fonts (Compress, ADR 0027): MuPDF's `pdf_subset_fonts` keeps the
//! glyphs that pages, annotations and widgets show. The fonts of the form's default
//! resources (`/AcroForm /DR`) are kept whole: a reader that fills in a field, or redraws
//! a text box, types new text with them.

use std::collections::HashMap;

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::Result;
use crate::ffi::set_new_stream;

/// Subsets `doc`'s fonts and says how many changed.
pub(super) fn subset(doc: &mut PdfDocument) -> Result<usize> {
    let kept = form_fonts(doc)?;
    let before = font_files(doc)?;
    // A font MuPDF can't subset stays as it was; an error elsewhere leaves the fonts it
    // reached subset, each whole and consistent.
    let _ = crate::ffi::compress::subset_fonts(doc);
    restore(doc, kept)?;
    let after = font_files(doc)?;
    Ok(before
        .iter()
        .filter(|(num, len)| after.get(num).is_some_and(|l| l != *len))
        .count())
}

/// Each embedded font program (`/FontFile`, `/FontFile2`, `/FontFile3`) by object number,
/// with its length.
fn font_files(doc: &PdfDocument) -> Result<HashMap<i32, usize>> {
    let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
    let mut files = HashMap::new();
    for num in 1..len {
        let obj = doc.new_indirect(num, 0)?;
        if !obj.is_dict()? || type_is(&obj, "FontDescriptor")? == Some(false) {
            continue;
        }
        for key in ["FontFile", "FontFile2", "FontFile3"] {
            if let Some(file) = obj.get_dict(key)?
                && file.is_indirect()?
                && file.is_stream()?
            {
                files.insert(file.as_indirect()?, file.read_raw_stream()?.len());
            }
        }
    }
    Ok(files)
}

/// `Some(true)` if `obj` has `/Type /<name>`, `Some(false)` if another type, `None` if none.
fn type_is(obj: &PdfObject, name: &str) -> Result<Option<bool>> {
    Ok(match obj.get_dict("Type")? {
        Some(t) if t.is_name()? => Some(t.as_name()? == name.as_bytes()),
        _ => None,
    })
}

/// An object as it was: its dictionary and, for a stream, its raw data.
struct Saved {
    num: i32,
    dict: PdfObject,
    stream: Option<Vec<u8>>,
}

/// The fonts of `/AcroForm /DR /Font`, with their descendant fonts, descriptors and
/// programs, as they are.
fn form_fonts(doc: &PdfDocument) -> Result<Vec<Saved>> {
    let mut saved = Vec::new();
    let Some(fonts) = doc
        .catalog()?
        .get_dict("AcroForm")?
        .and_then(|f| f.get_dict("DR").ok().flatten())
        .and_then(|dr| dr.get_dict("Font").ok().flatten())
    else {
        return Ok(saved);
    };
    if !fonts.is_dict()? {
        return Ok(saved);
    }
    let mut seen = Vec::new();
    for i in 0..fonts.dict_len()? {
        let Some(font) = fonts.get_dict_val(i as i32)? else {
            continue;
        };
        keep(&font, &mut saved, &mut seen)?;
        if let Some(descendants) = font.get_dict("DescendantFonts")?
            && let Some(descendant) = descendants.get_array(0)?
        {
            keep(&descendant, &mut saved, &mut seen)?;
            keep_descriptor(&descendant, &mut saved, &mut seen)?;
        }
        keep_descriptor(&font, &mut saved, &mut seen)?;
    }
    Ok(saved)
}

fn keep_descriptor(font: &PdfObject, saved: &mut Vec<Saved>, seen: &mut Vec<i32>) -> Result<()> {
    if let Some(descriptor) = font.get_dict("FontDescriptor")? {
        keep(&descriptor, saved, seen)?;
        for key in ["FontFile", "FontFile2", "FontFile3"] {
            if let Some(file) = descriptor.get_dict(key)? {
                keep(&file, saved, seen)?;
            }
        }
    }
    Ok(())
}

fn keep(obj: &PdfObject, saved: &mut Vec<Saved>, seen: &mut Vec<i32>) -> Result<()> {
    if !obj.is_indirect()? {
        return Ok(());
    }
    let num = obj.as_indirect()?;
    if seen.contains(&num) {
        return Ok(());
    }
    seen.push(num);
    let Some(resolved) = obj.resolve()? else {
        return Ok(());
    };
    saved.push(Saved {
        num,
        // A deep copy: what MuPDF changes in the document doesn't reach it.
        dict: resolved.try_clone()?,
        stream: if obj.is_stream()? {
            Some(obj.read_raw_stream()?)
        } else {
            None
        },
    });
    Ok(())
}

fn restore(doc: &mut PdfDocument, saved: Vec<Saved>) -> Result<()> {
    for s in saved {
        let mut obj = doc.new_indirect(s.num, 0)?;
        if let Some(stream) = &s.stream {
            set_new_stream(doc, &obj, stream)?;
            let length = obj.get_dict("Length")?;
            let mut dict = s.dict;
            // The raw data's length is the one just written.
            if let Some(length) = length {
                dict.dict_put("Length", length)?;
            }
            for i in 0..dict.dict_len()? {
                if let (Some(key), Some(value)) =
                    (dict.get_dict_key(i as i32)?, dict.get_dict_val(i as i32)?)
                {
                    obj.dict_put(&*String::from_utf8_lossy(&key.as_name()?), value)?;
                }
            }
        } else {
            obj.write_object(&s.dict)?;
        }
    }
    Ok(())
}
