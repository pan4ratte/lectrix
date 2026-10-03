//! Reading annotations for the annotation list (AGENTS.md sections 5.2 and 6.5), leniently:
//! whatever another app wrote is shown, and what the write profile would not accept is
//! reported as a [`Problem`] ("needs repair").
//!
//! Annotations are read from the page dictionaries' `/Annots`, without loading pages, so
//! listing a document with thousands of notes stays cheap.

use mupdf::pdf::{PdfDocument, PdfObject};

use super::quads::{Quad, is_acrobat_order, quads_from_array};
use super::{Kind, Rgb, SYNTHESIZED, meta};
use crate::error::Result;
use crate::geometry::{PageGeometry, Point, Rect, read_page_boxes};
use crate::objects;

/// What the write profile requires that an annotation lacks (section 5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Problem {
    /// No normal appearance stream: some readers show nothing.
    MissingAppearance,
    /// QuadPoints not in Acrobat's order: some readers draw a wrong shape.
    QuadOrder,
    /// `/Rect` does not contain the content: readers may cut it off.
    RectTooSmall,
    MissingName,
    /// No `/F`: the annotation is not printed.
    MissingFlags,
    MissingModified,
    /// No `/P` (the page it is on).
    MissingPage,
    /// QuadPoints missing or not a multiple of 8 numbers. Repair can't guess them.
    MalformedQuads,
}

impl Problem {
    pub const ALL: [Problem; 8] = [
        Problem::MissingAppearance,
        Problem::QuadOrder,
        Problem::RectTooSmall,
        Problem::MissingName,
        Problem::MissingFlags,
        Problem::MissingModified,
        Problem::MissingPage,
        Problem::MalformedQuads,
    ];

    /// Repair fixes everything except quads it can't read.
    pub fn fixable(self) -> bool {
        self != Problem::MalformedQuads
    }
}

/// One annotation as the list and the page overlay show it. Geometry is in view space
/// (points at zoom 1, origin top-left of the visible page, y down).
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationInfo {
    /// Object number; 0 for an annotation that is not an indirect object (shown, but
    /// cannot be edited).
    pub id: u32,
    pub page: usize,
    pub subtype: String,
    /// Set for the types Folio creates.
    pub kind: Option<Kind>,
    pub rect: Rect,
    /// What moving and resizing act on: text-markup quads, ink strokes, a text box without
    /// its margin, a note's icon; otherwise `/Rect`.
    pub bounds: Rect,
    pub quads: Vec<Quad>,
    pub ink: Vec<Vec<Point>>,
    /// FreeText: the text colour.
    pub color: Option<Rgb>,
    pub opacity: f32,
    pub contents: String,
    pub author: String,
    /// Milliseconds since the Unix epoch.
    pub created: Option<i64>,
    pub modified: Option<i64>,
    /// Object number of the annotation this one replies to (`/IRT`).
    pub reply_to: Option<u32>,
    /// Hidden or NoView: not drawn.
    pub hidden: bool,
    /// Ink stroke width.
    pub width: Option<f64>,
    /// FreeText font size.
    pub font_size: Option<f64>,
    pub problems: Vec<Problem>,
}

/// Annotations of every page (index = page), without popups, links and form widgets.
pub fn read_all(doc: &PdfDocument) -> Result<Vec<Vec<AnnotationInfo>>> {
    let count = usize::try_from(doc.page_count()?).unwrap_or(0);
    (0..count).map(|p| read_page(doc, p)).collect()
}

pub fn read_page(doc: &PdfDocument, page: usize) -> Result<Vec<AnnotationInfo>> {
    let page_obj = doc.find_page(i32::try_from(page).unwrap_or(i32::MAX))?;
    let Some(annots) = page_obj.get_dict("Annots")? else {
        return Ok(Vec::new());
    };
    let items = objects::array_items(&annots)?;
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let geometry = PageGeometry::new(&read_page_boxes(&page_obj)?);
    let mut out = Vec::new();
    for item in items {
        if !item.is_dict()? {
            continue;
        }
        if let Some(info) = read_one(&item, page, &geometry)? {
            out.push(info);
        }
    }
    Ok(out)
}

fn read_one(
    obj: &PdfObject,
    page: usize,
    geometry: &PageGeometry,
) -> Result<Option<AnnotationInfo>> {
    let subtype = super::subtype(obj)?;
    if matches!(subtype.as_str(), "Popup" | "Link" | "Widget" | "") {
        return Ok(None);
    }
    let id = if obj.is_indirect()? {
        u32::try_from(obj.as_indirect()?).unwrap_or(0)
    } else {
        0
    };
    let kind = Kind::from_subtype(&subtype);
    let user_rect = match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r)?,
        None => None,
    }
    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    let rect = geometry.user_rect_to_view(user_rect);

    let user_quads = match obj.get_dict("QuadPoints")? {
        Some(q) => objects::numbers(&q)?.and_then(|v| quads_from_array(&v)),
        None => None,
    };
    let quads: Vec<Quad> = user_quads
        .iter()
        .flatten()
        .map(|q| Quad {
            ul: geometry.user_to_view(q.ul),
            ur: geometry.user_to_view(q.ur),
            ll: geometry.user_to_view(q.ll),
            lr: geometry.user_to_view(q.lr),
        })
        .collect();
    let ink: Vec<Vec<Point>> = if subtype == "Ink" {
        ink_list_user(obj)?
            .into_iter()
            .map(|s| s.into_iter().map(|p| geometry.user_to_view(p)).collect())
            .collect()
    } else {
        Vec::new()
    };

    let mut font_size = None;
    let color = if kind == Some(Kind::FreeText) {
        let da = default_appearance(obj)?;
        font_size = da.size;
        da.color
    } else {
        match obj.get_dict("C")? {
            Some(c) => objects::numbers(&c)?.and_then(|v| Rgb::from_components(&v)),
            None => None,
        }
    };
    let bounds = match kind {
        Some(Kind::Ink) => points_bounds(ink.iter().flatten()).unwrap_or(rect),
        Some(k) if k.is_text_markup() => quads
            .iter()
            .map(Quad::bounds)
            .reduce(|a, b| a.union(&b))
            .unwrap_or(rect),
        Some(Kind::FreeText) => text_box_view(obj, geometry)?,
        _ => rect,
    };
    let opacity = match obj.get_dict("CA")? {
        Some(ca) => objects::number(&ca)?.map_or(1.0, |v| v.clamp(0.0, 1.0) as f32),
        None => 1.0,
    };
    let flags = match obj.get_dict("F")? {
        Some(f) if f.is_int()? => Some(f.as_int()?),
        _ => None,
    };
    let reply_to = match obj.get_dict("IRT")? {
        Some(r) if r.is_indirect()? => u32::try_from(r.as_indirect()?).ok(),
        _ => None,
    };
    let date = |key: &str| -> Result<Option<i64>> {
        Ok(match obj.get_dict(key)? {
            Some(d) if d.is_string()? => meta::parse_pdf_date(&d.as_string_lossy()?),
            _ => None,
        })
    };

    Ok(Some(AnnotationInfo {
        id,
        page,
        kind,
        rect,
        bounds,
        quads,
        ink,
        color,
        opacity,
        contents: contents(obj)?,
        author: string(obj, "T")?.unwrap_or_default(),
        created: date("CreationDate")?,
        modified: date("M")?,
        reply_to,
        hidden: flags.is_some_and(|f| f & (2 | 32) != 0),
        width: (subtype == "Ink").then(|| border_width(obj)).transpose()?,
        font_size,
        problems: problems(obj, &subtype, flags)?,
        subtype,
    }))
}

/// Everything the write profile requires that `obj` lacks, sorted.
pub(crate) fn problems(obj: &PdfObject, subtype: &str, flags: Option<i32>) -> Result<Vec<Problem>> {
    let mut out = Vec::new();
    let markup = matches!(
        subtype,
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly"
    );
    let quads = match obj.get_dict("QuadPoints")? {
        Some(q) if markup => objects::numbers(&q)?.and_then(|v| quads_from_array(&v)),
        _ => None,
    };
    // An appearance can't be drawn from quads that can't be read: that is reported as
    // malformed quads only.
    if SYNTHESIZED.contains(&subtype) && !has_appearance(obj)? && !(markup && quads.is_none()) {
        out.push(Problem::MissingAppearance);
    }
    if markup {
        match quads {
            None => out.push(Problem::MalformedQuads),

            Some(qs) => {
                if qs.iter().any(|q| q.is_valid() && !is_acrobat_order(q)) {
                    out.push(Problem::QuadOrder);
                }
            }
        }
    }
    if let Some(content) = raw_content(obj, subtype)? {
        let rect = match obj.get_dict("Rect")? {
            Some(r) => objects::rect(&r)?,
            None => None,
        };
        const EPS: f64 = 0.01;
        let contained = rect.is_some_and(|r| {
            r.x0 <= content.x0 + EPS
                && r.y0 <= content.y0 + EPS
                && r.x1 >= content.x1 - EPS
                && r.y1 >= content.y1 - EPS
        });
        if !contained {
            out.push(Problem::RectTooSmall);
        }
    }
    if string(obj, "NM")?.is_none_or(|s| s.is_empty()) {
        out.push(Problem::MissingName);
    }
    if flags.is_none() {
        out.push(Problem::MissingFlags);
    }
    if string(obj, "M")?.is_none() {
        out.push(Problem::MissingModified);
    }
    if !obj
        .get_dict("P")?
        .is_some_and(|p| p.is_dict().unwrap_or(false))
    {
        out.push(Problem::MissingPage);
    }
    out.sort();
    Ok(out)
}

/// The content an annotation draws, without margin: text-markup quads, or ink strokes
/// with half the stroke width. `None` for other types (their `/Rect` is their content).
pub(crate) fn raw_content(obj: &PdfObject, subtype: &str) -> Result<Option<Rect>> {
    match subtype {
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
            let quads = match obj.get_dict("QuadPoints")? {
                Some(q) => objects::numbers(&q)?.and_then(|v| quads_from_array(&v)),
                None => None,
            };
            Ok(quads.and_then(|qs| qs.iter().map(Quad::bounds).reduce(|a, b| a.union(&b))))
        }
        "Ink" => {
            let half = border_width(obj)? / 2.0;
            Ok(points_bounds(ink_list_user(obj)?.iter().flatten()).map(|r| r.expand(half)))
        }
        _ => Ok(None),
    }
}

/// True if the annotation has a usable normal appearance: `/AP /N` is a stream, or a
/// dictionary of states holding the one `/AS` names (any, if `/AS` is missing).
pub(crate) fn has_appearance(obj: &PdfObject) -> Result<bool> {
    let Some(ap) = obj.get_dict("AP")? else {
        return Ok(false);
    };
    let Some(n) = ap.get_dict("N")? else {
        return Ok(false);
    };
    if n.is_stream()? {
        return Ok(true);
    }
    if !n.is_dict()? {
        return Ok(false);
    }
    if let Some(state) = obj.get_dict("AS")?
        && state.is_name()?
    {
        let name = String::from_utf8_lossy(&state.as_name()?).into_owned();
        return Ok(n
            .get_dict(name.as_str())?
            .is_some_and(|s| s.is_stream().unwrap_or(false)));
    }
    for (_, value) in objects::dict_entries(&n)? {
        if value.is_stream()? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn string(obj: &PdfObject, key: &str) -> Result<Option<String>> {
    Ok(match obj.get_dict(key)? {
        Some(s) if s.is_string()? => Some(s.as_string_lossy()?),
        _ => None,
    })
}

/// `/Contents` ("" if missing).
pub(crate) fn contents(obj: &PdfObject) -> Result<String> {
    Ok(string(obj, "Contents")?.unwrap_or_default())
}

/// `/InkList` in user space; strokes with an odd number of values lose the last one.
pub(crate) fn ink_list_user(obj: &PdfObject) -> Result<Vec<Vec<Point>>> {
    let Some(list) = obj.get_dict("InkList")? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for stroke in objects::array_items(&list)? {
        if let Some(values) = objects::numbers(&stroke)? {
            out.push(
                values
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|[x, y]| Point::new(*x, *y))
                    .collect(),
            );
        }
    }
    Ok(out)
}

pub(crate) fn points_bounds<'a>(points: impl IntoIterator<Item = &'a Point>) -> Option<Rect> {
    let r = Rect::bounding(points.into_iter().copied());
    r.x0.is_finite().then_some(r)
}

/// Border width from `/BS /W`, else `/Border`'s third number, else 1 (the default).
pub(crate) fn border_width(obj: &PdfObject) -> Result<f64> {
    if let Some(bs) = obj.get_dict("BS")?
        && let Some(w) = bs.get_dict("W")?
        && let Some(w) = objects::number(&w)?
    {
        return Ok(w.max(0.0));
    }
    if let Some(border) = obj.get_dict("Border")?
        && let Some(values) = objects::numbers(&border)?
        && let Some(w) = values.get(2)
    {
        return Ok(w.max(0.0));
    }
    Ok(1.0)
}

/// What a `/DA` string sets.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DefaultAppearance {
    pub font: Option<String>,
    pub size: Option<f64>,
    pub color: Option<Rgb>,
}

/// Reads `/DA` ("/Helv 12 Tf 0 0 1 rg"): the last font and colour operators win.
pub(crate) fn default_appearance(obj: &PdfObject) -> Result<DefaultAppearance> {
    let da = string(obj, "DA")?.unwrap_or_default();
    Ok(parse_da(&da))
}

fn parse_da(da: &str) -> DefaultAppearance {
    let mut out = DefaultAppearance::default();
    let mut operands: Vec<&str> = Vec::new();
    for token in da.split_whitespace() {
        let numbers = |n: usize, ops: &[&str]| -> Option<Vec<f64>> {
            let start = ops.len().checked_sub(n)?;
            ops[start..].iter().map(|v| v.parse().ok()).collect()
        };
        match token {
            "Tf" => {
                if let [.., font, size] = operands.as_slice() {
                    out.font = font.strip_prefix('/').map(str::to_owned);
                    out.size = size.parse().ok().filter(|s: &f64| *s > 0.0);
                }
                operands.clear();
            }
            "g" | "rg" | "k" => {
                let n = match token {
                    "g" => 1,
                    "rg" => 3,
                    _ => 4,
                };
                if let Some(c) = numbers(n, &operands) {
                    out.color = Rgb::from_components(&c);
                }
                operands.clear();
            }
            op if op.chars().all(|c| c.is_ascii_alphabetic() || c == '*') => operands.clear(),
            operand => operands.push(operand),
        }
    }
    out
}

/// A text box's colour (from `/DA`).
pub(crate) fn text_box_color(obj: &PdfObject) -> Result<Option<Rgb>> {
    Ok(default_appearance(obj)?.color)
}

/// A text box in view space: `/Rect` without the `/RD` margin.
pub(crate) fn text_box_view(obj: &PdfObject, geometry: &PageGeometry) -> Result<Rect> {
    let rect = match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r)?,
        None => None,
    }
    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    let rd = match obj.get_dict("RD")? {
        Some(rd) => objects::numbers(&rd)?,
        None => None,
    };
    let inner = match rd.as_deref() {
        Some([l, b, r, t]) if rect.width() > l + r && rect.height() > b + t => {
            Rect::new(rect.x0 + l, rect.y0 + b, rect.x1 - r, rect.y1 - t)
        }
        _ => rect,
    };
    Ok(geometry.user_rect_to_view(inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_appearance_strings() {
        let da = parse_da("/Helv 12 Tf 0 0 1 rg");
        assert_eq!(da.font.as_deref(), Some("Helv"));
        assert_eq!(da.size, Some(12.0));
        assert_eq!(
            da.color,
            Some(Rgb {
                r: 0.0,
                g: 0.0,
                b: 1.0
            })
        );
        // Acrobat writes the colour first; gray and CMYK too.
        let da = parse_da("0.5 g /TiRo 9.5 Tf");
        assert_eq!(da.size, Some(9.5));
        assert_eq!(da.color.map(|c| c.r), Some(0.5));
        assert_eq!(parse_da("0 0 0 1 k").color, Some(Rgb::BLACK));
        assert_eq!(parse_da(""), DefaultAppearance::default());
        assert_eq!(parse_da("/Helv 0 Tf").size, None);
    }
}
