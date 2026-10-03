//! Document-level facts the UI needs before letting the user edit (AGENTS.md section 5.4).

use mupdf::pdf::{PdfDocument, PdfObject, Permission};

use crate::error::Result;

/// What the document allows and how it must be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocumentFlags {
    /// The file is encrypted (it keeps its encryption on save).
    pub encrypted: bool,
    /// At least one signature field holds a signature: warn before the first edit, and
    /// always save incrementally.
    pub signed: bool,
    /// MuPDF repaired a damaged cross-reference table on open, so the next save must be
    /// a full save.
    pub repaired: bool,
    /// Pages may be rotated, inserted or deleted (permission bit 4 or 11).
    pub can_assemble: bool,
    /// Annotations may be added or changed (permission bit 6).
    pub can_annotate: bool,
    /// Text may be copied (permission bit 5).
    pub can_copy: bool,
}

pub fn read_flags(doc: &PdfDocument) -> Result<DocumentFlags> {
    let encrypted = doc.trailer()?.get_dict("Encrypt")?.is_some();
    let permissions = if encrypted {
        doc.permissions()
    } else {
        Permission::all()
    };
    Ok(DocumentFlags {
        encrypted,
        signed: is_signed(doc)?,
        repaired: crate::ffi::was_repaired(doc)?,
        can_assemble: permissions.intersects(Permission::MODIFY | Permission::ASSEMBLE),
        can_annotate: permissions.contains(Permission::ANNOTATE),
        can_copy: permissions.contains(Permission::COPY),
    })
}

/// True if any form field of type `/Sig` (possibly inherited) has a value.
pub fn is_signed(doc: &PdfDocument) -> Result<bool> {
    let Some(form) = doc.catalog()?.get_dict("AcroForm")? else {
        return Ok(false);
    };
    let Some(fields) = form.get_dict("Fields")? else {
        return Ok(false);
    };
    any_signed(&fields, false, 0)
}

fn any_signed(fields: &PdfObject, parent_is_sig: bool, depth: u32) -> Result<bool> {
    // Guards against reference cycles in malformed files.
    if depth > 32 || !fields.is_array()? {
        return Ok(false);
    }
    for i in 0..i32::try_from(fields.len()?).unwrap_or(i32::MAX) {
        let Some(field) = fields.get_array(i)? else {
            continue;
        };
        if !field.is_dict()? {
            continue;
        }
        let is_sig = match field.get_dict("FT")? {
            Some(ft) if ft.is_name()? => ft.as_name()? == b"Sig",
            _ => parent_is_sig,
        };
        if is_sig
            && field
                .get_dict("V")?
                .is_some_and(|v| v.is_dict().unwrap_or(false))
        {
            return Ok(true);
        }
        if let Some(kids) = field.get_dict("Kids")?
            && any_signed(&kids, is_sig, depth + 1)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    fn add_field(doc: &mut PdfDocument, ft_on_parent: bool, with_value: bool) {
        let mut kid = doc.new_dict().unwrap();
        kid.dict_put("T", PdfObject::new_string("kid").unwrap())
            .unwrap();
        if !ft_on_parent {
            kid.dict_put("FT", PdfObject::new_name("Sig").unwrap())
                .unwrap();
        }
        if with_value {
            let mut v = doc.new_dict().unwrap();
            v.dict_put("Type", PdfObject::new_name("Sig").unwrap())
                .unwrap();
            kid.dict_put("V", v).unwrap();
        }
        let mut kids = doc.new_array().unwrap();
        kids.array_push(doc.add_object(&kid).unwrap()).unwrap();
        let mut parent = doc.new_dict().unwrap();
        if ft_on_parent {
            parent
                .dict_put("FT", PdfObject::new_name("Sig").unwrap())
                .unwrap();
        }
        parent.dict_put("Kids", kids).unwrap();
        let mut fields = doc.new_array().unwrap();
        fields.array_push(doc.add_object(&parent).unwrap()).unwrap();
        let mut form = doc.new_dict().unwrap();
        form.dict_put("Fields", fields).unwrap();
        doc.catalog().unwrap().dict_put("AcroForm", form).unwrap();
    }

    #[test]
    fn plain_document_is_unsigned_and_editable() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let flags = read_flags(&doc).unwrap();
        assert_eq!(
            flags,
            DocumentFlags {
                encrypted: false,
                signed: false,
                repaired: false,
                can_assemble: true,
                can_annotate: true,
                can_copy: true,
            }
        );
    }

    #[test]
    fn detects_signatures_including_inherited_field_type() {
        for ft_on_parent in [false, true] {
            let mut doc = sample_document(&SampleSpec::default()).unwrap();
            add_field(&mut doc, ft_on_parent, true);
            assert!(is_signed(&doc).unwrap(), "ft_on_parent: {ft_on_parent}");
        }
    }

    #[test]
    fn empty_signature_field_is_not_signed() {
        let mut doc = sample_document(&SampleSpec::default()).unwrap();
        add_field(&mut doc, false, false);
        assert!(!is_signed(&doc).unwrap());
    }
}
