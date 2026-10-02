//! Conversions between view space and PDF user space.
//!
//! **View space** is what the frontend draws in: origin at the top-left of the visible
//! (cropped, rotated) page, y pointing down, units of 1/72 inch at zoom 1. It equals
//! MuPDF's "page space".
//!
//! **PDF user space** is the coordinate system of the page's content stream and of
//! annotation keys like `/Rect` and `/QuadPoints`: arbitrary origin, y pointing up, units
//! of `/UserUnit` / 72 inch.
//!
//! Every conversion in Folio goes through this module (AGENTS.md section 5.1 rule 8). The
//! transform mirrors MuPDF's `pdf_page_obj_transform_box`, and the integration tests
//! check it against MuPDF's own page matrix for every rotation, an offset CropBox and a
//! non-default UserUnit.

/// A 2-D affine transform in MuPDF's row-vector convention:
/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn scale(sx: f64, sy: f64) -> Self {
        Affine {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    pub fn translate(tx: f64, ty: f64) -> Self {
        Affine {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// Rotation by a multiple of 90 degrees, computed exactly (no trigonometry rounding).
    pub fn rotate_quarter_turns(degrees: i32) -> Self {
        let (cos, sin) = match degrees.rem_euclid(360) {
            0 => (1.0, 0.0),
            90 => (0.0, 1.0),
            180 => (-1.0, 0.0),
            270 => (0.0, -1.0),
            other => {
                let r = f64::from(other).to_radians();
                (r.cos(), r.sin())
            }
        };
        Affine {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self` followed by `then`.
    pub fn then(&self, then: &Affine) -> Affine {
        Affine {
            a: self.a * then.a + self.b * then.c,
            b: self.a * then.b + self.b * then.d,
            c: self.c * then.a + self.d * then.c,
            d: self.c * then.b + self.d * then.d,
            e: self.e * then.a + self.f * then.c + then.e,
            f: self.e * then.b + self.f * then.d + then.f,
        }
    }

    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < f64::EPSILON {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(self.e * a + self.f * c),
            f: -(self.e * b + self.f * d),
        })
    }

    pub fn apply(&self, p: Point) -> Point {
        Point {
            x: self.a * p.x + self.c * p.y + self.e,
            y: self.b * p.x + self.d * p.y + self.f,
        }
    }

    pub fn apply_rect(&self, r: Rect) -> Rect {
        Rect::bounding([
            self.apply(Point::new(r.x0, r.y0)),
            self.apply(Point::new(r.x1, r.y0)),
            self.apply(Point::new(r.x0, r.y1)),
            self.apply(Point::new(r.x1, r.y1)),
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
}

/// An axis-aligned rectangle with `x0 <= x1` and `y0 <= y1` once normalized.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Rect { x0, y0, x1, y1 }
    }

    /// Orders the corners so that `x0 <= x1` and `y0 <= y1` (PDF rectangles may be given
    /// in any corner order).
    pub fn normalized(self) -> Rect {
        Rect {
            x0: self.x0.min(self.x1),
            y0: self.y0.min(self.y1),
            x1: self.x0.max(self.x1),
            y1: self.y0.max(self.y1),
        }
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub fn bounding(points: impl IntoIterator<Item = Point>) -> Rect {
        let mut r = Rect::new(
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in points {
            r.x0 = r.x0.min(p.x);
            r.y0 = r.y0.min(p.y);
            r.x1 = r.x1.max(p.x);
            r.y1 = r.y1.max(p.y);
        }
        r
    }

    pub fn union(&self, other: &Rect) -> Rect {
        Rect::new(
            self.x0.min(other.x0),
            self.y0.min(other.y0),
            self.x1.max(other.x1),
            self.y1.max(other.y1),
        )
    }

    pub fn intersect(&self, other: &Rect) -> Rect {
        Rect::new(
            self.x0.max(other.x0),
            self.y0.max(other.y0),
            self.x1.min(other.x1),
            self.y1.min(other.y1),
        )
    }

    pub fn expand(&self, by: f64) -> Rect {
        Rect::new(self.x0 - by, self.y0 - by, self.x1 + by, self.y1 + by)
    }
}

/// The page dictionary values that determine the view transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageBoxes {
    pub media_box: Rect,
    /// `None` when the page has no CropBox (it then defaults to the MediaBox).
    pub crop_box: Option<Rect>,
    /// Raw `/Rotate` value (inherited); normalized by [`normalize_rotation`].
    pub rotate: i32,
    /// `/UserUnit`, default 1.
    pub user_unit: f64,
}

/// Snaps a `/Rotate` value to 0, 90, 180 or 270, as MuPDF does.
pub fn normalize_rotation(rotate: i32) -> i32 {
    let r = rotate.rem_euclid(360);
    let snapped = 90 * ((r + 45) / 90);
    if snapped >= 360 { 0 } else { snapped }
}

/// Transform between view space and PDF user space for one page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    /// PDF user space to view space.
    pub to_view: Affine,
    /// View space to PDF user space.
    pub to_user: Affine,
    /// Size of the visible page in view units (points at zoom 1).
    pub width: f64,
    pub height: f64,
    /// Normalized rotation (0, 90, 180 or 270).
    pub rotation: i32,
    /// The box that is visible (CropBox clipped to MediaBox), in user space.
    pub visible_box: Rect,
}

impl PageGeometry {
    pub fn new(boxes: &PageBoxes) -> PageGeometry {
        let media = boxes.media_box.normalized();
        let visible = match boxes.crop_box {
            Some(crop) => {
                let clipped = crop.normalized().intersect(&media);
                if clipped.is_empty() { media } else { clipped }
            }
            None => media,
        };
        let unit = if boxes.user_unit.is_finite() && boxes.user_unit > 0.0 {
            boxes.user_unit
        } else {
            1.0
        };
        let rotation = normalize_rotation(boxes.rotate);

        // Same order as MuPDF: rotate by -rotation, then scale by UserUnit and flip y,
        // then move the visible box's top-left corner to the origin.
        let base = Affine::rotate_quarter_turns(-rotation).then(&Affine::scale(unit, -unit));
        let moved = base.apply_rect(visible);
        let to_view = base.then(&Affine::translate(-moved.x0, -moved.y0));
        // A product of a rotation, a non-zero scale and a translation is always invertible.
        let to_user = to_view.invert().unwrap_or(Affine::IDENTITY);

        PageGeometry {
            to_view,
            to_user,
            width: moved.width(),
            height: moved.height(),
            rotation,
            visible_box: visible,
        }
    }

    pub fn view_to_user(&self, p: Point) -> Point {
        self.to_user.apply(p)
    }

    pub fn user_to_view(&self, p: Point) -> Point {
        self.to_view.apply(p)
    }

    pub fn view_rect_to_user(&self, r: Rect) -> Rect {
        self.to_user.apply_rect(r)
    }

    pub fn user_rect_to_view(&self, r: Rect) -> Rect {
        self.to_view.apply_rect(r)
    }
}

/// Reads the page dictionary values that determine the geometry (inherited where the
/// format allows). Falls back to US Letter for a missing or broken MediaBox, as MuPDF does.
pub fn read_page_boxes(page: &mupdf::pdf::PdfObject) -> crate::Result<PageBoxes> {
    use crate::objects;
    let media_box = match page.get_dict_inheritable("MediaBox")? {
        Some(obj) => objects::rect(&obj)?.filter(|r| !r.is_empty()),
        None => None,
    }
    .unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0));
    let crop_box = match page.get_dict_inheritable("CropBox")? {
        Some(obj) => objects::rect(&obj)?,
        None => None,
    };
    let rotate = match page.get_dict_inheritable("Rotate")? {
        Some(obj) if obj.is_number()? => obj.as_int()?,
        _ => 0,
    };
    let user_unit = match page.get_dict("UserUnit")? {
        Some(obj) => objects::number(&obj)?.unwrap_or(1.0),
        None => 1.0,
    };
    Ok(PageBoxes {
        media_box,
        crop_box,
        rotate,
        user_unit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: Rect = Rect::new(0.0, 0.0, 612.0, 792.0);

    fn geom(rotate: i32, crop: Option<Rect>, unit: f64) -> PageGeometry {
        PageGeometry::new(&PageBoxes {
            media_box: LETTER,
            crop_box: crop,
            rotate,
            user_unit: unit,
        })
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn rotation_snaps_like_mupdf() {
        assert_eq!(normalize_rotation(0), 0);
        assert_eq!(normalize_rotation(90), 90);
        assert_eq!(normalize_rotation(-90), 270);
        assert_eq!(normalize_rotation(450), 90);
        assert_eq!(normalize_rotation(44), 0);
        assert_eq!(normalize_rotation(46), 90);
        assert_eq!(normalize_rotation(359), 0);
    }

    #[test]
    fn unrotated_flips_y() {
        let g = geom(0, None, 1.0);
        assert_eq!((g.width, g.height), (612.0, 792.0));
        // User-space top-left is view origin.
        assert!(close(
            g.user_to_view(Point::new(0.0, 792.0)),
            Point::new(0.0, 0.0)
        ));
        assert!(close(
            g.user_to_view(Point::new(612.0, 0.0)),
            Point::new(612.0, 792.0)
        ));
    }

    #[test]
    fn rotate_90_shows_user_top_left_at_view_top_right() {
        let g = geom(90, None, 1.0);
        assert_eq!((g.width, g.height), (792.0, 612.0));
        assert!(close(
            g.user_to_view(Point::new(0.0, 792.0)),
            Point::new(792.0, 0.0)
        ));
        assert!(close(
            g.user_to_view(Point::new(0.0, 0.0)),
            Point::new(0.0, 0.0)
        ));
    }

    #[test]
    fn rotate_180() {
        let g = geom(180, None, 1.0);
        assert_eq!((g.width, g.height), (612.0, 792.0));
        assert!(close(
            g.user_to_view(Point::new(0.0, 0.0)),
            Point::new(612.0, 0.0)
        ));
        assert!(close(
            g.user_to_view(Point::new(612.0, 792.0)),
            Point::new(0.0, 792.0)
        ));
    }

    #[test]
    fn rotate_270() {
        let g = geom(270, None, 1.0);
        assert_eq!((g.width, g.height), (792.0, 612.0));
        assert!(close(
            g.user_to_view(Point::new(612.0, 792.0)),
            Point::new(0.0, 0.0)
        ));
        assert!(close(
            g.user_to_view(Point::new(0.0, 792.0)),
            Point::new(0.0, 612.0)
        ));
    }

    #[test]
    fn crop_box_with_offset_origin() {
        let g = geom(0, Some(Rect::new(100.0, 50.0, 400.0, 650.0)), 1.0);
        assert_eq!((g.width, g.height), (300.0, 600.0));
        assert!(close(
            g.user_to_view(Point::new(100.0, 650.0)),
            Point::new(0.0, 0.0)
        ));
        assert!(close(
            g.user_to_view(Point::new(400.0, 50.0)),
            Point::new(300.0, 600.0)
        ));
    }

    #[test]
    fn crop_box_with_offset_and_rotation() {
        let g = geom(90, Some(Rect::new(100.0, 50.0, 400.0, 650.0)), 1.0);
        assert_eq!((g.width, g.height), (600.0, 300.0));
        assert!(close(
            g.user_to_view(Point::new(100.0, 50.0)),
            Point::new(0.0, 0.0)
        ));
        assert!(close(
            g.user_to_view(Point::new(100.0, 650.0)),
            Point::new(600.0, 0.0)
        ));
    }

    #[test]
    fn crop_box_is_clipped_to_media_box() {
        let g = geom(0, Some(Rect::new(-50.0, -50.0, 300.0, 300.0)), 1.0);
        assert_eq!(g.visible_box, Rect::new(0.0, 0.0, 300.0, 300.0));
    }

    #[test]
    fn user_unit_scales_view() {
        let g = geom(0, None, 2.0);
        assert_eq!((g.width, g.height), (1224.0, 1584.0));
        assert!(close(
            g.user_to_view(Point::new(612.0, 0.0)),
            Point::new(1224.0, 1584.0)
        ));
    }

    #[test]
    fn round_trip_every_case() {
        for rotate in [0, 90, 180, 270] {
            for crop in [None, Some(Rect::new(30.0, 40.0, 500.0, 700.0))] {
                for unit in [1.0, 2.5] {
                    let g = geom(rotate, crop, unit);
                    let p = Point::new(123.25, 456.5);
                    assert!(
                        close(g.view_to_user(g.user_to_view(p)), p),
                        "{rotate} {crop:?} {unit}"
                    );
                }
            }
        }
    }
}
