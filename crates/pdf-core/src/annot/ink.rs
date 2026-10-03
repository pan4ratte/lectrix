//! Freehand strokes are simplified before they are written (AGENTS.md section 5.1
//! rule 9), with the Ramer–Douglas–Peucker algorithm.

use crate::geometry::Point;

/// The points of `stroke` that keep it within `tolerance` of the original. The first and
/// last points always stay; a stroke of one point stays one point (a dot).
pub fn simplify(stroke: &[Point], tolerance: f64) -> Vec<Point> {
    if stroke.len() < 3 {
        return stroke.to_vec();
    }
    let mut keep = vec![false; stroke.len()];
    keep[0] = true;
    keep[stroke.len() - 1] = true;
    // An explicit stack instead of recursion: a long stroke must not overflow it.
    let mut ranges = vec![(0, stroke.len() - 1)];
    while let Some((first, last)) = ranges.pop() {
        let mut worst = 0.0;
        let mut at = first;
        for i in first + 1..last {
            let d = distance_to_segment(stroke[i], stroke[first], stroke[last]);
            if d > worst {
                worst = d;
                at = i;
            }
        }
        if worst > tolerance {
            keep[at] = true;
            ranges.push((first, at));
            ranges.push((at, last));
        }
    }
    stroke
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f64, f64)]) -> Vec<Point> {
        v.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn straight_lines_keep_their_ends() {
        let line: Vec<Point> = (0..100).map(|i| Point::new(f64::from(i), 5.0)).collect();
        assert_eq!(simplify(&line, 0.5), pts(&[(0.0, 5.0), (99.0, 5.0)]));
    }

    #[test]
    fn corners_stay_and_jitter_goes() {
        let mut stroke = Vec::new();
        for i in 0..=50 {
            let jitter = if i % 2 == 0 { 0.2 } else { -0.2 };
            stroke.push(Point::new(f64::from(i), jitter));
        }
        for i in 1..=50 {
            stroke.push(Point::new(50.0, f64::from(i)));
        }
        let out = simplify(&stroke, 0.5);
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!(out[1], Point::new(50.0, 0.2));
        // Every original point is within the tolerance of the simplified stroke.
        for p in &stroke {
            let d = out
                .windows(2)
                .map(|w| distance_to_segment(*p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            assert!(d <= 0.5, "{p:?} is {d} away");
        }
    }

    #[test]
    fn dots_and_short_strokes_stay() {
        assert_eq!(simplify(&pts(&[(1.0, 1.0)]), 0.5).len(), 1);
        assert_eq!(simplify(&pts(&[(1.0, 1.0), (1.2, 1.0)]), 0.5).len(), 2);
        // A closed loop (first point = last point) keeps its far side.
        let loop_ = pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 0.0)]);
        assert_eq!(simplify(&loop_, 0.5).len(), 4);
    }
}
