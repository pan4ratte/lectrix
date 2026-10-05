//! Annotations, written to the interoperability profile in AGENTS.md section 5.1
//! (mirrored in `docs/interop-profile.md`).
//!
//! - [`create`] makes the seven types Lectrix offers (rule 1).
//! - [`edit`] changes style, note text, author or position; [`delete`] removes one with
//!   its popup and replies.
//! - [`read`] lists every annotation for the sidebar, with problems that need repair.
//! - [`repair`] fixes other apps' annotations (section 5.3).
//! - [`reply`] adds replies, which are never drawn on the page (ADR 0012).
//!
//! Every write ends in [`write::synthesize`]: MuPDF's appearance synthesis (rule 2), then
//! the profile's own corrections (`/Rect`, FreeText margin, upright note icons).

pub mod ink;
pub mod meta;
pub mod quads;
pub mod read;
pub mod repair;
pub mod reply;
mod text_box;
mod write;

use mupdf::color::AnnotationColor;
use mupdf::pdf::{PdfAnnotation, PdfAnnotationType, PdfDocument, PdfObject, PdfPage};

use crate::error::{Error, Result};
use crate::geometry::{PageGeometry, Point, Rect, read_page_boxes};
use crate::objects;
use quads::{Quad, quad_points_array};

/// Margin added around the content when computing `/Rect` (rule 4).
pub const RECT_MARGIN: f64 = 1.0;
/// Side of a sticky note's icon, in points (view space).
pub const NOTE_SIZE: f64 = 20.0;
/// Size of a popup window, in points (view space).
const POPUP_SIZE: (f64, f64) = (200.0, 100.0);
/// Freehand strokes are simplified to within this distance, in points (rule 9).
pub const INK_TOLERANCE: f64 = 0.5;
/// The icon sticky notes get (Acrobat's default).
const NOTE_ICON: &str = "Comment";
/// FreeText font sizes Lectrix writes, in points.
pub const FONT_SIZES: std::ops::RangeInclusive<f64> = 4.0..=144.0;
/// Ink stroke widths Lectrix writes, in points.
pub const INK_WIDTHS: std::ops::RangeInclusive<f64> = 0.25..=48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

impl MarkupKind {
    pub fn kind(self) -> Kind {
        match self {
            MarkupKind::Highlight => Kind::Highlight,
            MarkupKind::Underline => Kind::Underline,
            MarkupKind::StrikeOut => Kind::StrikeOut,
            MarkupKind::Squiggly => Kind::Squiggly,
        }
    }
}

/// The annotation types Lectrix creates: standard subtypes only (rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    /// A sticky note (`/Text`).
    Note,
    /// Freehand drawing (`/Ink`).
    Ink,
    /// A text box (`/FreeText`).
    FreeText,
}

impl Kind {
    pub fn from_subtype(subtype: &str) -> Option<Kind> {
        Some(match subtype {
            "Highlight" => Kind::Highlight,
            "Underline" => Kind::Underline,
            "StrikeOut" => Kind::StrikeOut,
            "Squiggly" => Kind::Squiggly,
            "Text" => Kind::Note,
            "Ink" => Kind::Ink,
            "FreeText" => Kind::FreeText,
            _ => return None,
        })
    }

    pub fn subtype(self) -> &'static str {
        match self {
            Kind::Highlight => "Highlight",
            Kind::Underline => "Underline",
            Kind::StrikeOut => "StrikeOut",
            Kind::Squiggly => "Squiggly",
            Kind::Note => "Text",
            Kind::Ink => "Ink",
            Kind::FreeText => "FreeText",
        }
    }

    /// Lower-case name for undo steps ("Undo add highlight").
    pub fn label(self) -> &'static str {
        match self {
            Kind::Highlight => "highlight",
            Kind::Underline => "underline",
            Kind::StrikeOut => "strikeout",
            Kind::Squiggly => "squiggly underline",
            Kind::Note => "note",
            Kind::Ink => "drawing",
            Kind::FreeText => "text box",
        }
    }

    pub fn is_text_markup(self) -> bool {
        matches!(
            self,
            Kind::Highlight | Kind::Underline | Kind::StrikeOut | Kind::Squiggly
        )
    }

    fn annotation_type(self) -> PdfAnnotationType {
        match self {
            Kind::Highlight => PdfAnnotationType::Highlight,
            Kind::Underline => PdfAnnotationType::Underline,
            Kind::StrikeOut => PdfAnnotationType::StrikeOut,
            Kind::Squiggly => PdfAnnotationType::Squiggly,
            Kind::Note => PdfAnnotationType::Text,
            Kind::Ink => PdfAnnotationType::Ink,
            Kind::FreeText => PdfAnnotationType::FreeText,
        }
    }
}

/// Subtypes MuPDF can draw an appearance for. Repair generates missing appearances only
/// for these; the others (3D, Movie, Redact...) are left alone.
pub(crate) const SYNTHESIZED: &[&str] = &[
    "Text",
    "FreeText",
    "Line",
    "Square",
    "Circle",
    "Polygon",
    "PolyLine",
    "Highlight",
    "Underline",
    "Squiggly",
    "StrikeOut",
    "Caret",
    "Ink",
    "Stamp",
    "FileAttachment",
];

/// Subtypes whose colour and opacity can be changed: MuPDF redraws them from their
/// properties. (Stamps and attachments keep the picture they have.)
pub(crate) const RESTYLABLE: &[&str] = &[
    "Text",
    "FreeText",
    "Line",
    "Square",
    "Circle",
    "Polygon",
    "PolyLine",
    "Highlight",
    "Underline",
    "Squiggly",
    "StrikeOut",
    "Caret",
    "Ink",
];

/// Markup annotations (PDF 32000-1 table 170): they have a note, an author and replies.
pub(crate) const MARKUP: &[&str] = &[
    "Text",
    "FreeText",
    "Line",
    "Square",
    "Circle",
    "Polygon",
    "PolyLine",
    "Highlight",
    "Underline",
    "Squiggly",
    "StrikeOut",
    "Stamp",
    "Caret",
    "Ink",
    "FileAttachment",
    "Sound",
    "Redact",
];

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

    pub const BLACK: Rgb = Rgb {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    /// From a `/C` or `/DA` colour: 1 (gray), 3 (RGB) or 4 (CMYK) components.
    pub fn from_components(c: &[f64]) -> Option<Rgb> {
        let ch = |v: f64| v.clamp(0.0, 1.0) as f32;
        match c {
            [g] => Some(Rgb {
                r: ch(*g),
                g: ch(*g),
                b: ch(*g),
            }),
            [r, g, b] => Some(Rgb {
                r: ch(*r),
                g: ch(*g),
                b: ch(*b),
            }),
            [c, m, y, k] => Some(Rgb {
                r: ch((1.0 - c) * (1.0 - k)),
                g: ch((1.0 - m) * (1.0 - k)),
                b: ch((1.0 - y) * (1.0 - k)),
            }),
            _ => None,
        }
    }

    pub fn is_valid(&self) -> bool {
        [self.r, self.g, self.b]
            .iter()
            .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
    }

    fn annotation_color(self) -> AnnotationColor {
        AnnotationColor::Rgb {
            red: self.r,
            green: self.g,
            blue: self.b,
        }
    }
}

/// What a new annotation is, in view space (points at zoom 1, origin top-left of the
/// visible page, y down: as the frontend draws).
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Highlight, underline, strikeout or squiggly over text (or an area: one quad).
    Markup {
        kind: MarkupKind,
        quads: Vec<Quad>,
        /// Note text. A note also gets a linked `/Popup` (rule 6).
        note: Option<String>,
    },
    /// Text selected in the viewer: characters of the page's structured text, turned into
    /// quads in the text's own direction (AGENTS.md section 3).
    TextMarkup {
        kind: MarkupKind,
        ranges: Vec<crate::text::TextRange>,
        note: Option<String>,
    },
    /// A sticky note whose icon's top-left corner is at `at`.
    Note { at: Point, text: String },
    /// Freehand strokes, simplified before writing (rule 9).
    Ink {
        strokes: Vec<Vec<Point>>,
        width: f64,
    },
    /// A text box. Its height grows to fit the text.
    FreeText {
        rect: Rect,
        text: String,
        font_size: f64,
    },
}

impl Body {
    pub fn kind(&self) -> Kind {
        match self {
            Body::Markup { kind, .. } | Body::TextMarkup { kind, .. } => kind.kind(),
            Body::Note { .. } => Kind::Note,
            Body::Ink { .. } => Kind::Ink,
            Body::FreeText { .. } => Kind::FreeText,
        }
    }
}

/// A new annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAnnotation {
    pub page: usize,
    pub body: Body,
    /// The annotation's colour (FreeText: the text colour).
    pub color: Rgb,
    /// 0.0 to 1.0.
    pub opacity: f32,
    pub author: String,
}

impl NewAnnotation {
    /// Checks the values before anything is written.
    pub fn validate(&self) -> Result<()> {
        let bad = |what: &str| Err(Error::InvalidArgument(what.into()));
        if !self.color.is_valid() {
            return bad("the colour is not valid");
        }
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return bad("the opacity must be between 0 and 1");
        }
        match &self.body {
            Body::Markup { quads, .. } => {
                if quads.is_empty() || !quads.iter().all(Quad::is_valid) {
                    return bad("a text markup needs at least one non-empty quad");
                }
            }
            Body::TextMarkup { ranges, .. } => {
                if ranges.is_empty() || ranges.iter().any(|r| r.start >= r.end) {
                    return bad("no text is selected");
                }
            }
            Body::Note { at, .. } => {
                if !at.x.is_finite() || !at.y.is_finite() {
                    return bad("the note's position is not a number");
                }
            }
            Body::Ink { strokes, width } => {
                if strokes.is_empty() || strokes.iter().any(Vec::is_empty) {
                    return bad("a drawing needs at least one stroke");
                }
                if strokes
                    .iter()
                    .flatten()
                    .any(|p| !p.x.is_finite() || !p.y.is_finite())
                {
                    return bad("a stroke point is not a number");
                }
                if !INK_WIDTHS.contains(width) {
                    return bad("the stroke width is out of range");
                }
            }
            Body::FreeText {
                rect, font_size, ..
            } => {
                let r = rect.normalized();
                if ![r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()) || r.width() < 1.0 {
                    return bad("the text box is too small");
                }
                if !FONT_SIZES.contains(font_size) {
                    return bad("the font size is out of range");
                }
            }
        }
        Ok(())
    }
}

/// A change to one annotation. `None` leaves a property as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotationEdit {
    /// FreeText: the text colour.
    pub color: Option<Rgb>,
    pub opacity: Option<f32>,
    /// The note text (FreeText: the text in the box). Empty removes a note.
    pub contents: Option<String>,
    pub author: Option<String>,
    /// Where the annotation's content goes, in view space: moves a note (its size stays),
    /// moves or resizes a drawing or a text box. See [`read::AnnotationInfo::bounds`].
    pub bounds: Option<Rect>,
    /// Ink stroke width.
    pub width: Option<f64>,
    /// FreeText font size.
    pub font_size: Option<f64>,
    /// Turns a text-markup annotation into another text-markup type (Highlight,
    /// Underline, StrikeOut, Squiggly); its quads, colour and note stay.
    pub kind: Option<MarkupKind>,
}

impl AnnotationEdit {
    pub fn is_empty(&self) -> bool {
        *self == AnnotationEdit::default()
    }

    /// What the undo step is called ("Move", "Change", "Edit note of").
    pub fn verb(&self) -> &'static str {
        let without_bounds = AnnotationEdit {
            bounds: None,
            ..self.clone()
        };
        let without_contents = AnnotationEdit {
            contents: None,
            ..self.clone()
        };
        if self.bounds.is_some() && without_bounds.is_empty() {
            "Move"
        } else if self.contents.is_some() && without_contents.is_empty() {
            "Edit text of"
        } else {
            "Change"
        }
    }

    pub fn validate(&self) -> Result<()> {
        let bad = |what: &str| Err(Error::InvalidArgument(what.into()));
        if self.is_empty() {
            return bad("nothing to change");
        }
        if self.color.is_some_and(|c| !c.is_valid()) {
            return bad("the colour is not valid");
        }
        if self
            .opacity
            .is_some_and(|o| !o.is_finite() || !(0.0..=1.0).contains(&o))
        {
            return bad("the opacity must be between 0 and 1");
        }
        if let Some(b) = self.bounds {
            let b = b.normalized();
            if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) {
                return bad("the position is not a number");
            }
        }
        if self.width.is_some_and(|w| !INK_WIDTHS.contains(&w)) {
            return bad("the stroke width is out of range");
        }
        if self.font_size.is_some_and(|s| !FONT_SIZES.contains(&s)) {
            return bad("the font size is out of range");
        }
        Ok(())
    }
}

/// Identifies an annotation Lectrix created or edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationRef {
    pub page: usize,
    /// Object number of the annotation dictionary.
    pub xref: i32,
    /// `/NM` value.
    pub name: String,
}

/// Geometry of a loaded page.
pub fn page_geometry(page: &PdfPage) -> Result<PageGeometry> {
    Ok(PageGeometry::new(&read_page_boxes(&page.object())?))
}

fn load_page(doc: &PdfDocument, page: usize) -> Result<PdfPage> {
    let page_no = i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(page));
    }
    Ok(doc.load_pdf_page(page_no)?)
}

/// The annotation with object number `id` on `page`.
fn find(page: &PdfPage, id: u32) -> Result<PdfAnnotation> {
    for annot in page.annotations() {
        if u32::try_from(annot.xref()?).ok() == Some(id) {
            return Ok(annot);
        }
    }
    Err(Error::InvalidArgument(format!(
        "annotation {id} is not on this page"
    )))
}

/// `/Subtype` of an annotation dictionary ("" if missing).
pub(crate) fn subtype(obj: &PdfObject) -> Result<String> {
    Ok(match obj.get_dict("Subtype")? {
        Some(s) if s.is_name()? => String::from_utf8_lossy(&s.as_name()?).into_owned(),
        _ => String::new(),
    })
}

/// The subtype of annotation `id` on `page`, if it is there (for undo step names).
pub fn subtype_of(doc: &PdfDocument, page: usize, id: u32) -> Option<String> {
    let page = load_page(doc, page).ok()?;
    let annot = find(&page, id).ok()?;
    subtype(&annot.object()).ok()
}

/// Creates an annotation to the write profile. The caller wraps this in a journal
/// operation.
pub fn create(doc: &mut PdfDocument, new: &NewAnnotation) -> Result<AnnotationRef> {
    new.validate()?;
    if let Body::TextMarkup { kind, ranges, note } = &new.body {
        // The same text the viewer selected: structured text of the page as displayed.
        let list = crate::render::display_list(doc, new.page)?;
        let quads = crate::text::range_quads(&list, ranges)?;
        let resolved = NewAnnotation {
            body: Body::Markup {
                kind: *kind,
                quads,
                note: note.clone(),
            },
            ..new.clone()
        };
        return create(doc, &resolved);
    }
    let mut page = load_page(doc, new.page)?;
    let geometry = page_geometry(&page)?;
    let kind = new.body.kind();
    let mut annot = page.create_annotation(kind.annotation_type())?;
    let mut obj = annot.object();

    match &new.body {
        // Resolved to quads above.
        Body::TextMarkup { .. } => {
            return Err(Error::InvalidArgument("unresolved text markup".into()));
        }
        Body::Markup { quads, .. } => {
            let user: Vec<Quad> = quads.iter().map(|q| q.view_to_user(&geometry)).collect();
            // Rule 3: QuadPoints in Acrobat order, PDF user space.
            obj.dict_put(
                "QuadPoints",
                objects::real_array(doc, &quad_points_array(&user))?,
            )?;
            let content = user
                .iter()
                .map(Quad::bounds)
                .reduce(|a, b| a.union(&b))
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            obj.dict_put(
                "Rect",
                objects::rect_array(doc, content.expand(RECT_MARGIN))?,
            )?;
        }
        Body::Note { at, .. } => {
            let rect = write::note_rect(*at, NOTE_SIZE, &geometry);
            obj.dict_put("Rect", objects::rect_array(doc, rect)?)?;
            annot.set_icon_name(NOTE_ICON)?;
        }
        Body::Ink { strokes, width } => {
            let strokes: Vec<Vec<Point>> = strokes
                .iter()
                .map(|s| ink::simplify(s, INK_TOLERANCE))
                .collect();
            write::put_ink(doc, &mut obj, &strokes, &geometry)?;
            annot.set_border_width(*width as f32)?;
        }
        Body::FreeText {
            rect,
            text,
            font_size,
        } => {
            annot.set_default_appearance(
                write::FREE_TEXT_FONT,
                *font_size as f32,
                Some(new.color.annotation_color()),
            )?;
            annot.set_contents(text)?;
            // MuPDF's create writes a callout line (/CL) on every text box; it means
            // something only for callouts (/IT /FreeTextCallout), which Lectrix doesn't make.
            obj.dict_delete("CL")?;
            let fitted = text_box::fit(rect.normalized(), text, *font_size, false)?;
            write::put_text_box(doc, &mut obj, fitted, &geometry)?;
        }
    }

    // Rules 5 and 6: colour, opacity (/CA), metadata.
    write::set_color(doc, &mut annot, kind, new.color)?;
    write::set_opacity(&mut annot, new.opacity)?;
    write::write_metadata(doc, &mut annot, &new.author)?;
    match &new.body {
        Body::Markup {
            note: Some(note), ..
        }
        | Body::TextMarkup {
            note: Some(note), ..
        } if !note.is_empty() => {
            annot.set_contents(note)?;
            write::ensure_popup(doc, &mut annot, &geometry)?;
        }
        Body::Note { text, .. } => {
            if !text.is_empty() {
                annot.set_contents(text)?;
            }
            write::ensure_popup(doc, &mut annot, &geometry)?;
        }
        _ => {}
    }

    // Rule 2: appearance stream, then the profile's /Rect.
    write::synthesize(doc, &mut annot)?;

    Ok(AnnotationRef {
        page: new.page,
        xref: annot.xref()?,
        name: read_name(&annot.object())?.unwrap_or_default(),
    })
}

/// A new text-markup annotation (kept for the CLI and tests; see [`create`]).
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

/// Creates a highlight, underline, strikeout or squiggly annotation.
pub fn add_text_markup(doc: &mut PdfDocument, spec: &MarkupSpec) -> Result<AnnotationRef> {
    create(
        doc,
        &NewAnnotation {
            page: spec.page,
            body: Body::Markup {
                kind: spec.kind,
                quads: spec.quads.clone(),
                note: spec.note.clone(),
            },
            color: spec.color,
            opacity: spec.opacity.clamp(0.0, 1.0),
            author: spec.author.clone(),
        },
    )
}

/// Changes one annotation (rule 10: only the edited keys, plus `/M` and the appearance;
/// unknown keys stay).
pub fn edit(doc: &mut PdfDocument, page: usize, id: u32, edit: &AnnotationEdit) -> Result<()> {
    edit.validate()?;
    let pdf_page = load_page(doc, page)?;
    let geometry = page_geometry(&pdf_page)?;
    let mut annot = find(&pdf_page, id)?;
    let mut obj = annot.object();
    if reply::is_reply(&obj)? {
        return reply::edit_reply(doc, &mut annot, edit);
    }
    let subtype = subtype(&obj)?;
    let kind = Kind::from_subtype(&subtype);
    let refuse = |what: &str| {
        Err(Error::InvalidArgument(format!(
            "{what} of a {subtype} annotation can't be changed"
        )))
    };

    if (edit.color.is_some() || edit.opacity.is_some()) && !RESTYLABLE.contains(&subtype.as_str()) {
        return refuse("the colour");
    }
    if (edit.contents.is_some() || edit.author.is_some()) && !MARKUP.contains(&subtype.as_str()) {
        return refuse("the text");
    }
    if edit.width.is_some() && kind != Some(Kind::Ink) {
        return refuse("the stroke width");
    }
    if edit.font_size.is_some() && kind != Some(Kind::FreeText) {
        return refuse("the font size");
    }
    if edit.kind.is_some() && !kind.is_some_and(Kind::is_text_markup) {
        return refuse("the type");
    }
    let callout = match obj.get_dict("IT")? {
        Some(it) if it.is_name()? => it.as_name()? == b"FreeTextCallout",
        _ => false,
    };

    if edit.bounds.is_some()
        && !(matches!(kind, Some(Kind::Note | Kind::Ink))
            || (kind == Some(Kind::FreeText) && !callout))
    {
        return refuse("the position");
    }

    if let Some(color) = edit.color {
        match kind {
            Some(Kind::FreeText) => {
                let (font, size) = write::text_box_font(&obj)?;
                annot.set_default_appearance(&font, size, Some(color.annotation_color()))?;
            }
            _ => annot.set_color(color.annotation_color())?,
        }
    }
    if let Some(opacity) = edit.opacity {
        write::set_opacity(&mut annot, opacity)?;
    }
    if let Some(author) = &edit.author {
        obj.dict_put("T", objects::text_string(doc, author)?)?;
    }
    if let Some(width) = edit.width {
        annot.set_border_width(width as f32)?;
    }
    if let Some(size) = edit.font_size {
        let (font, _) = write::text_box_font(&obj)?;
        let color = read::text_box_color(&obj)?.unwrap_or(Rgb::BLACK);
        annot.set_default_appearance(&font, size as f32, Some(color.annotation_color()))?;
    }
    if let Some(new_kind) = edit.kind.map(MarkupKind::kind)
        && Some(new_kind) != kind
    {
        obj.dict_put("Subtype", PdfObject::new_name(new_kind.subtype())?)?;
        // The old look goes entirely: synthesis below writes /N, and a rollover or down
        // appearance from another app would still show the old type.
        obj.dict_delete("AP")?;
    }
    if let Some(text) = &edit.contents {
        if text.is_empty() && kind != Some(Kind::FreeText) {
            obj.dict_delete("Contents")?;
            obj.dict_delete("RC")?;
        } else {
            // MuPDF also removes /RC (rich text), which would otherwise be shown instead.
            annot.set_contents(text)?;
            let has_popup = obj.get_dict("Popup")?.is_some();
            if !has_popup && kind != Some(Kind::FreeText) && subtype != "Popup" {
                write::ensure_popup(doc, &mut annot, &geometry)?;
            }
        }
    }

    match (kind, edit.bounds) {
        (Some(Kind::Note), Some(b)) => write::move_note(doc, &mut annot, b, &geometry)?,
        (Some(Kind::Ink), Some(b)) => write::reshape_ink(doc, &mut annot, b, &geometry)?,
        _ => {}
    }
    if kind == Some(Kind::FreeText)
        && (edit.bounds.is_some() || edit.contents.is_some() || edit.font_size.is_some())
    {
        let current = read::text_box_view(&obj, &geometry)?;
        let (target, grow_only) = match edit.bounds {
            Some(b) => (b.normalized(), true),
            None => (current, false),
        };
        let text = read::contents(&obj)?;
        let (_, size) = write::text_box_font(&obj)?;
        let fitted = text_box::fit(target, &text, f64::from(size), grow_only)?;
        let mut obj = annot.object();
        write::put_text_box(doc, &mut obj, fitted, &geometry)?;
    }

    annot
        .object()
        .dict_put("M", PdfObject::new_string(&meta::pdf_date_now())?)?;
    write::synthesize(doc, &mut annot)
}

/// Deletes an annotation, its popup, and its replies (`/IRT`) on the same page, as
/// Acrobat does.
pub fn delete(doc: &mut PdfDocument, page: usize, id: u32) -> Result<()> {
    let mut pdf_page = load_page(doc, page)?;
    let annot = find(&pdf_page, id)?;
    if matches!(
        subtype(&annot.object())?.as_str(),
        "Popup" | "Link" | "Widget"
    ) {
        return Err(Error::InvalidArgument(
            "links, form fields and popups are not deleted here".into(),
        ));
    }
    drop(annot);
    // Replies, and replies to replies.
    let mut doomed = vec![id];
    let mut i = 0;
    while i < doomed.len() {
        let parent = doomed[i];
        for a in pdf_page.annotations() {
            let obj = a.object();
            let irt = match obj.get_dict("IRT")? {
                Some(r) if r.is_indirect()? => u32::try_from(r.as_indirect()?).ok(),
                _ => None,
            };
            let xref = u32::try_from(a.xref()?).ok();
            if irt == Some(parent)
                && let Some(x) = xref
                && !doomed.contains(&x)
            {
                doomed.push(x);
            }
        }
        i += 1;
    }
    for id in doomed {
        let annot = find(&pdf_page, id)?;
        // MuPDF removes the popup from /Annots too.
        pdf_page.delete_annotation(annot)?;
    }
    Ok(())
}

fn read_name(obj: &PdfObject) -> Result<Option<String>> {
    match obj.get_dict("NM")? {
        Some(nm) if nm.is_string()? => Ok(Some(nm.as_string_lossy()?)),
        _ => Ok(None),
    }
}
