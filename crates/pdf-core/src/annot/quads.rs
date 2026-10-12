//! QuadPoints, written in the order Acrobat uses (AGENTS.md section 5.1 rule 3).
//!
//! Each quad is written as upper-left, upper-right, lower-left, lower-right, in PDF user
//! space. "Upper" and "left" are relative to the text direction, not the page axes, so a
//! quad taken from vertical or rotated text keeps its reading orientation. This is the de
//! facto order (and the one MuPDF's appearance synthesis reads), not the counter-clockwise
//! order the spec text describes.
//!
//! [`quad_points_array`] is the only function that produces a `/QuadPoints` array.

use crate::geometry::{PageGeometry, Point, Rect};

/// A quadrilateral with corners named relative to the text direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub ul: Point,
    pub ur: Point,
    pub ll: Point,
    pub lr: Point,
}

impl Quad {
    /// The quad of an axis-aligned rectangle given in view space (y down).
    pub fn from_view_rect(r: Rect) -> Quad {
        let r = r.normalized();
        Quad {
            ul: Point::new(r.x0, r.y0),
            ur: Point::new(r.x1, r.y0),
            ll: Point::new(r.x0, r.y1),
            lr: Point::new(r.x1, r.y1),
        }
    }

    /// Maps a view-space quad to PDF user space, keeping corner names.
    pub fn view_to_user(&self, geometry: &PageGeometry) -> Quad {
        Quad {
            ul: geometry.view_to_user(self.ul),
            ur: geometry.view_to_user(self.ur),
            ll: geometry.view_to_user(self.ll),
            lr: geometry.view_to_user(self.lr),
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect::bounding([self.ul, self.ur, self.ll, self.lr])
    }

    /// True if the corners are finite and span a non-zero area.
    pub fn is_valid(&self) -> bool {
        let pts = [self.ul, self.ur, self.ll, self.lr];
        pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) && !self.bounds().is_empty()
    }
}

impl From<&mupdf::Quad> for Quad {
    fn from(q: &mupdf::Quad) -> Quad {
        let p = |p: mupdf::Point| Point::new(f64::from(p.x), f64::from(p.y));
        Quad {
            ul: p(q.ul),
            ur: p(q.ur),
            ll: p(q.ll),
            lr: p(q.lr),
        }
    }
}

/// Flattens user-space quads into a `/QuadPoints` array: x1 y1 … x4 y4 per quad in the
/// order UL, UR, LL, LR.
pub fn quad_points_array(quads: &[Quad]) -> Vec<f64> {
    let mut out = Vec::with_capacity(quads.len() * 8);
    for q in quads {
        for p in [q.ul, q.ur, q.ll, q.lr] {
            out.extend([p.x, p.y]);
        }
    }
    out
}

/// Reads a `/QuadPoints` array written in Acrobat order. Returns `None` if the length is
/// not a positive multiple of 8.
pub fn quads_from_array(values: &[f64]) -> Option<Vec<Quad>> {
    if values.is_empty() || !values.len().is_multiple_of(8) {
        return None;
    }
    Some(
        values
            .as_chunks::<8>()
            .0
            .iter()
            .map(|[x1, y1, x2, y2, x3, y3, x4, y4]| Quad {
                ul: Point::new(*x1, *y1),
                ur: Point::new(*x2, *y2),
                ll: Point::new(*x3, *y3),
                lr: Point::new(*x4, *y4),
            })
            .collect(),
    )
}

/// True if a user-space quad is in Acrobat order: the top edge (UL to UR) runs in the same
/// direction as the baseline (LL to LR), and the top edge lies on the "up" side of the
/// baseline in the text's own frame. Vector tests, so this holds for any text direction.
pub fn is_acrobat_order(q: &Quad) -> bool {
    let (bx, by) = (q.lr.x - q.ll.x, q.lr.y - q.ll.y);
    let (tx, ty) = (q.ur.x - q.ul.x, q.ur.y - q.ul.y);
    let same_direction = bx * tx + by * ty > 0.0;
    // In PDF user space (y up) "up" is counter-clockwise from the baseline: cross > 0.
    let side = |p: Point| bx * (p.y - q.ll.y) - by * (p.x - q.ll.x);
    same_direction && side(q.ul) > 0.0 && side(q.ur) > 0.0
}

/// The same four corners in Acrobat's order, or `None` when they have none (corners in
/// one line). Other apps' orders are tried first: the spec's counter-clockwise order from
/// the lower left, clockwise from the upper left, the bottom edge first, and
/// counter-clockwise from the upper left. In all of them the first two corners run along
/// the text, so the text's direction stays the one the writer meant. Failing those, the
/// corners are taken as they appear on screen (upper left is the top-left on the page as
/// shown).
///
/// Text that would then stand on its head on the page as shown is taken to be upright
/// instead: a quad written from right to left (upper right, upper left, lower right,
/// lower left, as Lectrix 1.0 wrote for right-to-left text) reads as upside-down text
/// with its bottom edge first, and an underline redrawn from that would run above the
/// line.
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
        let (top, bottom) = (
            geometry.user_to_view(found.ul),
            geometry.user_to_view(found.ll),
        );
        // View space has y down: "up" pointing down the screen is text on its head.
        let (across, down) = (top.x - bottom.x, top.y - bottom.y);
        if down > across.abs() {
            return Some(Quad {
                ul: found.lr,
                ur: found.ll,
                ll: found.ur,
                lr: found.ul,
            });
        }
        return Some(found);
    }
    // As shown on screen: sort the corners by view position.
    let mut corners: Vec<_> = p.iter().map(|c| (geometry.user_to_view(*c), *c)).collect();
    corners.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    let (top, bottom) = corners.split_at(2);
    let by_x = |v: &[(Point, Point)]| {
        let mut v = v.to_vec();
        v.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        (v[0].1, v[1].1)
    };
    let (ul, ur) = by_x(top);
    let (ll, lr) = by_x(bottom);
    let found = Quad { ul, ur, ll, lr };
    is_acrobat_order(&found).then_some(found)
}

/// The quad in Acrobat's order, if it is not in it and its corners can be put in it. A
/// quad of no area, or with its corners in one line, has no order to put right.
pub fn out_of_order(q: &Quad, geometry: &PageGeometry) -> Option<Quad> {
    if !q.is_valid() || is_acrobat_order(q) {
        return None;
    }
    reorder(q, geometry)
}

/// True if the quad is the mirror image of one in Acrobat's order: both edges along the
/// text run the same way, with the upper and the lower one swapped ("bottom edge first":
/// LL, LR, UL, UR, as apps built on Apple's PDFKit write) or each written from right to
/// left (UR, UL, LR, LL). Read in Acrobat's order it is still the same four-sided shape,
/// not a bow-tie, so readers that draw from the corners fill the same area.
pub fn is_mirrored(q: &Quad) -> bool {
    !is_acrobat_order(q)
        && is_acrobat_order(&Quad {
            ul: q.ll,
            ur: q.lr,
            ll: q.ul,
            lr: q.ur,
        })
}

/// The quad in Acrobat's order, if its corners are crossed: out of order in a way that is
/// no mirror image (the spec's counter-clockwise order, a clockwise one), so that readers
/// which take them in Acrobat's order draw a bow-tie. This is what the list reports as
/// corners in the wrong order, and what repair rewrites in an annotation that keeps its
/// appearance: a mirrored quad is left as its app wrote it (ADR 0017).
pub fn crossed(q: &Quad, geometry: &PageGeometry) -> Option<Quad> {
    if is_mirrored(q) {
        return None;
    }
    out_of_order(q, geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{PageBoxes, PageGeometry};

    fn geometry(rotate: i32) -> PageGeometry {
        PageGeometry::new(&PageBoxes {
            media_box: Rect::new(0.0, 0.0, 612.0, 792.0),
            crop_box: None,
            rotate,
            user_unit: 1.0,
        })
    }

    #[test]
    fn writes_ul_ur_ll_lr() {
        let q =
            Quad::from_view_rect(Rect::new(72.0, 100.0, 200.0, 112.0)).view_to_user(&geometry(0));
        assert_eq!(
            quad_points_array(&[q]),
            [72.0, 692.0, 200.0, 692.0, 72.0, 680.0, 200.0, 680.0]
        );
        assert!(is_acrobat_order(&q));
    }

    #[test]
    fn view_quads_map_to_acrobat_order_for_every_rotation() {
        for rotate in [0, 90, 180, 270] {
            let g = geometry(rotate);
            let q = Quad::from_view_rect(Rect::new(50.0, 60.0, 150.0, 72.0)).view_to_user(&g);
            assert!(is_acrobat_order(&q), "rotation {rotate}: {q:?}");
        }
    }

    #[test]
    fn detects_counter_clockwise_spec_order() {
        // Spec-text order (LL, LR, UR, UL) misread as UL, UR, LL, LR.
        let wrong =
            quads_from_array(&[72.0, 680.0, 200.0, 680.0, 200.0, 692.0, 72.0, 692.0]).unwrap();
        assert!(!is_acrobat_order(&wrong[0]));
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
            assert_eq!(out_of_order(&q, &g), Some(RIGHT), "{q:?}");
        }
        assert_eq!(out_of_order(&RIGHT, &g), None);
        assert_eq!(crossed(&RIGHT, &g), None);
    }

    #[test]
    fn mirrored_corners_are_not_crossed() {
        let g = geometry(0);
        // Bottom edge first (PDFKit), and each edge from right to left (Lectrix 1.0 on
        // right-to-left text): mirror images of Acrobat's order.
        let bottom = quad([72.0, 680.0, 200.0, 680.0, 72.0, 692.0, 200.0, 692.0]);
        let leftwards = quad([200.0, 692.0, 72.0, 692.0, 200.0, 680.0, 72.0, 680.0]);
        for q in [bottom, leftwards] {
            assert!(is_mirrored(&q), "{q:?}");
            assert_eq!(crossed(&q, &g), None, "{q:?}");
            assert_eq!(out_of_order(&q, &g), Some(RIGHT), "{q:?}");
        }
        // The spec's order, a clockwise one, and a scramble (wide and tall) are crossed.
        let ccw = quad([72.0, 680.0, 200.0, 680.0, 200.0, 692.0, 72.0, 692.0]);
        let cw = quad([72.0, 692.0, 200.0, 692.0, 200.0, 680.0, 72.0, 680.0]);
        let scramble = quad([200.0, 692.0, 72.0, 680.0, 72.0, 692.0, 200.0, 680.0]);
        let tall = quad([80.0, 692.0, 72.0, 500.0, 72.0, 692.0, 80.0, 500.0]);
        for q in [ccw, cw, scramble, tall] {
            assert!(!is_mirrored(&q), "{q:?}");
            assert!(
                crossed(&q, &g).is_some_and(|r| is_acrobat_order(&r)),
                "{q:?}"
            );
        }
        assert!(!is_mirrored(&RIGHT));
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

    #[test]
    fn a_quad_written_from_right_to_left_stays_upright() {
        // UR, UL, LR, LL: what Lectrix 1.0 wrote for right-to-left text. Read as "bottom
        // edge first" it would be text on its head, with its baseline along the top.
        let q = quad([200.0, 692.0, 72.0, 692.0, 200.0, 680.0, 72.0, 680.0]);
        assert!(!is_acrobat_order(&q));
        assert_eq!(reorder(&q, &geometry(0)), Some(RIGHT));
        // The same text on a page shown turned by 180 degrees is upright there, where
        // user space has it on its head.
        let turned = Quad {
            ul: RIGHT.lr,
            ur: RIGHT.ll,
            ll: RIGHT.ur,
            lr: RIGHT.ul,
        };
        let bottom_first = quad([72.0, 680.0, 200.0, 680.0, 72.0, 692.0, 200.0, 692.0]);
        assert_eq!(reorder(&bottom_first, &geometry(180)), Some(turned));
        // Text that runs up or down the page as shown keeps the direction it was given.
        // (The spec's order: LL, LR, UR, UL.)
        let up = quad([112.0, 100.0, 112.0, 300.0, 100.0, 300.0, 100.0, 100.0]);
        let r = reorder(&up, &geometry(0)).unwrap();
        assert!(is_acrobat_order(&r));
        assert_eq!(
            (r.ll, r.lr),
            (Point::new(112.0, 100.0), Point::new(112.0, 300.0))
        );
    }

    #[test]
    fn corners_in_one_line_have_no_order() {
        // Not empty by its bounds, but no area: nothing to report, nothing to rewrite.
        let line = quad([0.0, 0.0, 10.0, 10.0, 0.0, 0.0, 10.0, 10.0]);
        assert!(line.is_valid() && !is_acrobat_order(&line));
        assert_eq!(reorder(&line, &geometry(0)), None);
        assert_eq!(out_of_order(&line, &geometry(0)), None);
    }

    #[test]
    fn round_trips_arrays() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let quads = quads_from_array(&values).unwrap();
        assert_eq!(quad_points_array(&quads), values);
        assert!(quads_from_array(&values[..7]).is_none());
        assert!(quads_from_array(&[]).is_none());
    }
}
