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

    #[test]
    fn round_trips_arrays() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let quads = quads_from_array(&values).unwrap();
        assert_eq!(quad_points_array(&quads), values);
        assert!(quads_from_array(&values[..7]).is_none());
        assert!(quads_from_array(&[]).is_none());
    }
}
