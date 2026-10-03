//! The write profile's building blocks (AGENTS.md section 5.1): metadata, colour and
//! opacity, popups, geometry keys, and [`synthesize`], which every write ends with.

use mupdf::pdf::{PdfAnnotation, PdfDocument, PdfObject};

use super::read::{self, ink_list_user};
use super::{Kind, POPUP_SIZE, RECT_MARGIN, Rgb, meta, quads};
use crate::error::{Error, Result};
use crate::ffi;
use crate::geometry::{Affine, PageGeometry, Point, Rect};
use crate::objects;

/// The `/DA` font of text boxes: base-14 Helvetica under MuPDF's resource name (rule 7).
pub(super) const FREE_TEXT_FONT: &str = "Helv";

/// Rule 6 metadata for a new annotation. `/P` is set by MuPDF on creation.
pub(super) fn write_metadata(
    doc: &PdfDocument,
    annot: &mut PdfAnnotation,
    author: &str,
) -> Result<()> {
    let mut obj = annot.object();
    let now = meta::pdf_date_now();
    obj.dict_put("NM", PdfObject::new_string(&meta::new_annotation_name())?)?;
    obj.dict_put("T", objects::text_string(doc, author)?)?;
    obj.dict_put("CreationDate", PdfObject::new_string(&now)?)?;
    obj.dict_put("M", PdfObject::new_string(&now)?)?;
    // Print, and nothing else: MuPDF gives notes NoZoom and NoRotate, which pdf.js and
    // PDFium do not honour; notes stay upright through their appearance instead.
    obj.dict_put("F", PdfObject::new_int(4)?)?;
    Ok(())
}

/// `/C`. For a text box, `/C` is the background fill (PDF 32000-1 12.5.6.6), so the text
/// colour goes into `/DA` and `/C` is written empty: no background (see
/// `docs/interop-profile.md`, rule 6).
pub(super) fn set_color(
    doc: &PdfDocument,
    annot: &mut PdfAnnotation,
    kind: Kind,
    color: Rgb,
) -> Result<()> {
    if kind == Kind::FreeText {
        annot.object().dict_put("C", doc.new_array()?)?;
        return Ok(());
    }
    annot.set_color(color.annotation_color())?;
    Ok(())
}

/// Opacity in `/CA` (rule 5), written also at 1, where MuPDF would leave the key out.
pub(super) fn set_opacity(annot: &mut PdfAnnotation, opacity: f32) -> Result<()> {
    let opacity = opacity.clamp(0.0, 1.0);
    annot.set_opacity(opacity)?;
    annot
        .object()
        .dict_put("CA", PdfObject::new_real(opacity)?)?;
    Ok(())
}

/// Gives the annotation a `/Popup` (rule 6) if it has none: next to the annotation,
/// written in user space, with the keys Acrobat writes on its own popups: `/P`, `/Parent`
/// and `/F 28` (Print, NoZoom, NoRotate).
pub(super) fn ensure_popup(
    doc: &PdfDocument,
    annot: &mut PdfAnnotation,
    geometry: &PageGeometry,
) -> Result<()> {
    let obj = annot.object();
    let anchor = match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r)?,
        None => None,
    }
    .map(|r| geometry.user_rect_to_view(r))
    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    if obj.get_dict("Popup")?.is_none() {
        // MuPDF creates the popup, links /Parent and adds it to /Annots; its /Rect is
        // replaced below.
        annot.set_popup(mupdf::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        })?;
    }
    let view = Rect::new(
        anchor.x1,
        anchor.y0,
        anchor.x1 + POPUP_SIZE.0,
        anchor.y0 + POPUP_SIZE.1,
    );
    let obj = annot.object();
    let (Some(mut popup), Some(page)) = (obj.get_dict("Popup")?, obj.get_dict("P")?) else {
        return Ok(());
    };
    popup.dict_put(
        "Rect",
        objects::rect_array(doc, geometry.view_rect_to_user(view))?,
    )?;
    popup.dict_put("P", page)?;
    popup.dict_put("F", PdfObject::new_int(28)?)?;
    Ok(())
}

/// `/InkList` from strokes in view space.
pub(super) fn put_ink(
    doc: &PdfDocument,
    obj: &mut PdfObject,
    strokes: &[Vec<Point>],
    geometry: &PageGeometry,
) -> Result<()> {
    let mut list = doc.new_array()?;
    for stroke in strokes {
        let values: Vec<f64> = stroke
            .iter()
            .flat_map(|p| {
                let u = geometry.view_to_user(*p);
                [u.x, u.y]
            })
            .collect();
        list.array_push(objects::real_array(doc, &values)?)?;
    }
    obj.dict_put("InkList", list)?;
    Ok(())
}

/// A text box's `/Rect` from its box in view space. `/RD` goes: [`synthesize`] writes
/// the margin around the box (rule 4).
pub(super) fn put_text_box(
    doc: &PdfDocument,
    obj: &mut PdfObject,
    view: Rect,
    geometry: &PageGeometry,
) -> Result<()> {
    obj.dict_put(
        "Rect",
        objects::rect_array(doc, geometry.view_rect_to_user(view))?,
    )?;
    obj.dict_delete("RD")?;
    Ok(())
}

/// The font name and size in a text box's `/DA` (Helvetica 12 if it has none).
pub(super) fn text_box_font(obj: &PdfObject) -> Result<(String, f32)> {
    let da = read::default_appearance(obj)?;
    Ok((
        da.font.unwrap_or_else(|| FREE_TEXT_FONT.to_owned()),
        da.size.map_or(12.0, |s| s as f32),
    ))
}

/// A sticky note's `/Rect` (user space) for an icon whose top-left corner is shown at
/// `at` (view space), `size` view points square.
///
/// Acrobat and MuPDF draw text annotations as if NoZoom and NoRotate were set (PDF 32000-1
/// 12.5.6.4): the icon stays upright and keeps its size, fixed at the upper-left corner of
/// `/Rect` in user space. So that corner is the point under `at`, whatever the page's
/// rotation. (PDFium and pdf.js turn the icon with the page instead; see
/// `docs/interop-profile.md`.)
pub(super) fn note_rect(at: Point, size: f64, geometry: &PageGeometry) -> Rect {
    let corner = geometry.view_to_user(at);
    // View points to user units (UserUnit).
    let unit = {
        let a = geometry.view_to_user(Point::new(0.0, 0.0));
        let b = geometry.view_to_user(Point::new(1.0, 0.0));
        (b.x - a.x).hypot(b.y - a.y)
    };
    let s = size * unit;
    Rect::new(corner.x, corner.y - s, corner.x + s, corner.y)
}

/// Moves a sticky note so its icon's top-left corner is shown at `bounds`' (view space);
/// the icon keeps its size and its popup moves along.
pub(super) fn move_note(
    doc: &PdfDocument,
    annot: &mut PdfAnnotation,
    bounds: Rect,
    geometry: &PageGeometry,
) -> Result<()> {
    let mut obj = annot.object();
    let Some(current) = current_rect(&obj)? else {
        return Err(Error::InvalidArgument("the note has no position".into()));
    };
    let bounds = bounds.normalized();
    let old_corner = geometry.user_to_view(Point::new(current.x0, current.y1));
    let corner = geometry.view_to_user(Point::new(bounds.x0, bounds.y0));
    let moved = Rect::new(
        corner.x,
        corner.y - current.height(),
        corner.x + current.width(),
        corner.y,
    );
    obj.dict_put("Rect", objects::rect_array(doc, moved)?)?;
    let (dx, dy) = (bounds.x0 - old_corner.x, bounds.y0 - old_corner.y);
    if let Some(mut popup) = obj.get_dict("Popup")?
        && let Some(r) = current_rect(&popup)?
    {
        let v = geometry.user_rect_to_view(r);
        let shifted = Rect::new(v.x0 + dx, v.y0 + dy, v.x1 + dx, v.y1 + dy);
        popup.dict_put(
            "Rect",
            objects::rect_array(doc, geometry.view_rect_to_user(shifted))?,
        )?;
    }
    Ok(())
}

/// Moves and scales a drawing so the bounds of its strokes become `bounds` (view space).
pub(super) fn reshape_ink(
    doc: &PdfDocument,
    annot: &mut PdfAnnotation,
    bounds: Rect,
    geometry: &PageGeometry,
) -> Result<()> {
    let mut obj = annot.object();
    let strokes: Vec<Vec<Point>> = ink_list_user(&obj)?
        .into_iter()
        .map(|s| s.into_iter().map(|p| geometry.user_to_view(p)).collect())
        .collect();
    let Some(old) = read::points_bounds(strokes.iter().flatten()) else {
        return Err(Error::InvalidArgument("the drawing has no strokes".into()));
    };
    let new = bounds.normalized();
    // A stroke that is a straight horizontal or vertical line has no extent on one axis:
    // only move it there.
    let axis = |o0: f64, o1: f64, n0: f64, n1: f64| {
        let span = o1 - o0;
        if span.abs() < 1e-6 {
            (1.0, n0 - o0)
        } else {
            let s = (n1 - n0) / span;
            (s, n0 - o0 * s)
        }
    };
    let (sx, tx) = axis(old.x0, old.x1, new.x0, new.x1);
    let (sy, ty) = axis(old.y0, old.y1, new.y0, new.y1);
    let mapped: Vec<Vec<Point>> = strokes
        .iter()
        .map(|s| {
            s.iter()
                .map(|p| Point::new(p.x * sx + tx, p.y * sy + ty))
                .collect()
        })
        .collect();
    put_ink(doc, &mut obj, &mapped, geometry)
}

/// Regenerates the annotation's normal appearance with MuPDF (rule 2) and applies the
/// profile's corrections: `/Rect` containing the content plus stroke and margin (rule 4),
/// a text box's margin in `/RD`, and note icons drawn the size of their `/Rect`.
pub(crate) fn synthesize(doc: &PdfDocument, annot: &mut PdfAnnotation) -> Result<()> {
    let subtype = super::subtype(&annot.object())?;
    let kind = Kind::from_subtype(&subtype);
    ffi::request_appearance(annot)?;
    annot.update()?;
    let obj = annot.object();
    if super::SYNTHESIZED.contains(&subtype.as_str()) && !read::has_appearance(&obj)? {
        return Err(Error::InvalidArgument(format!(
            "MuPDF could not draw an appearance for this {subtype} annotation"
        )));
    }
    match kind {
        Some(Kind::Note) => icon_box_fits_rect(doc, &obj)?,
        Some(Kind::FreeText) => {
            if let Some(box_) = current_rect(&obj)? {
                finalize_rect(doc, annot, box_.expand(RECT_MARGIN))?;
                let mut obj = annot.object();
                if let Some(rect) = current_rect(&obj)? {
                    let rd = [
                        box_.x0 - rect.x0,
                        box_.y0 - rect.y0,
                        rect.x1 - box_.x1,
                        rect.y1 - box_.y1,
                    ];
                    if rd.iter().any(|v| *v > 0.0) {
                        obj.dict_put("RD", objects::real_array(doc, &rd)?)?;
                    }
                }
            }
        }
        _ => {
            if let Some(content) = content_rect(&obj)? {
                finalize_rect(doc, annot, content)?;
            }
        }
    }
    Ok(())
}

fn current_rect(obj: &PdfObject) -> Result<Option<Rect>> {
    match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r),
        None => Ok(None),
    }
}

/// What `/Rect` must contain (rule 4): text-markup quads, or ink strokes plus the stroke
/// width, plus the margin. `None` for other types.
pub(crate) fn content_rect(obj: &PdfObject) -> Result<Option<Rect>> {
    match super::subtype(obj)?.as_str() {
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
            let values = match obj.get_dict("QuadPoints")? {
                Some(q) => objects::numbers(&q)?,
                None => None,
            };
            Ok(values
                .and_then(|v| quads::quads_from_array(&v))
                .and_then(|qs| {
                    qs.iter()
                        .map(quads::Quad::bounds)
                        .reduce(|a, b| a.union(&b))
                })
                .map(|r| r.expand(RECT_MARGIN)))
        }
        "Ink" => {
            let width = read::border_width(obj)?;
            let strokes = ink_list_user(obj)?;
            Ok(
                read::points_bounds(strokes.iter().flatten())
                    .map(|r| r.expand(width + RECT_MARGIN)),
            )
        }
        _ => Ok(None),
    }
}

/// Makes a note icon's appearance box the size of its `/Rect`. MuPDF draws the icon in a
/// 16 × 16 box and lets readers scale it into `/Rect`; but readers that treat notes as
/// NoZoom (Acrobat, MuPDF) draw the box at its own size, so a 20 pt `/Rect` would show
/// a 16 pt icon there and a 20 pt one in PDFium and pdf.js. The drawing is scaled inside
/// the stream instead, and `/BBox` set to the `/Rect`'s size.
fn icon_box_fits_rect(doc: &PdfDocument, obj: &PdfObject) -> Result<()> {
    let Some(rect) = current_rect(obj)? else {
        return Ok(());
    };
    let Some(mut normal) = obj
        .get_dict("AP")?
        .and_then(|ap| ap.get_dict("N").ok().flatten())
        .filter(|n| n.is_stream().unwrap_or(false))
    else {
        return Ok(());
    };
    let Some(bbox) = current_rect_key(&normal, "BBox")?.filter(|b| !b.is_empty()) else {
        return Ok(());
    };
    let (sx, sy) = (rect.width() / bbox.width(), rect.height() / bbox.height());
    if (sx - 1.0).abs() < 1e-6 && (sy - 1.0).abs() < 1e-6 && bbox.x0 == 0.0 && bbox.y0 == 0.0 {
        return Ok(());
    }
    let m = Affine::translate(-bbox.x0, -bbox.y0).then(&Affine::scale(sx, sy));
    let drawing = normal.read_stream()?;
    let mut content = format!(
        "q {} {} {} {} {} {} cm
",
        m.a, m.b, m.c, m.d, m.e, m.f
    )
    .into_bytes();
    content.extend_from_slice(&drawing);
    content.extend_from_slice(
        b"
Q
",
    );
    normal.write_stream_buffer(&mupdf::Buffer::from_copied_bytes(&content)?)?;
    normal.dict_put(
        "BBox",
        objects::rect_array(doc, Rect::new(0.0, 0.0, rect.width(), rect.height()))?,
    )?;
    Ok(())
}

fn current_rect_key(obj: &PdfObject, key: &str) -> Result<Option<Rect>> {
    match obj.get_dict(key)? {
        Some(r) => objects::rect(&r),
        None => Ok(None),
    }
}

/// Rule 4 after appearance synthesis: MuPDF sets `/Rect` to the appearance bounds, which
/// can be tighter than "content plus stroke plus 1 pt". Grow `/Rect` and the appearance
/// `/BBox` together, so the appearance is never rescaled into a different Rect.
pub(crate) fn finalize_rect(
    doc: &PdfDocument,
    annot: &PdfAnnotation,
    content_rect: Rect,
) -> Result<()> {
    let mut obj = annot.object();
    let Some(current) = current_rect(&obj)? else {
        obj.dict_put("Rect", objects::rect_array(doc, content_rect)?)?;
        return Ok(());
    };
    let wanted = current.union(&content_rect);
    if wanted == current {
        return Ok(());
    }
    if !grow_with_appearance(doc, &obj, current, wanted)? {
        return Ok(());
    }
    obj.dict_put("Rect", objects::rect_array(doc, wanted)?)?;
    Ok(())
}

/// Grows the normal appearance's `/BBox` so that it still maps onto the same place when
/// `/Rect` grows from `current` to `wanted` (PDF 32000-1 algorithm 8.1). Returns false,
/// changing nothing, when that is not possible: a `/Matrix` other than the identity.
/// True if there is no appearance.
pub(crate) fn grow_with_appearance(
    doc: &PdfDocument,
    obj: &PdfObject,
    current: Rect,
    wanted: Rect,
) -> Result<bool> {
    let ap = obj
        .get_dict("AP")?
        .and_then(|ap| ap.get_dict("N").ok().flatten());
    let Some(mut normal) = ap else {
        return Ok(true);
    };
    if !normal.is_stream()? {
        return Ok(false);
    }
    let identity = match normal.get_dict("Matrix")? {
        Some(m) => objects::numbers(&m)?.is_some_and(|v| v == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        None => true,
    };
    let bbox = match normal.get_dict("BBox")? {
        Some(b) => objects::rect(&b)?,
        None => None,
    };
    let Some(bbox) = bbox.filter(|b| !b.is_empty() && identity) else {
        return Ok(false);
    };
    if current.is_empty() {
        return Ok(false);
    }
    // The map from BBox onto the current Rect, and back.
    let sx = current.width() / bbox.width();
    let sy = current.height() / bbox.height();
    let to_form = |x: f64, y: f64| {
        Point::new(
            bbox.x0 + (x - current.x0) / sx,
            bbox.y0 + (y - current.y0) / sy,
        )
    };
    let a = to_form(wanted.x0, wanted.y0);
    let b = to_form(wanted.x1, wanted.y1);
    normal.dict_put(
        "BBox",
        objects::rect_array(doc, Rect::new(a.x, a.y, b.x, b.y))?,
    )?;
    Ok(true)
}
