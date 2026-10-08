//! The index of installed fonts and how a requested font name is matched against it
//! (ADR 0005). Portable: the platform module lists the installed faces.
//!
//! Matching follows what the `mupdf` crate's `system-fonts` feature did through
//! `font-kit`, so documents render with the same fonts: an exact PostScript name first,
//! then a family name (ignoring case, any language) with the CSS Fonts 3 §5.2 algorithm
//! for the requested bold and italic.

use std::collections::HashMap;
use std::path::PathBuf;

use mupdf::CjkFontOrdering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Oblique,
    Italic,
}

/// One installed font face.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFace {
    pub postscript_name: Option<String>,
    /// Family names in every language the font gives, English first.
    pub families: Vec<String>,
    /// 1 to 1000; 400 is regular, 700 bold.
    pub weight: u16,
    /// 1 (ultra-condensed) to 9 (ultra-expanded); 5 is normal.
    pub stretch: u8,
    pub style: Style,
    pub path: PathBuf,
    /// Face number inside a collection file (.ttc).
    pub index: u32,
}

impl FontFace {
    /// The name MuPDF gets for the font (the crate passed the family name).
    pub fn family_name(&self) -> &str {
        self.families.first().map(String::as_str).unwrap_or("")
    }
}

const NORMAL_STRETCH: u8 = 5;

#[derive(Debug, Default)]
pub struct FontIndex {
    faces: Vec<FontFace>,
    by_postscript: HashMap<String, usize>,
    /// Lower-cased family name to faces, in listing order.
    by_family: HashMap<String, Vec<usize>>,
}

impl FontIndex {
    pub fn new(faces: Vec<FontFace>) -> FontIndex {
        let mut by_postscript = HashMap::new();
        let mut by_family: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, face) in faces.iter().enumerate() {
            if let Some(ps) = &face.postscript_name {
                // The first face with a name wins, as in a search through the list.
                by_postscript.entry(ps.clone()).or_insert(i);
            }
            let mut seen = Vec::new();
            for family in &face.families {
                let key = family.to_lowercase();
                if !seen.contains(&key) {
                    by_family.entry(key.clone()).or_default().push(i);
                    seen.push(key);
                }
            }
        }
        FontIndex {
            faces,
            by_postscript,
            by_family,
        }
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// The face with this exact PostScript name.
    pub fn by_postscript(&self, name: &str) -> Option<&FontFace> {
        self.by_postscript.get(name).map(|&i| &self.faces[i])
    }

    /// The face of `family` (any case, any language) that best fits `bold` and `italic`.
    pub fn best_match(&self, family: &str, bold: bool, italic: bool) -> Option<&FontFace> {
        let candidates: Vec<&FontFace> = self
            .by_family
            .get(&family.to_lowercase())?
            .iter()
            .map(|&i| &self.faces[i])
            .collect();
        let weight = if bold { 700 } else { 400 };
        let style = if italic { Style::Italic } else { Style::Normal };
        best_match(&candidates, NORMAL_STRETCH, style, weight)
    }

    /// The face for a font name from a PDF, as the crate looked it up: the PostScript
    /// name, else the name without a trailing `MT`, `PS` or `IdentityH` as a family name.
    pub fn lookup(&self, name: &str, bold: bool, italic: bool) -> Option<&FontFace> {
        if let Some(face) = self.by_postscript(name) {
            return Some(face);
        }
        let mut family = name;
        for suffix in ["MT", "PS", "IdentityH"] {
            if let Some(stripped) = family.strip_suffix(suffix) {
                family = stripped;
            }
        }
        self.best_match(family, bold, italic)
    }

    /// An installed CJK face for `ordering`: the platform's usual families in the
    /// requested style, then in the other style (stock Windows has no serif Japanese or
    /// Korean font, and a sans glyph beats a blank).
    pub fn cjk(&self, ordering: CjkFontOrdering, serif: bool) -> Option<&FontFace> {
        cjk_families(ordering, serif)
            .iter()
            .chain(cjk_families(ordering, !serif))
            .find_map(|family| self.lookup(family, false, false))
    }
}

/// CSS Fonts 3 §5.2 font matching, steps 4a to 4c (as `font-kit` implements it): narrow
/// the candidates by stretch, then style, then weight, and take the first one left.
fn best_match<'a>(
    candidates: &[&'a FontFace],
    stretch: u8,
    style: Style,
    weight: u16,
) -> Option<&'a FontFace> {
    let mut set: Vec<&FontFace> = candidates.to_vec();
    if set.is_empty() {
        return None;
    }

    // 4a: stretch. Exact, else the closest narrower one (for normal or narrower
    // requests), else the closest wider one; the reverse for wider requests.
    let chosen_stretch = if set.iter().any(|f| f.stretch == stretch) {
        stretch
    } else {
        let narrower = set
            .iter()
            .filter(|f| f.stretch < stretch)
            .map(|f| f.stretch)
            .max();
        let wider = set
            .iter()
            .filter(|f| f.stretch > stretch)
            .map(|f| f.stretch)
            .min();
        let first = if stretch <= NORMAL_STRETCH {
            narrower.or(wider)
        } else {
            wider.or(narrower)
        };
        first?
    };
    set.retain(|f| f.stretch == chosen_stretch);

    // 4b: style, by preference order.
    let preference = match style {
        Style::Italic => [Style::Italic, Style::Oblique, Style::Normal],
        Style::Oblique => [Style::Oblique, Style::Italic, Style::Normal],
        Style::Normal => [Style::Normal, Style::Oblique, Style::Italic],
    };
    let chosen_style = preference
        .into_iter()
        .find(|s| set.iter().any(|f| f.style == *s))?;
    set.retain(|f| f.style == chosen_style);

    // 4c: weight.
    let has = |w: u16| set.iter().any(|f| f.weight == w);
    let chosen_weight = if has(weight) {
        weight
    } else if (400..450).contains(&weight) && has(500) {
        500
    } else if (450..=500).contains(&weight) && has(400) {
        400
    } else {
        let lighter = set
            .iter()
            .filter(|f| f.weight <= weight)
            .map(|f| f.weight)
            .max();
        let heavier = set
            .iter()
            .filter(|f| f.weight >= weight)
            .map(|f| f.weight)
            .min();
        let first = if weight <= 500 {
            lighter.or(heavier)
        } else {
            heavier.or(lighter)
        };
        first?
    };
    set.retain(|f| f.weight == chosen_weight);
    set.first().copied()
}

/// The CJK ordering to substitute for a script and language (`UCDN_SCRIPT_*`,
/// `FZ_LANG_*`), or `None` for other scripts. Mirrors MuPDF's
/// `fz_lookup_noto_stem_from_script`, as the crate did.
// bindgen gives the `FZ_LANG_*` enum constants the C compiler's enum type: `i32` with MSVC,
// `u32` with GCC and Clang, so the casts are needed on Windows only.
#[allow(clippy::unnecessary_cast)]
pub fn cjk_ordering(script: u32, language: u32) -> Option<CjkFontOrdering> {
    use mupdf_sys::{
        FZ_LANG_ja, FZ_LANG_ko, FZ_LANG_zh_Hans, UCDN_SCRIPT_BOPOMOFO, UCDN_SCRIPT_HAN,
        UCDN_SCRIPT_HANGUL, UCDN_SCRIPT_HIRAGANA, UCDN_SCRIPT_KATAKANA,
    };
    const JA: u32 = FZ_LANG_ja as u32;
    const KO: u32 = FZ_LANG_ko as u32;
    const ZH_HANS: u32 = FZ_LANG_zh_Hans as u32;
    match script {
        UCDN_SCRIPT_HANGUL => Some(CjkFontOrdering::AdobeKorea),
        UCDN_SCRIPT_HIRAGANA | UCDN_SCRIPT_KATAKANA => Some(CjkFontOrdering::AdobeJapan),
        UCDN_SCRIPT_BOPOMOFO => Some(CjkFontOrdering::AdobeCns),
        UCDN_SCRIPT_HAN => Some(match language {
            JA => CjkFontOrdering::AdobeJapan,
            KO => CjkFontOrdering::AdobeKorea,
            ZH_HANS => CjkFontOrdering::AdobeGb,
            _ => CjkFontOrdering::AdobeCns,
        }),
        _ => None,
    }
}

/// The families MuPDF's own Windows port substitutes for each CJK ordering, then the
/// fonts newer Windows versions ship instead (the crate's Windows table). Other platforms
/// get their lists with their font sources (docs/status.md, platform gaps).
fn cjk_families(ordering: CjkFontOrdering, serif: bool) -> &'static [&'static str] {
    use CjkFontOrdering::*;
    match (ordering, serif) {
        (AdobeGb, true) => &["SimSun", "NSimSun"],
        (AdobeGb, false) => &[
            "KaiTi",
            "KaiTi_GB2312",
            "Microsoft YaHei",
            "SimHei",
            "SimSun",
        ],
        (AdobeCns, true) => &["MingLiU", "PMingLiU"],
        (AdobeCns, false) => &["DFKaiShu-SB-Estd-BF", "Microsoft JhengHei", "MingLiU"],
        (AdobeJapan, true) => &["MS-Mincho", "MS Mincho", "Yu Mincho"],
        (AdobeJapan, false) => &["MS-Gothic", "MS Gothic", "Yu Gothic", "Meiryo"],
        (AdobeKorea, true) => &["Batang"],
        (AdobeKorea, false) => &["Gulim", "Malgun Gothic"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(ps: &str, family: &str, weight: u16, stretch: u8, style: Style) -> FontFace {
        FontFace {
            postscript_name: Some(ps.into()),
            families: vec![family.into()],
            weight,
            stretch,
            style,
            path: PathBuf::from(format!("{ps}.ttf")),
            index: 0,
        }
    }

    fn times() -> FontIndex {
        FontIndex::new(vec![
            face(
                "TimesNewRomanPSMT",
                "Times New Roman",
                400,
                5,
                Style::Normal,
            ),
            face(
                "TimesNewRomanPS-BoldMT",
                "Times New Roman",
                700,
                5,
                Style::Normal,
            ),
            face(
                "TimesNewRomanPS-ItalicMT",
                "Times New Roman",
                400,
                5,
                Style::Italic,
            ),
            face(
                "TimesNewRomanPS-BoldItalicMT",
                "Times New Roman",
                700,
                5,
                Style::Italic,
            ),
            face("Arial-Black", "Arial Black", 900, 5, Style::Normal),
            face("ArialNarrow", "Arial Narrow", 400, 3, Style::Normal),
            face("ArialNarrow-Bold", "Arial Narrow", 700, 3, Style::Normal),
            face("Sylfaen", "Sylfaen", 400, 5, Style::Oblique),
        ])
    }

    fn ps(face: Option<&FontFace>) -> Option<&str> {
        face.and_then(|f| f.postscript_name.as_deref())
    }

    #[test]
    fn postscript_names_match_exactly() {
        let index = times();
        assert_eq!(
            ps(index.lookup("TimesNewRomanPS-BoldMT", false, false)),
            Some("TimesNewRomanPS-BoldMT")
        );
        // PostScript names are case-sensitive; family names are not.
        assert_eq!(ps(index.by_postscript("timesnewromanpsmt")), None);
        assert_eq!(
            ps(index.lookup("times new roman", true, true)),
            Some("TimesNewRomanPS-BoldItalicMT")
        );
    }

    #[test]
    fn suffixes_are_stripped_before_family_lookup() {
        let mut faces = vec![face("X-Regular", "Garamond", 400, 5, Style::Normal)];
        faces.push(face("X-Bold", "Garamond", 700, 5, Style::Normal));
        let index = FontIndex::new(faces);
        assert_eq!(ps(index.lookup("GaramondMT", true, false)), Some("X-Bold"));
        assert_eq!(
            ps(index.lookup("GaramondPSMT", false, false)),
            Some("X-Regular")
        );
        assert_eq!(
            ps(index.lookup("GaramondIdentityH", false, false)),
            Some("X-Regular")
        );
        assert_eq!(ps(index.lookup("Unknown", false, false)), None);
    }

    #[test]
    fn css_matching_falls_back_by_stretch_style_and_weight() {
        let index = times();
        // No bold in Arial Black: the closest heavier weight.
        assert_eq!(
            ps(index.best_match("Arial Black", true, false)),
            Some("Arial-Black")
        );
        // Only condensed faces: the closest narrower stretch is taken.
        assert_eq!(
            ps(index.best_match("Arial Narrow", true, false)),
            Some("ArialNarrow-Bold")
        );
        // Italic requested, only oblique available.
        assert_eq!(
            ps(index.best_match("Sylfaen", false, true)),
            Some("Sylfaen")
        );
        // Normal requested, only italic faces left after stretch: italic is accepted.
        let italic_only = FontIndex::new(vec![face("I", "Only Italic", 400, 5, Style::Italic)]);
        assert_eq!(
            ps(italic_only.best_match("only italic", false, false)),
            Some("I")
        );
    }

    #[test]
    fn weight_rules_follow_css() {
        let faces = [
            face("W300", "W", 300, 5, Style::Normal),
            face("W500", "W", 500, 5, Style::Normal),
            face("W900", "W", 900, 5, Style::Normal),
        ];
        let candidates: Vec<&FontFace> = faces.iter().collect();
        let pick = |w| best_match(&candidates, 5, Style::Normal, w).map(|f| f.weight);
        assert_eq!(pick(400), Some(500), "400 checks 500 first");
        assert_eq!(pick(700), Some(900), "above 500: heavier first");
        assert_eq!(pick(200), Some(300), "below the lightest: the lightest");
    }

    #[test]
    #[allow(clippy::unnecessary_cast)] // As in `cjk_ordering`.
    fn cjk_orderings_follow_script_and_language() {
        use mupdf_sys::{FZ_LANG_ja, UCDN_SCRIPT_HAN, UCDN_SCRIPT_HANGUL, UCDN_SCRIPT_LATIN};
        assert_eq!(
            cjk_ordering(UCDN_SCRIPT_HAN, FZ_LANG_ja as u32),
            Some(CjkFontOrdering::AdobeJapan)
        );
        assert_eq!(
            cjk_ordering(UCDN_SCRIPT_HANGUL, 0),
            Some(CjkFontOrdering::AdobeKorea)
        );
        assert_eq!(
            cjk_ordering(UCDN_SCRIPT_HAN, 0),
            Some(CjkFontOrdering::AdobeCns)
        );
        assert_eq!(cjk_ordering(UCDN_SCRIPT_LATIN, 0), None);
    }

    #[test]
    fn cjk_prefers_the_requested_style_then_the_other() {
        let index = FontIndex::new(vec![face("MS-Gothic", "MS Gothic", 400, 5, Style::Normal)]);
        assert_eq!(
            ps(index.cjk(CjkFontOrdering::AdobeJapan, true)),
            Some("MS-Gothic")
        );
        assert_eq!(ps(index.cjk(CjkFontOrdering::AdobeKorea, false)), None);
    }
}
