//! Annotations, written to the interoperability profile in AGENTS.md section 5.1
//! (mirrored in `docs/interop-profile.md`).

pub mod meta;
pub mod quads;

use mupdf::color::AnnotationColor;
use mupdf::pdf::{PdfAnnotation, PdfAnnotationType, PdfDocument, PdfObject, PdfPage};

use crate::error::{Error, Result};
use crate::geometry::{PageGeometry, Rect, read_page_boxes};
use crate::objects;
use quads::{Quad, quad_points_array};

/// Margin added around the content when computing `/Rect` (rule 4).
pub const RECT_MARGIN: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

impl MarkupKind {
    fn annotation_type(self) -> PdfAnnotationType {
        match self {
            MarkupKind::Highlight => PdfAnnotationType::Highlight,
            MarkupKind::Underline => PdfAnnotationType::Underline,
            MarkupKind::StrikeOut => PdfAnnotationType::StrikeOut,
            MarkupKind::Squiggly => PdfAnnotationType::Squiggly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const YELLOW: Rgb = Rgb {
        r: 1.0,
        g: 0.92,
        b: 0.23,
    };

    fn annotation_color(self) -> AnnotationColor {
        AnnotationColor::Rgb {
            red: self.r,
            green: self.g,
            blue: self.b,
        }
    }
}

/// A new text-markup annotation.
#[derive(Debug, Clone)]
pub struct MarkupSpec {
    pub kind: MarkupKind,
    pub page: usize,
    /// Quads in view space (as produced by text selection or search).
    pub quads: Vec<Quad>,
    pub color: Rgb,
    /// 0.0 to 1.0.
    pub opacity: f32,
    pub author: String,
    /// Note text. A note also gets a linked `/Popup` (rule 6).
    pub note: Option<String>,
}

/// Identifies an annotation Folio created or edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationRef {
    pub page: usize,
    /// Object number of the annotation dictionary.
    pub xref: i32,
    /// `/NM` value.
    pub name: String,
}

/// Geometry of page `index`.
pub fn page_geometry(page: &PdfPage) -> Result<PageGeometry> {
    Ok(PageGeometry::new(&read_page_boxes(&page.object())?))
}

/// Creates a highlight, underline, strikeout or squiggly annotation.
///
/// The caller is responsible for journalling (wrapping this in an operation).
pub fn add_text_markup(doc: &mut PdfDocument, spec: &MarkupSpec) -> Result<AnnotationRef> {
    if spec.quads.is_empty() || !spec.quads.iter().all(Quad::is_valid) {
        return Err(Error::InvalidArgument(
            "a text markup needs at least one non-empty quad".into(),
        ));
    }
    let opacity = spec.opacity.clamp(0.0, 1.0);
    let page_no = i32::try_from(spec.page).map_err(|_| Error::PageOutOfRange(spec.page))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(spec.page));
    }
    let mut page = doc.load_pdf_page(page_no)?;
    let geometry = page_geometry(&page)?;
    let user_quads: Vec<Quad> = spec
        .quads
        .iter()
        .map(|q| q.view_to_user(&geometry))
        .collect();

    let mut annot = page.create_annotation(spec.kind.annotation_type())?;
    let mut obj = annot.object();

    // Rule 3: QuadPoints in Acrobat order, PDF user space.
    obj.dict_put(
        "QuadPoints",
        objects::real_array(doc, &quad_points_array(&user_quads))?,
    )?;
    // Rule 4: preliminary Rect; finalized after appearance synthesis.
    let content_rect = user_quads
        .iter()
        .map(Quad::bounds)
        .reduce(|a, b| a.union(&b))
        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
        .expand(RECT_MARGIN);
    obj.dict_put("Rect", objects::rect_array(doc, content_rect)?)?;

    // Rules 5 and 6: colour, opacity (/CA), metadata.
    annot.set_color(spec.color.annotation_color())?;
    annot.set_opacity(opacity)?;
    write_metadata(doc, &mut annot, &spec.author)?;
    if let Some(note) = spec.note.as_deref() {
        annot.set_contents(note)?;
        let anchor = geometry.user_rect_to_view(content_rect);
        let popup = mupdf::Rect {
            x0: anchor.x1 as f32,
            y0: anchor.y0 as f32,
            x1: (anchor.x1 + 200.0) as f32,
            y1: (anchor.y0 + 100.0) as f32,
        };
        annot.set_popup(popup)?;
        finish_popup(&annot)?;
    }

    // Rule 2: appearance stream from MuPDF's synthesis.
    annot.update()?;
    finalize_rect(doc, &annot, content_rect)?;

    let name = read_name(&annot.object())?.unwrap_or_default();
    Ok(AnnotationRef {
        page: spec.page,
        xref: annot.xref()?,
        name,
    })
}

/// Rule 6 metadata for a new annotation. `/P` and `/F 4` are set by MuPDF on creation and
/// re-asserted here.
fn write_metadata(doc: &PdfDocument, annot: &mut PdfAnnotation, author: &str) -> Result<()> {
    let mut obj = annot.object();
    let now = meta::pdf_date_now();
    obj.dict_put("NM", PdfObject::new_string(&meta::new_annotation_name())?)?;
    obj.dict_put("T", objects::text_string(doc, author)?)?;
    obj.dict_put("CreationDate", PdfObject::new_string(&now)?)?;
    obj.dict_put("M", PdfObject::new_string(&now)?)?;
    obj.dict_put("F", PdfObject::new_int(4)?)?;
    Ok(())
}

/// Gives the `/Popup` MuPDF created the keys Acrobat writes on its own popups: `/P` and
/// `/F 28` (Print, NoZoom, NoRotate). `/Parent` is set by MuPDF.
fn finish_popup(annot: &PdfAnnotation) -> Result<()> {
    let obj = annot.object();
    let (Some(mut popup), Some(page)) = (obj.get_dict("Popup")?, obj.get_dict("P")?) else {
        return Ok(());
    };
    popup.dict_put("P", page)?;
    popup.dict_put("F", PdfObject::new_int(28)?)?;
    Ok(())
}

/// Rule 4 after appearance synthesis: MuPDF sets `/Rect` to the appearance bounds, which
/// can be tighter than "content plus stroke plus 1 pt". Grow `/Rect` and the appearance
/// `/BBox` together, so the appearance is never rescaled into a different Rect.
fn finalize_rect(doc: &PdfDocument, annot: &PdfAnnotation, content_rect: Rect) -> Result<()> {
    let mut obj = annot.object();
    let current = match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r)?,
        None => None,
    };
    let Some(current) = current else {
        obj.dict_put("Rect", objects::rect_array(doc, content_rect)?)?;
        return Ok(());
    };
    let wanted = current.union(&content_rect);
    if wanted == current {
        return Ok(());
    }
    let ap = obj
        .get_dict("AP")?
        .and_then(|ap| ap.get_dict("N").ok().flatten());
    if let Some(mut normal) = ap {
        let identity = match normal.get_dict("Matrix")? {
            Some(m) => objects::numbers(&m)?.is_some_and(|v| v == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            None => true,
        };
        let bbox_matches = match normal.get_dict("BBox")? {
            Some(b) => objects::rect(&b)?.is_some_and(|b| rects_close(&b, &current)),
            None => false,
        };
        if !(identity && bbox_matches) {
            // Growing Rect alone would rescale this appearance; keep MuPDF's Rect, which
            // already contains the appearance.
            return Ok(());
        }
        normal.dict_put("BBox", objects::rect_array(doc, wanted)?)?;
    }
    obj.dict_put("Rect", objects::rect_array(doc, wanted)?)?;
    Ok(())
}

fn rects_close(a: &Rect, b: &Rect) -> bool {
    const EPS: f64 = 0.01;
    (a.x0 - b.x0).abs() < EPS
        && (a.y0 - b.y0).abs() < EPS
        && (a.x1 - b.x1).abs() < EPS
        && (a.y1 - b.y1).abs() < EPS
}

fn read_name(obj: &PdfObject) -> Result<Option<String>> {
    match obj.get_dict("NM")? {
        Some(nm) if nm.is_string()? => Ok(Some(nm.as_string_lossy()?)),
        _ => Ok(None),
    }
}
