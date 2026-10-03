//! Font lookup policy (Phase 0 review, decision 1).
//!
//! With the `mupdf` crate's `system-fonts` feature, MuPDF asks the system font hook for
//! every non-embedded font, base-14 names included, and the first query enumerates the
//! Windows font collection (about 1.6 s once per process). Folio therefore:
//!
//! - sends the 14 standard font names straight to MuPDF's built-in, metric-compatible
//!   fonts, as Acrobat does, so documents that use only base-14 fonts never wait for the
//!   system font collection; and
//! - warms the system font collection on a background thread at startup, so documents
//!   that name other non-embedded fonts (for example "Arial") usually find it ready.

use std::sync::Once;
use std::thread;

use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::{Buffer, Font, FontHints, FontLoader, TextPageFlags};

use crate::error::Result;

/// The 14 standard font names (PDF 32000-1:2008, 9.6.2.2), exactly as MuPDF's
/// `fz_lookup_base14_font` accepts them.
const BASE14: [&str; 14] = [
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Symbol",
    "ZapfDingbats",
];

/// The base-14 name for a font name as it appears in a PDF, if it is one. Subset prefixes
/// ("ABCDEF+Helvetica") are ignored.
pub fn base14_name(name: &str) -> Option<&'static str> {
    let name = match name.split_once('+') {
        Some((prefix, rest))
            if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            rest
        }
        _ => name,
    };
    BASE14.iter().copied().find(|b| *b == name)
}

struct Base14First;

impl FontLoader for Base14First {
    fn load_font(&self, name: &str, _hints: FontHints) -> Option<Font> {
        // `Font::new` loads the built-in font data for an exact base-14 name.
        Font::new(base14_name(name)?).ok()
    }
}

static INSTALL: Once = Once::new();

/// Installs Folio's font policy. Call once at startup, before opening documents; later
/// calls do nothing.
pub fn install() {
    INSTALL.call_once(|| mupdf::set_font_loader(Base14First));
}

/// Loads the system font collection on a background thread, so the first document that
/// needs a non-embedded, non-base-14 font does not pay for it. Returns immediately.
pub fn warm_up_in_background() {
    let spawned = thread::Builder::new()
        .name("font-warm-up".into())
        .spawn(|| {
            // A failure here only means the first such document pays the cost instead.
            let _ = warm_up();
        });
    drop(spawned);
}

/// Renders the text of a one-page document that uses a non-embedded "Arial", which makes
/// MuPDF query the system font hook.
pub fn warm_up() -> Result<()> {
    let mut doc = PdfDocument::new();
    let mut font = doc.new_dict()?;
    font.dict_put("Type", PdfObject::new_name("Font")?)?;
    font.dict_put("Subtype", PdfObject::new_name("TrueType")?)?;
    font.dict_put("BaseFont", PdfObject::new_name("Arial")?)?;
    font.dict_put("Encoding", PdfObject::new_name("WinAnsiEncoding")?)?;
    let font = doc.add_object(&font)?;
    let mut fonts = doc.new_dict()?;
    fonts.dict_put("F1", font)?;
    let mut resources = doc.new_dict()?;
    resources.dict_put("Font", fonts)?;
    let content = doc.add_stream(
        &Buffer::from_copied_bytes(b"BT /F1 12 Tf 10 10 Td (warm) Tj ET")?,
        None,
        false,
    )?;
    let mut page = doc.new_dict()?;
    page.dict_put("Type", PdfObject::new_name("Page")?)?;
    page.dict_put(
        "MediaBox",
        crate::objects::real_array(&doc, &[0.0, 0.0, 100.0, 40.0])?,
    )?;
    page.dict_put("Resources", resources)?;
    page.dict_put("Contents", content)?;
    let page = doc.add_object(&page)?;
    doc.insert_page(0, &page)?;
    doc.load_page(0)?.to_text_page(TextPageFlags::empty())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_base14_names() {
        assert_eq!(base14_name("Helvetica"), Some("Helvetica"));
        assert_eq!(base14_name("ABCDEF+Times-Bold"), Some("Times-Bold"));
        assert_eq!(base14_name("ZapfDingbats"), Some("ZapfDingbats"));
        assert_eq!(base14_name("Arial"), None);
        assert_eq!(base14_name("Helvetica,Bold"), None);
        assert_eq!(base14_name("abcdef+Helvetica"), None);
        assert_eq!(base14_name("TimesNewRoman"), None);
    }

    #[test]
    fn base14_loader_returns_builtin_font() {
        let font = Base14First
            .load_font("Helvetica-Bold", FontHints::default())
            .unwrap();
        assert!(font.name().contains("Helvetica"), "{}", font.name());
        assert!(
            Base14First
                .load_font("Arial", FontHints::default())
                .is_none()
        );
    }

    #[test]
    fn warm_up_succeeds() {
        warm_up().unwrap();
    }
}
