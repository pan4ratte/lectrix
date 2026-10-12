//! Repairing annotations other apps wrote (AGENTS.md section 5.3): generate missing
//! appearances, put crossed QuadPoints into Acrobat's order, grow `/Rect` to contain the
//! content, add missing `/NM`, `/F 4`, `/M` and `/P`.
//!
//! Repair never changes an annotation's content, colour, author or position: it writes no
//! `/Contents`, `/C`, `/T`, quads' corners or ink points, only the keys above (and the
//! appearance, which MuPDF draws from those unchanged properties).
//!
//! Mirrored QuadPoints (the bottom edge first, as PDFKit apps write) are not a problem
//! while the annotation has an appearance, and stay as written: every reader shows that
//! appearance, and the app that wrote them reads them back in its own order (ADR 0017).
//! They are put in Acrobat's order only when Lectrix draws the annotation itself.

use std::collections::BTreeMap;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::quads::{Quad, crossed, out_of_order, quad_points_array, quads_from_array};
use super::read::{self, Problem};
use super::{SYNTHESIZED, meta, write};
use crate::error::Result;
use crate::geometry::{PageGeometry, Rect, read_page_boxes};
use crate::objects;

/// What a scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepairScan {
    /// Annotations with each problem.
    pub counts: BTreeMap<Problem, usize>,
    /// Annotations repair would change.
    pub fixable: usize,
    /// Annotations with a problem repair can't fix (and nothing it can).
    pub unfixable: usize,
}

impl RepairScan {
    pub fn is_clean(&self) -> bool {
        self.counts.is_empty()
    }
}

/// One change repair made, for the app log (with the object number, section 5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairChange {
    pub page: usize,
    pub id: u32,
    pub subtype: String,
    pub fixed: Problem,
}

impl std::fmt::Display for RepairChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.fixed {
            Problem::MissingAppearance => "generated the appearance stream",
            Problem::QuadOrder => "put QuadPoints into Acrobat's order",
            Problem::RectTooSmall => "grew /Rect to contain the content",
            Problem::MissingName => "added /NM",
            Problem::MissingFlags => "added /F 4",
            Problem::MissingModified => "added /M",
            Problem::MissingPage => "added /P",
            Problem::MalformedQuads => "left malformed QuadPoints",
        };
        write!(
            f,
            "annotation object {} ({}, page {}): {what}",
            self.id,
            self.subtype,
            self.page + 1
        )
    }
}

/// Counts the problems in every annotation of `doc`.
pub fn scan(doc: &PdfDocument) -> Result<RepairScan> {
    let mut out = RepairScan::default();
    for page in read::read_all(doc)? {
        for a in page {
            if a.problems.is_empty() {
                continue;
            }
            for p in &a.problems {
                *out.counts.entry(*p).or_default() += 1;
            }
            if a.id != 0 && a.problems.iter().any(|p| p.fixable()) {
                out.fixable += 1;
            } else {
                out.unfixable += 1;
            }
        }
    }
    Ok(out)
}

/// Fixes every fixable problem. The caller wraps this in one journal operation.
pub fn repair(doc: &mut PdfDocument) -> Result<Vec<RepairChange>> {
    let mut changes = Vec::new();
    for (page, annots) in read::read_all(doc)?.into_iter().enumerate() {
        let todo: Vec<_> = annots
            .into_iter()
            .filter(|a| a.id != 0 && a.problems.iter().any(|p| p.fixable()))
            .collect();
        if todo.is_empty() {
            continue;
        }
        let pdf_page = super::load_page(doc, page)?;
        let page_obj = pdf_page.object();
        let geometry = PageGeometry::new(&read_page_boxes(&page_obj)?);
        for info in todo {
            let mut annot = super::find(&pdf_page, info.id)?;
            let mut obj = annot.object();
            let mut fixed = Vec::new();
            let mut synthesize = info.problems.contains(&Problem::MissingAppearance);

            if info.problems.contains(&Problem::QuadOrder)
                && fix_quad_order(doc, &mut obj, &geometry, Corners::Crossed)?
            {
                fixed.push(Problem::QuadOrder);
            }
            if info.problems.contains(&Problem::MissingName) {
                obj.dict_put("NM", PdfObject::new_string(&meta::new_annotation_name())?)?;
                fixed.push(Problem::MissingName);
            }
            if info.problems.contains(&Problem::MissingFlags) {
                obj.dict_put("F", PdfObject::new_int(4)?)?;
                fixed.push(Problem::MissingFlags);
            }
            if info.problems.contains(&Problem::MissingModified) {
                obj.dict_put("M", PdfObject::new_string(&meta::pdf_date_now())?)?;
                fixed.push(Problem::MissingModified);
            }
            if info.problems.contains(&Problem::MissingPage) {
                obj.dict_put("P", page_obj.clone())?;
                fixed.push(Problem::MissingPage);
            }
            if info.problems.contains(&Problem::RectTooSmall) {
                let current = match obj.get_dict("Rect")? {
                    Some(r) => objects::rect(&r)?,
                    None => None,
                };
                if let Some(content) = write::content_rect(&obj)? {
                    let wanted = current.map_or(content, |c| c.union(&content));
                    let kept = match current {
                        Some(c) => write::grow_with_appearance(doc, &obj, c, wanted)?,
                        None => true,
                    };
                    if kept {
                        obj.dict_put("Rect", objects::rect_array(doc, wanted)?)?;
                        fixed.push(Problem::RectTooSmall);
                    } else if SYNTHESIZED.contains(&info.subtype.as_str()) {
                        // The appearance can't follow a bigger /Rect: draw a new one,
                        // which sets /Rect from the content.
                        obj.dict_put("Rect", objects::rect_array(doc, wanted)?)?;
                        synthesize = true;
                        fixed.push(Problem::RectTooSmall);
                    }
                }
            }
            if synthesize {
                // The new appearance is drawn from the corners as Acrobat orders them:
                // mirrored ones, left alone while an annotation keeps its own appearance,
                // are put in order for it.
                if fix_quad_order(doc, &mut obj, &geometry, Corners::OutOfOrder)?
                    && !fixed.contains(&Problem::QuadOrder)
                {
                    fixed.push(Problem::QuadOrder);
                }
                // A reply gets an empty appearance: MuPDF's would draw a note icon over its
                // parent, and readers show replies in the parent's thread (ADR 0012).
                if super::reply::is_reply(&annot.object())? {
                    super::reply::put_empty_appearance(doc, &mut annot.object())?;
                } else {
                    write::synthesize(doc, &mut annot)?;
                }
                if info.problems.contains(&Problem::MissingAppearance) {
                    fixed.push(Problem::MissingAppearance);
                }
            }
            fixed.sort();
            changes.extend(fixed.into_iter().map(|p| RepairChange {
                page,
                id: info.id,
                subtype: info.subtype.clone(),
                fixed: p,
            }));
        }
    }
    Ok(changes)
}

/// Which quads [`fix_quad_order`] rewrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Corners {
    /// Crossed ones: for an annotation that keeps the appearance it has (ADR 0017).
    Crossed,
    /// Every one not in Acrobat's order, mirrored ones too: for an annotation Lectrix is
    /// about to draw, since MuPDF draws from the corners as Acrobat orders them.
    OutOfOrder,
}

/// Rewrites `/QuadPoints` with the quads `which` names in Acrobat's order. Corners keep
/// their coordinates; only their order changes. Quads that have no order to put right
/// stay as they are. Returns false, writing nothing, if no quad changed.
pub(crate) fn fix_quad_order(
    doc: &PdfDocument,
    obj: &mut PdfObject,
    geometry: &PageGeometry,
    which: Corners,
) -> Result<bool> {
    let Some(quads) = (match obj.get_dict("QuadPoints")? {
        Some(q) => objects::numbers(&q)?.and_then(|v| quads_from_array(&v)),
        None => None,
    }) else {
        return Ok(false);
    };
    let ordered = |q: &Quad| match which {
        Corners::Crossed => crossed(q, geometry),
        Corners::OutOfOrder => out_of_order(q, geometry),
    };
    let mut changed = false;
    let fixed: Vec<Quad> = quads
        .iter()
        .map(|q| match ordered(q) {
            Some(ordered) => {
                changed = true;
                ordered
            }
            None => *q,
        })
        .collect();
    if !changed {
        return Ok(false);
    }
    obj.dict_put(
        "QuadPoints",
        objects::real_array(doc, &quad_points_array(&fixed))?,
    )?;
    Ok(true)
}

/// Bounds of `/Rect` of an annotation (for tests and the CLI).
pub fn rect_of(obj: &PdfObject) -> Result<Option<Rect>> {
    match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r),
        None => Ok(None),
    }
}
