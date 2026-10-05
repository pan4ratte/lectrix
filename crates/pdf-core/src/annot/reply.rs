//! Replies to annotations (section 6.5, ADR 0012): a note (`/Text`) linked to the
//! annotation it answers through `/IRT`, which Acrobat and Foxit show in that
//! annotation's thread.
//!
//! A reply is never drawn on the page. It gets an empty normal appearance, so MuPDF and
//! PDFium draw nothing and pdf.js draws no icon of its own, and a `/Rect` of zero area at
//! its parent's top-left corner, which pdf.js skips when it builds its annotation layer:
//! a reply over its parent would otherwise take the parent's hover and pop-up in
//! Firefox. It has no `/Popup`; readers show it in the parent's thread.

use mupdf::Buffer;
use mupdf::pdf::{PdfAnnotation, PdfAnnotationType, PdfDocument, PdfObject};

use super::{AnnotationEdit, AnnotationRef, MARKUP, NOTE_ICON, Rgb, meta, read, write};
use crate::error::{Error, Result};
use crate::geometry::Rect;
use crate::objects;

/// True for a reply that is a note: `/Subtype /Text` with `/IRT`, and not a member of a
/// group (`/RT /Group`, which stands for its parent instead of answering it). Lectrix's
/// replies and other apps' are both; none is drawn on the page by Lectrix's rules.
pub(crate) fn is_reply(obj: &PdfObject) -> Result<bool> {
    if super::subtype(obj)? != "Text" {
        return Ok(false);
    }
    if !obj
        .get_dict("IRT")?
        .is_some_and(|r| r.is_indirect().unwrap_or(false))
    {
        return Ok(false);
    }
    Ok(match obj.get_dict("RT")? {
        Some(rt) if rt.is_name()? => rt.as_name()? != b"Group",
        _ => true,
    })
}

/// True if annotation `id` on `page` is a reply (for undo step names).
pub fn is_reply_at(doc: &PdfDocument, page: usize, id: u32) -> bool {
    super::load_page(doc, page)
        .and_then(|p| super::find(&p, id))
        .and_then(|a| is_reply(&a.object()))
        .unwrap_or(false)
}

/// Writes `/AP /N` as an empty form: nothing is drawn, in any reader that uses it.
pub(crate) fn put_empty_appearance(doc: &mut PdfDocument, obj: &mut PdfObject) -> Result<()> {
    let mut form = doc.new_dict()?;
    form.dict_put("Type", PdfObject::new_name("XObject")?)?;
    form.dict_put("Subtype", PdfObject::new_name("Form")?)?;
    form.dict_put("BBox", objects::real_array(doc, &[0.0, 0.0, 0.0, 0.0])?)?;
    let stream = doc.add_stream(&Buffer::from_copied_bytes(b"")?, Some(&form), false)?;
    let mut ap = doc.new_dict()?;
    ap.dict_put("N", stream)?;
    obj.dict_put("AP", ap)?;
    Ok(())
}

/// Adds a reply by `author` saying `text` to annotation `parent` on `page`, with the
/// write profile's metadata (rule 6). The caller wraps this in a journal operation.
pub fn add_reply(
    doc: &mut PdfDocument,
    page: usize,
    parent: u32,
    text: &str,
    author: &str,
) -> Result<AnnotationRef> {
    if text.trim().is_empty() {
        return Err(Error::InvalidArgument("a reply needs some text".into()));
    }
    let mut pdf_page = super::load_page(doc, page)?;
    let parent_obj = super::find(&pdf_page, parent)?.object();
    let subtype = super::subtype(&parent_obj)?;
    if !MARKUP.contains(&subtype.as_str()) {
        return Err(Error::InvalidArgument(format!(
            "a {subtype} annotation can't be replied to"
        )));
    }
    let corner = match parent_obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r)?,
        None => None,
    }
    .map_or((0.0, 0.0), |r| (r.x0, r.y1));
    // The parent's colour, as Acrobat gives its replies; a note's yellow without one.
    let color = match parent_obj.get_dict("C")? {
        Some(c) => objects::numbers(&c)?.and_then(|v| Rgb::from_components(&v)),
        None => None,
    }
    .unwrap_or(Rgb::YELLOW);

    let mut annot = pdf_page.create_annotation(PdfAnnotationType::Text)?;
    // A new annotation gets MuPDF's appearance at its first update: let that happen now,
    // so the empty appearance written below is the one that stays.
    annot.update()?;
    let mut obj = annot.object();
    obj.dict_put(
        "Rect",
        objects::rect_array(doc, Rect::new(corner.0, corner.1, corner.0, corner.1))?,
    )?;
    obj.dict_put("IRT", parent_obj)?;
    obj.dict_put("Name", PdfObject::new_name(NOTE_ICON)?)?;
    obj.dict_put("Open", PdfObject::new_bool(false))?;
    obj.dict_put(
        "C",
        objects::real_array(doc, &[color.r, color.g, color.b].map(f64::from))?,
    )?;
    obj.dict_put("Contents", objects::text_string(doc, text)?)?;
    obj.dict_delete("Popup")?;
    write::write_metadata(doc, &mut annot, author)?;
    put_empty_appearance(doc, &mut annot.object())?;
    Ok(AnnotationRef {
        page,
        xref: annot.xref()?,
        name: super::read_name(&annot.object())?.unwrap_or_default(),
    })
}

/// Changes a reply's text or author (nothing else can change). It keeps no appearance
/// but an empty one: MuPDF's would draw a note icon on the page.
pub(crate) fn edit_reply(
    doc: &mut PdfDocument,
    annot: &mut PdfAnnotation,
    edit: &AnnotationEdit,
) -> Result<()> {
    let only_text = AnnotationEdit {
        contents: None,
        author: None,
        ..edit.clone()
    };
    if !only_text.is_empty() {
        return Err(Error::InvalidArgument(
            "only the text and author of a reply can be changed".into(),
        ));
    }
    let mut obj = annot.object();
    if let Some(text) = &edit.contents {
        if text.trim().is_empty() {
            return Err(Error::InvalidArgument("a reply needs some text".into()));
        }
        obj.dict_put("Contents", objects::text_string(doc, text)?)?;
        // Rich text would be shown instead of /Contents.
        obj.dict_delete("RC")?;
    }
    if let Some(author) = &edit.author {
        obj.dict_put("T", objects::text_string(doc, author)?)?;
    }
    obj.dict_put("M", PdfObject::new_string(&meta::pdf_date_now())?)?;
    if !read::has_appearance(&obj)? {
        put_empty_appearance(doc, &mut obj)?;
    }
    Ok(())
}
