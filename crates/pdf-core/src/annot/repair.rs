//! Repairing annotations other apps wrote (AGENTS.md section 5.3): generate missing
//! appearances, put QuadPoints into Acrobat's order, grow `/Rect` to contain the content,
//! add missing `/NM`, `/F 4`, `/M` and `/P`.
//!
//! Repair never changes an annotation's content, colour, author or position: it writes no
//! `/Contents`, `/C`, `/T`, quads' corners or ink points, only the keys above (and the
//! appearance, which MuPDF draws from those unchanged properties).

use std::collections::BTreeMap;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::quads::{Quad, is_acrobat_order, quad_points_array, quads_from_array};
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
                && fix_quad_order(doc, &mut obj, &geometry)?
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
                write::synthesize(doc, &mut annot, &geometry)?;
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

/// Rewrites `/QuadPoints` with every quad in Acrobat's order. Corners keep their
/// coordinates; only their order changes. Returns false if a quad's order can't be
/// worked out (nothing is written then).
fn fix_quad_order(doc: &PdfDocument, obj: &mut PdfObject, geometry: &PageGeometry) -> Result<bool> {
    let Some(quads) = (match obj.get_dict("QuadPoints")? {
        Some(q) => objects::numbers(&q)?.and_then(|v| quads_from_array(&v)),
        None => None,
    }) else {
        return Ok(false);
    };
    let mut fixed = Vec::with_capacity(quads.len());
    for q in &quads {
        if !q.is_valid() || is_acrobat_order(q) {
            fixed.push(*q);
            continue;
        }
        match reorder(q, geometry) {
            Some(r) => fixed.push(r),
            None => return Ok(false),
        }
    }
    obj.dict_put(
        "QuadPoints",
        objects::real_array(doc, &quad_points_array(&fixed))?,
    )?;
    Ok(true)
}

/// The same four corners in Acrobat's order. Other apps' orders are tried first (they keep
/// the text direction the writer meant): the spec's counter-clockwise order from the lower
/// left, clockwise from the upper left, and the bottom edge first. Failing those, the
/// corners are taken as they appear on screen (upper left is the top-left on the page as
/// shown).
pub fn reorder(q: &Quad, geometry: &PageGeometry) -> Option<Quad> {
    let p = [q.ul, q.ur, q.ll, q.lr];
    let candidates = [
        // Spec text: LL, LR, UR, UL.
        Quad {
            ll: p[0],
            lr: p[1],
            ur: p[2],
            ul: p[3],
        },
        // Clockwise: UL, UR, LR, LL.
        Quad {
            ul: p[0],
            ur: p[1],
            lr: p[2],
            ll: p[3],
        },
        // Bottom edge first: LL, LR, UL, UR.
        Quad {
            ll: p[0],
            lr: p[1],
            ul: p[2],
            ur: p[3],
        },
        // Counter-clockwise from the upper left: UL, LL, LR, UR.
        Quad {
            ul: p[0],
            ll: p[1],
            lr: p[2],
            ur: p[3],
        },
    ];
    if let Some(found) = candidates.into_iter().find(is_acrobat_order) {
        return Some(found);
    }
    // As shown on screen: sort the corners by view position.
    let mut corners: Vec<_> = p.iter().map(|c| (geometry.user_to_view(*c), *c)).collect();
    corners.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    let (top, bottom) = corners.split_at(2);
    let by_x = |v: &[(crate::geometry::Point, crate::geometry::Point)]| {
        let mut v = v.to_vec();
        v.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        (v[0].1, v[1].1)
    };
    let (ul, ur) = by_x(top);
    let (ll, lr) = by_x(bottom);
    let found = Quad { ul, ur, ll, lr };
    is_acrobat_order(&found).then_some(found)
}

/// Bounds of `/Rect` of an annotation (for tests and the CLI).
pub fn rect_of(obj: &PdfObject) -> Result<Option<Rect>> {
    match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{PageBoxes, Point};

    fn geometry(rotate: i32) -> PageGeometry {
        PageGeometry::new(&PageBoxes {
            media_box: Rect::new(0.0, 0.0, 612.0, 792.0),
            crop_box: None,
            rotate,
            user_unit: 1.0,
        })
    }

    fn quad(v: [f64; 8]) -> Quad {
        quads_from_array(&v).unwrap()[0]
    }

    const RIGHT: Quad = Quad {
        ul: Point::new(72.0, 692.0),
        ur: Point::new(200.0, 692.0),
        ll: Point::new(72.0, 680.0),
        lr: Point::new(200.0, 680.0),
    };

    #[test]
    fn reorders_other_apps_quad_orders() {
        let g = geometry(0);
        // Spec counter-clockwise: LL, LR, UR, UL.
        let ccw = quad([72.0, 680.0, 200.0, 680.0, 200.0, 692.0, 72.0, 692.0]);
        // Clockwise from the upper left: UL, UR, LR, LL.
        let cw = quad([72.0, 692.0, 200.0, 692.0, 200.0, 680.0, 72.0, 680.0]);
        // Bottom edge first: LL, LR, UL, UR.
        let bottom = quad([72.0, 680.0, 200.0, 680.0, 72.0, 692.0, 200.0, 692.0]);
        for q in [ccw, cw, bottom] {
            assert!(!is_acrobat_order(&q));
            assert_eq!(reorder(&q, &g), Some(RIGHT), "{q:?}");
        }
    }

    #[test]
    fn falls_back_to_the_corners_as_shown() {
        // A scrambled order none of the conventions explains: UR, LL, UL, LR.
        let q = quad([200.0, 692.0, 72.0, 680.0, 72.0, 692.0, 200.0, 680.0]);
        assert_eq!(reorder(&q, &geometry(0)), Some(RIGHT));
        // On a page shown turned by 90 degrees, "up" on screen is -x in user space, and
        // text runs along +y.
        let r = reorder(&q, &geometry(90)).unwrap();
        assert!(is_acrobat_order(&r));
        assert_eq!(r.ul, Point::new(72.0, 680.0));
        assert_eq!(r.lr, Point::new(200.0, 692.0));
    }
}
