//! What a document asks of a presentation (AGENTS.md section 6.1): each page's transition,
//! the effect it comes on screen with (`/Trans`, ISO 32000-2 section 12.4.4.1), and its
//! display duration, after which the next page comes by itself (`/Dur`).
//!
//! Read from the page dictionaries directly rather than through MuPDF's
//! `pdf_page_presentation`, which loads each page and drops a Fly transition's settings and
//! a direction of `/None`. Reading is lenient: a value of the wrong type is its default,
//! and a style Lectrix doesn't know is no transition.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::objects;

/// The longest transition played, in seconds; a longer one is played this long.
pub const MAX_TRANSITION_SECONDS: f32 = 10.0;

/// The effects of ISO 32000-2 Table 164, but Replace (`/R`), which is no transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStyle {
    Split,
    Blinds,
    Box,
    Wipe,
    Dissolve,
    Glitter,
    Fly,
    Push,
    Cover,
    Uncover,
    Fade,
}

impl TransitionStyle {
    /// The style named `/S` (`name` without the slash).
    pub fn from_name(name: &[u8]) -> Option<TransitionStyle> {
        Some(match name {
            b"Split" => TransitionStyle::Split,
            b"Blinds" => TransitionStyle::Blinds,
            b"Box" => TransitionStyle::Box,
            b"Wipe" => TransitionStyle::Wipe,
            b"Dissolve" => TransitionStyle::Dissolve,
            b"Glitter" => TransitionStyle::Glitter,
            b"Fly" => TransitionStyle::Fly,
            b"Push" => TransitionStyle::Push,
            b"Cover" => TransitionStyle::Cover,
            b"Uncover" => TransitionStyle::Uncover,
            b"Fade" => TransitionStyle::Fade,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            TransitionStyle::Split => "Split",
            TransitionStyle::Blinds => "Blinds",
            TransitionStyle::Box => "Box",
            TransitionStyle::Wipe => "Wipe",
            TransitionStyle::Dissolve => "Dissolve",
            TransitionStyle::Glitter => "Glitter",
            TransitionStyle::Fly => "Fly",
            TransitionStyle::Push => "Push",
            TransitionStyle::Cover => "Cover",
            TransitionStyle::Uncover => "Uncover",
            TransitionStyle::Fade => "Fade",
        }
    }
}

/// How a page comes on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub style: TransitionStyle,
    /// `/D`: how long it takes, in seconds (1 by default).
    pub seconds: f32,
    /// `/Dm`: Split and Blinds move horizontal lines (`/H`, the default) or vertical ones.
    pub horizontal: bool,
    /// `/M`: Split, Box and Fly move inward from the edges (`/I`, the default) or outward
    /// from the middle.
    pub inward: bool,
    /// `/Di`: the direction Wipe, Glitter, Fly, Cover, Uncover and Push move in, in
    /// degrees counter-clockwise from left to right (0, the default; 90 is bottom to top,
    /// 315 top-left to bottom-right); None for `/None`, a Fly that only scales.
    pub direction: Option<u16>,
    /// `/SS`: the scale a Fly starts at (flying in) or ends at (flying out); 1 by default.
    pub scale: f32,
    /// `/B`: whether what a Fly moves is opaque (false by default).
    pub opaque: bool,
}

/// What a page asks of a presentation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PagePresentation {
    pub transition: Option<Transition>,
    /// `/Dur`: how many seconds the page shows before the next comes by itself.
    pub seconds: Option<f32>,
}

impl PagePresentation {
    pub fn is_empty(&self) -> bool {
        self.transition.is_none() && self.seconds.is_none()
    }
}

/// What each page of `doc` asks, in page order.
pub fn page_presentation(doc: &PdfDocument) -> Result<Vec<PagePresentation>> {
    let count = doc.page_count()?;
    let mut out = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for page in 0..count {
        out.push(read_page(&doc.find_page(page)?)?);
    }
    Ok(out)
}

fn read_page(page: &PdfObject) -> Result<PagePresentation> {
    let seconds = match page.get_dict("Dur")? {
        Some(d) => objects::number(&d)?
            .map(|s| s as f32)
            .filter(|s| s.is_finite() && *s > 0.0),
        None => None,
    };
    let transition = match page.get_dict("Trans")? {
        Some(t) if t.is_dict()? => read_transition(&t)?,
        _ => None,
    };
    Ok(PagePresentation {
        transition,
        seconds,
    })
}

fn name_of(dict: &PdfObject, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(match dict.get_dict(key)? {
        Some(n) if n.is_name()? => Some(n.as_name()?.to_vec()),
        _ => None,
    })
}

fn read_transition(dict: &PdfObject) -> Result<Option<Transition>> {
    let Some(style) = name_of(dict, "S")?.and_then(|n| TransitionStyle::from_name(&n)) else {
        return Ok(None);
    };
    let number = |key: &str| -> Result<Option<f64>> {
        match dict.get_dict(key)? {
            Some(v) => objects::number(&v),
            None => Ok(None),
        }
    };
    let seconds = number("D")?
        .map(|d| d as f32)
        .filter(|d| d.is_finite() && *d >= 0.0)
        .unwrap_or(1.0)
        .min(MAX_TRANSITION_SECONDS);
    let direction = match dict.get_dict("Di")? {
        Some(d) if d.is_name()? => {
            if d.as_name()? == b"None" {
                None
            } else {
                Some(0)
            }
        }
        Some(d) => Some(
            objects::number(&d)?
                .filter(|v| v.is_finite())
                .map(|v| v.round().rem_euclid(360.0) as u16)
                .unwrap_or(0),
        ),
        None => Some(0),
    };
    let scale = number("SS")?
        .map(|s| s as f32)
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    let opaque = match dict.get_dict("B")? {
        Some(b) if b.is_bool()? => b.as_bool()?,
        _ => false,
    };
    Ok(Some(Transition {
        style,
        seconds,
        horizontal: name_of(dict, "Dm")?.as_deref() != Some(b"V"),
        inward: name_of(dict, "M")?.as_deref() != Some(b"O"),
        direction,
        scale,
        opaque,
    }))
}

/// Writes what page `page` asks of a presentation (for generated samples and tests; the
/// app only reads it): its `/Trans` and `/Dur`, or removes them where `what` has none.
pub fn write_page_presentation(
    doc: &PdfDocument,
    page: usize,
    what: &PagePresentation,
) -> Result<()> {
    let page_no = i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(page));
    }
    let mut obj = doc.find_page(page_no)?;
    match what.seconds {
        Some(s) => obj.dict_put("Dur", PdfObject::new_real(s)?)?,
        None => obj.dict_delete("Dur")?,
    }
    match &what.transition {
        Some(t) => {
            let mut trans = doc.new_dict()?;
            trans.dict_put("Type", PdfObject::new_name("Trans")?)?;
            trans.dict_put("S", PdfObject::new_name(t.style.name())?)?;
            trans.dict_put("D", PdfObject::new_real(t.seconds)?)?;
            trans.dict_put(
                "Dm",
                PdfObject::new_name(if t.horizontal { "H" } else { "V" })?,
            )?;
            trans.dict_put("M", PdfObject::new_name(if t.inward { "I" } else { "O" })?)?;
            match t.direction {
                Some(d) => trans.dict_put("Di", PdfObject::new_int(i32::from(d))?)?,
                None => trans.dict_put("Di", PdfObject::new_name("None")?)?,
            }
            trans.dict_put("SS", PdfObject::new_real(t.scale)?)?;
            trans.dict_put("B", PdfObject::new_bool(t.opaque))?;
            obj.dict_put("Trans", trans)?;
        }
        None => obj.dict_delete("Trans")?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    fn sample() -> PdfDocument {
        sample_document(&SampleSpec {
            pages: 3,
            ..SampleSpec::default()
        })
        .unwrap()
    }

    #[test]
    fn a_page_without_either_asks_nothing() {
        let doc = sample();
        let pages = page_presentation(&doc).unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages.iter().all(PagePresentation::is_empty));
    }

    #[test]
    fn what_is_written_is_read_back() {
        let doc = sample();
        let fly = PagePresentation {
            transition: Some(Transition {
                style: TransitionStyle::Fly,
                seconds: 0.5,
                horizontal: false,
                inward: false,
                direction: None,
                scale: 0.25,
                opaque: true,
            }),
            seconds: Some(3.0),
        };
        write_page_presentation(&doc, 1, &fly).unwrap();
        let pages = page_presentation(&doc).unwrap();
        assert_eq!(pages[1], fly);
        assert!(pages[0].is_empty() && pages[2].is_empty());
        write_page_presentation(&doc, 1, &PagePresentation::default()).unwrap();
        assert!(page_presentation(&doc).unwrap()[1].is_empty());
    }

    /// Sets `/Trans` on the first page to the dictionary `entries` (PDF syntax).
    fn read_trans(entries: &str) -> Option<Transition> {
        let doc = sample();
        let trans = doc
            .new_object_from_str(&format!("<< {entries} >>"))
            .unwrap();
        doc.find_page(0).unwrap().dict_put("Trans", trans).unwrap();
        page_presentation(&doc).unwrap()[0].transition
    }

    #[test]
    fn defaults_are_those_of_the_standard() {
        let t = read_trans("/S /Wipe").unwrap();
        assert_eq!(t.style, TransitionStyle::Wipe);
        assert_eq!(t.seconds, 1.0);
        assert!(t.horizontal && t.inward && !t.opaque);
        assert_eq!(t.direction, Some(0));
        assert_eq!(t.scale, 1.0);
    }

    #[test]
    fn reads_every_entry() {
        let t = read_trans("/S /Split /D 2 /Dm /V /M /O /Di 270").unwrap();
        assert_eq!(t.style, TransitionStyle::Split);
        assert_eq!(t.seconds, 2.0);
        assert!(!t.horizontal && !t.inward);
        assert_eq!(t.direction, Some(270));
        let t = read_trans("/S /Glitter /Di 315.0").unwrap();
        assert_eq!(t.direction, Some(315));
        let t = read_trans("/S /Fly /Di /None /SS 0.5 /B true").unwrap();
        assert_eq!(t.direction, None);
        assert_eq!(t.scale, 0.5);
        assert!(t.opaque);
    }

    #[test]
    fn replace_and_unknown_styles_are_no_transition() {
        assert_eq!(read_trans("/S /R /D 2"), None);
        assert_eq!(read_trans("/S /Spin"), None);
        assert_eq!(read_trans("/D 2"), None);
    }

    #[test]
    fn wrong_values_are_their_defaults_and_long_ones_are_cut() {
        let t = read_trans("/S /Box /D (two) /Dm 3 /M /Sideways /Di (left) /SS -1 /B 1").unwrap();
        assert_eq!(t.seconds, 1.0);
        assert!(t.horizontal && t.inward && !t.opaque);
        assert_eq!(t.direction, Some(0));
        assert_eq!(t.scale, 1.0);
        assert_eq!(
            read_trans("/S /Fade /D 600").unwrap().seconds,
            MAX_TRANSITION_SECONDS
        );
        assert_eq!(read_trans("/S /Push /Di -90").unwrap().direction, Some(270));
    }

    #[test]
    fn a_duration_that_is_not_a_positive_number_is_none() {
        let doc = sample();
        let mut page = doc.find_page(0).unwrap();
        page.dict_put("Dur", PdfObject::new_int(0).unwrap())
            .unwrap();
        let mut second = doc.find_page(1).unwrap();
        second
            .dict_put("Dur", PdfObject::new_name("Long").unwrap())
            .unwrap();
        let mut third = doc.find_page(2).unwrap();
        third
            .dict_put("Dur", PdfObject::new_int(4).unwrap())
            .unwrap();
        let pages = page_presentation(&doc).unwrap();
        assert_eq!(
            pages.iter().map(|p| p.seconds).collect::<Vec<_>>(),
            [None, None, Some(4.0)]
        );
    }
}
