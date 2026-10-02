//! Helpers for building PDF objects the way Folio's write profile requires.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::Result;
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
