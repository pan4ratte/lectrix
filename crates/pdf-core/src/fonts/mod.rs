//! Fonts for documents that do not embed them (ADR 0005).
//!
//! MuPDF asks Lectrix's font loader for every non-embedded font. Lectrix:
//!
//! - sends the 14 standard font names, and the aliases of them that no installed font can
//!   match (such as "TimesNewRoman,Bold"), straight to MuPDF's built-in, metric-compatible
//!   fonts; for the aliases, MuPDF ended up with the same built-in font anyway, so pages
//!   render identically (checked on real-world files); and
//! - looks every other name up in an index of the installed fonts ([`index`]), built once
//!   on a background thread at startup. Lookups that arrive while it is being built wait
//!   for it. Matching follows the `mupdf` crate's former `font-kit` lookup, so documents
//!   render with the same fonts.

pub mod index;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, Once, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf::{Buffer, CjkFontOrdering, Font, FontHints, FontLoader, TextPageFlags};

use crate::error::Result;
use index::{FontFace, FontIndex, cjk_ordering};

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

/// The index of installed fonts, and how long building it took.
static INDEX: OnceLock<(FontIndex, Duration)> = OnceLock::new();

/// The installed-font index, built on first use. Concurrent callers wait for the one
/// build. A platform error leaves the index empty: fonts then fall back to MuPDF's
/// substitutes instead of failing.
pub fn installed_fonts() -> &'static FontIndex {
    &INDEX
        .get_or_init(|| {
            let start = Instant::now();
            let faces = crate::platform::system_fonts()
                .installed_faces()
                .unwrap_or_default();
            (FontIndex::new(faces), start.elapsed())
        })
        .0
}

/// How long building the index took (`None` before it is built).
pub fn index_build_time() -> Option<Duration> {
    INDEX.get().map(|(_, d)| *d)
}

/// Font files read so far, shared with MuPDF for the rest of the process (as the crate
/// did): bounded by the distinct fonts documents ask for.
static FONT_DATA: Mutex<Option<HashMap<PathBuf, &'static [u8]>>> = Mutex::new(None);

fn font_data(face: &FontFace) -> Option<&'static [u8]> {
    // The map stays valid if a reader panicked while holding the lock.
    let mut guard = FONT_DATA.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(data) = map.get(&face.path) {
        return Some(data);
    }
    let data: &'static [u8] = Box::leak(std::fs::read(&face.path).ok()?.into_boxed_slice());
    map.insert(face.path.clone(), data);
    Some(data)
}

/// Loads an installed face for MuPDF.
fn load_face(face: &FontFace) -> Option<Font> {
    let data = font_data(face)?;
    // The face index applies to collections only.
    let index = if data.starts_with(b"ttcf") {
        i32::try_from(face.index).ok()?
    } else {
        0
    };
    Font::from_static_bytes_with_index(face.family_name(), index, data).ok()
}

/// Lectrix's font loader: built-in fonts for the standard names, installed fonts for the
/// rest.
struct LectrixFonts;

impl LectrixFonts {
    fn cjk(&self, ordering: CjkFontOrdering, serif: bool) -> Option<Font> {
        load_face(installed_fonts().cjk(ordering, serif)?)
    }
}

impl FontLoader for LectrixFonts {
    fn load_font(&self, name: &str, hints: FontHints) -> Option<Font> {
        if let Some(builtin) = builtin_for(name) {
            // `Font::new` loads the built-in font data for an exact base-14 name.
            return Font::new(builtin).ok();
        }
        let face = installed_fonts().lookup(name, hints.bold, hints.italic)?;
        let font = load_face(face)?;
        // A face without real bold or italic is refused when MuPDF needs exact metrics.
        if hints.needs_exact_metrics
            && ((hints.bold && !font.is_bold()) || (hints.italic && !font.is_italic()))
        {
            return None;
        }
        Some(font)
    }

    fn load_cjk_font(&self, name: &str, ordering: CjkFontOrdering, serif: bool) -> Option<Font> {
        // The font the document names, if installed, before a generic one for the
        // ordering (the crate's order).
        if !name.is_empty()
            && let Some(font) = self.load_font(name, FontHints::default())
        {
            return Some(font);
        }
        self.cjk(ordering, serif)
    }

    fn load_fallback_font(&self, script: u32, language: u32, hints: FontHints) -> Option<Font> {
        // Only CJK scripts come from installed fonts; MuPDF covers the others. Bold and
        // italic hints are ignored because MuPDF caches fallback fonts per script and
        // serif flag only.
        self.cjk(cjk_ordering(script, language)?, hints.serif)
    }
}

static INSTALL: Once = Once::new();

/// Installs Lectrix's font policy. Call once at startup, before opening documents; later
/// calls do nothing.
pub fn install() {
    INSTALL.call_once(|| mupdf::set_font_loader(LectrixFonts));
}

/// Builds the installed-font index on a background thread, so the first document that
/// needs a non-embedded, non-base-14 font finds it ready. Returns immediately.
pub fn warm_up_in_background() {
    let spawned = thread::Builder::new()
        .name("font-warm-up".into())
        .spawn(|| {
            installed_fonts();
        });
    drop(spawned);
}

/// Renders the text of a one-page document that uses a non-embedded "Arial", which makes
/// MuPDF ask the font loader (tests and benchmarks).
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
    fn base14_names_load_builtin_fonts() {
        let font = LectrixFonts
            .load_font("Helvetica-Bold", FontHints::default())
            .unwrap();
        assert!(font.name().contains("Helvetica"), "{}", font.name());
    }

    #[test]
    fn unknown_names_find_nothing() {
        assert!(
            LectrixFonts
                .load_font("LectrixNoSuchFont-Regular", FontHints::default())
                .is_none()
        );
    }

    /// Stock Windows fonts, found the way the crate's font-kit lookup found them.
    #[cfg(windows)]
    #[test]
    fn installed_windows_fonts_are_found() {
        let index = installed_fonts();
        assert!(index.len() > 50, "only {} faces", index.len());
        let file = |name: &str, bold, italic| {
            index
                .lookup(name, bold, italic)
                .and_then(|f| f.path.file_name())
                .map(|n| n.to_string_lossy().to_lowercase())
        };
        assert_eq!(file("ArialMT", false, false).as_deref(), Some("arial.ttf"));
        assert_eq!(
            file("TimesNewRomanPSMT", false, false).as_deref(),
            Some("times.ttf")
        );
        assert_eq!(
            file("Times New Roman", true, false).as_deref(),
            Some("timesbd.ttf")
        );
        assert_eq!(file("Arial", true, true).as_deref(), Some("arialbi.ttf"));
        assert_eq!(file("Tahoma", false, false).as_deref(), Some("tahoma.ttf"));
        let font = LectrixFonts
            .load_font("ArialMT", FontHints::default())
            .unwrap();
        assert_eq!(font.name(), "Arial");
        // Japanese text: the Windows CJK families (MS Gothic or Yu Gothic) are installed.
        assert!(
            LectrixFonts
                .cjk(CjkFontOrdering::AdobeJapan, false)
                .is_some()
        );
    }

    #[test]
    fn warm_up_succeeds() {
        warm_up().unwrap();
    }
}
