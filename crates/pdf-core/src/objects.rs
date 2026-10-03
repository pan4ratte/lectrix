//! Helpers for building PDF objects the way Folio's write profile requires.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::Rect;

/// A PDF text string: plain ASCII as a literal string, anything else as UTF-16BE with a
/// byte-order mark (AGENTS.md section 6.2). MuPDF's own `pdf_new_text_string` would choose
/// PDFDocEncoding for characters such as "é", which some readers decode incorrectly.
pub fn text_string(doc: &PdfDocument, s: &str) -> Result<PdfObject> {
    if s.is_ascii() && !s.contains('\0') {
        return Ok(PdfObject::new_string(s)?);
    }
    Ok(doc.new_object_from_str(&utf16be_hex_literal(s))?)
}

/// The elements of an array, without nulls. (MuPDF's null object is a null pointer, which
/// the crate's iterators report as an error.) Empty if `array` is not an array.
pub fn array_items(array: &PdfObject) -> Result<Vec<PdfObject>> {
    let mut out = Vec::new();
    if !array.is_array()? {
        return Ok(out);
    }
    for i in 0..i32::try_from(array.len()?).unwrap_or(i32::MAX) {
        if let Some(item) = array.get_array(i)? {
            out.push(item);
        }
    }
    Ok(out)
}

/// The entries of a dictionary, without null values (which mean the same as a missing
/// key). Empty if `dict` is not a dictionary.
pub fn dict_entries(dict: &PdfObject) -> Result<Vec<(PdfObject, PdfObject)>> {
    let mut out = Vec::new();
    if !dict.is_dict()? {
        return Ok(out);
    }
    for i in 0..i32::try_from(dict.dict_len()?).unwrap_or(i32::MAX) {
        if let (Some(key), Some(value)) = (dict.get_dict_key(i)?, dict.get_dict_val(i)?) {
            out.push((key, value));
        }
    }
    Ok(out)
}

/// `dict[key]` as an array that can be changed in place, created empty if it is missing or
/// not an array. (The crate's `try_clone` makes a deep copy, so an object put into a
/// container must be read back from it to be changed there.)
pub fn child_array(doc: &PdfDocument, dict: &mut PdfObject, key: &str) -> Result<PdfObject> {
    if let Some(a) = dict.get_dict(key)?
        && a.is_array()?
    {
        return Ok(a);
    }
    dict.dict_put(key, doc.new_array()?)?;
    dict.get_dict(key)?
        .ok_or_else(|| Error::InvalidArgument(format!("could not create /{key}")))
}

/// `dict[key]` as a dictionary that can be changed in place, created empty if it is missing
/// or not a dictionary (see [`child_array`]).
pub fn child_dict(doc: &PdfDocument, dict: &mut PdfObject, key: &str) -> Result<PdfObject> {
    if let Some(d) = dict.get_dict(key)?
        && d.is_dict()?
    {
        return Ok(d);
    }
    dict.dict_put(key, doc.new_dict()?)?;
    dict.get_dict(key)?
        .ok_or_else(|| Error::InvalidArgument(format!("could not create /{key}")))
}

/// Reads a number (integer or real).
pub fn number(obj: &PdfObject) -> Result<Option<f64>> {
    if obj.is_number()? {
        Ok(Some(f64::from(obj.as_float()?)))
    } else {
        Ok(None)
    }
}

/// Reads a 4-number array as a normalized rectangle.
pub fn rect(obj: &PdfObject) -> Result<Option<Rect>> {
    let values = numbers(obj)?;
    match values.as_deref() {
        Some([x0, y0, x1, y1]) => Ok(Some(Rect::new(*x0, *y0, *x1, *y1).normalized())),
        _ => Ok(None),
    }
}

/// Reads an array of numbers. `None` if `obj` is not an array or holds a non-number.
pub fn numbers(obj: &PdfObject) -> Result<Option<Vec<f64>>> {
    if !obj.is_array()? {
        return Ok(None);
    }
    let len = obj.len()?;
    let mut out = Vec::with_capacity(len);
    for i in 0..i32::try_from(len).unwrap_or(i32::MAX) {
        match obj.get_array(i)? {
            Some(item) => match number(&item)? {
                Some(n) => out.push(n),
                None => return Ok(None),
            },
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// A new array of reals.
pub fn real_array(doc: &PdfDocument, values: &[f64]) -> Result<PdfObject> {
    let mut array = doc.new_array_with_capacity(i32::try_from(values.len()).unwrap_or(0))?;
    for v in values {
        // PDF reals are written with single precision by MuPDF; f32 is exact enough for
        // coordinates (sub-micrometre at page scale).
        array.array_push(PdfObject::new_real(*v as f32)?)?;
    }
    Ok(array)
}

pub fn rect_array(doc: &PdfDocument, r: Rect) -> Result<PdfObject> {
    real_array(doc, &[r.x0, r.y0, r.x1, r.y1])
}

/// `<FEFF....>`: a hex string holding the UTF-16BE encoding of `s` with a BOM.
pub fn utf16be_hex_literal(s: &str) -> String {
    let mut out = String::with_capacity(6 + s.len() * 4);
    out.push_str("<FEFF");
    for unit in s.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_literal_has_bom_and_surrogates() {
        assert_eq!(utf16be_hex_literal("é"), "<FEFF00E9>");
        assert_eq!(utf16be_hex_literal("𝄞"), "<FEFFD834DD1E>");
        assert_eq!(utf16be_hex_literal(""), "<FEFF>");
    }
}
