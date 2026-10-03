//! Font lookup policy (Phase 0 review, decision 1).
//!
//! With the `mupdf` crate's `system-fonts` feature, MuPDF asks the system font hook for
//! every non-embedded font, base-14 names included, and the first query enumerates the
//! Windows font collection (about 1.6 s once per process). Folio therefore:
//!
//! - sends the 14 standard font names, and the aliases of them that no installed font can
//!   match (such as "TimesNewRoman,Bold"), straight to MuPDF's built-in, metric-compatible
//!   fonts, so documents that use only those never wait for the system font lookup; for
//!   the aliases, MuPDF ends up with the same built-in font anyway, so pages render
//!   identically (checked on real-world files); and
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

/// Aliases of the base-14 fonts that are neither a PostScript name nor a family name of an
/// installed font, so the system lookup can never find them (it then costs a full scan of
/// the installed fonts, about a second per name on Windows) and MuPDF ends up with its
/// built-in font anyway (`pdf_load_builtin_font`). Taken from MuPDF's `base_font_names`
/// (`source/pdf/pdf-font.c`), leaving out the real names (`ArialMT`, `Arial`,
/// `TimesNewRomanPSMT`, `CourierNewPS-BoldMT`, `SymbolMT`, ...): those can resolve to the
/// installed font and keep doing so.
const UNRESOLVABLE_ALIASES: [(&str, &str); 31] = [
    ("CourierNew", "Courier"),
    ("CourierNew,Bold", "Courier-Bold"),
    ("Courier,Bold", "Courier-Bold"),
    ("CourierNew-Bold", "Courier-Bold"),
    ("CourierNew,Italic", "Courier-Oblique"),
    ("Courier,Italic", "Courier-Oblique"),
    ("CourierNew-Italic", "Courier-Oblique"),
    ("CourierNew,BoldItalic", "Courier-BoldOblique"),
    ("Courier,BoldItalic", "Courier-BoldOblique"),
    ("CourierNew-BoldItalic", "Courier-BoldOblique"),
    ("Arial,Bold", "Helvetica-Bold"),
    ("Helvetica,Bold", "Helvetica-Bold"),
    ("Arial,Italic", "Helvetica-Oblique"),
    ("Helvetica,Italic", "Helvetica-Oblique"),
    ("Helvetica-Italic", "Helvetica-Oblique"),
    ("Arial,BoldItalic", "Helvetica-BoldOblique"),
    ("Helvetica,BoldItalic", "Helvetica-BoldOblique"),
    ("Helvetica-BoldItalic", "Helvetica-BoldOblique"),
    ("TimesNewRoman", "Times-Roman"),
    ("TimesNewRomanPS", "Times-Roman"),
    ("TimesNewRoman,Bold", "Times-Bold"),
    ("TimesNewRomanPS-Bold", "Times-Bold"),
    ("TimesNewRoman-Bold", "Times-Bold"),
    ("TimesNewRoman,Italic", "Times-Italic"),
    ("TimesNewRomanPS-Italic", "Times-Italic"),
    ("TimesNewRoman-Italic", "Times-Italic"),
    ("TimesNewRoman,BoldItalic", "Times-BoldItalic"),
    ("TimesNewRomanPS-BoldItalic", "Times-BoldItalic"),
    ("TimesNewRoman-BoldItalic", "Times-BoldItalic"),
    ("Symbol,Italic", "Symbol"),
    ("Symbol,Bold", "Symbol"),
];

fn strip_subset_prefix(name: &str) -> &str {
    match name.split_once('+') {
        Some((prefix, rest))
            if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            rest
        }
        _ => name,
    }
}

/// The base-14 name for a font name as it appears in a PDF, if it is one. Subset prefixes
/// ("ABCDEF+Helvetica") are ignored.
pub fn base14_name(name: &str) -> Option<&'static str> {
    let name = strip_subset_prefix(name);
    BASE14.iter().copied().find(|b| *b == name)
}

/// The built-in font MuPDF would end up with for a font name the system can never resolve
/// (see [`UNRESOLVABLE_ALIASES`]): the base-14 names themselves and their unresolvable
/// aliases.
pub fn builtin_for(name: &str) -> Option<&'static str> {
    let bare = strip_subset_prefix(name);
    base14_name(bare).or_else(|| {
        UNRESOLVABLE_ALIASES
            .iter()
            .find(|(alias, _)| *alias == bare)
            .map(|(_, base)| *base)
    })
}

struct Base14First;

impl FontLoader for Base14First {
    fn load_font(&self, name: &str, _hints: FontHints) -> Option<Font> {
        // `Font::new` loads the built-in font data for an exact base-14 name.
        Font::new(builtin_for(name)?).ok()
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
    fn unresolvable_aliases_map_to_builtins_but_real_names_do_not() {
        assert_eq!(builtin_for("TimesNewRoman,Bold"), Some("Times-Bold"));
        assert_eq!(builtin_for("ABCDEF+CourierNew"), Some("Courier"));
        assert_eq!(
            builtin_for("Arial,BoldItalic"),
            Some("Helvetica-BoldOblique")
        );
        assert_eq!(builtin_for("Helvetica"), Some("Helvetica"));
        // Real names of installed fonts keep going to the system lookup.
        for real in [
            "Arial",
            "ArialMT",
            "Arial-BoldMT",
            "TimesNewRomanPSMT",
            "SymbolMT",
            "Times New Roman",
        ] {
            assert_eq!(builtin_for(real), None, "{real}");
        }
        // Every target is a base-14 name.
        for (_, base) in UNRESOLVABLE_ALIASES {
            assert!(BASE14.contains(&base), "{base}");
        }
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
