//! Plane geometry for ink: distances, polygon tests, and thinning a point list.

use super::P;

/// Whether `p` is inside `rect` (`[x0, y0, x1, y1]`), edges included.
pub fn rect_contains(rect: [f32; 4], p: P) -> bool {
    p[0] >= rect[0] && p[0] <= rect[2] && p[1] >= rect[1] && p[1] <= rect[3]
}

pub fn dist2(a: P, b: P) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

pub fn dist2_point_segment(p: P, a: P, b: P) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if len2 <= f32::EPSILON {
        return dist2(p, a);
    }
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
    dist2(p, [a[0] + ab[0] * t, a[1] + ab[1] * t])
}

pub(super) fn cross(o: P, a: P, b: P) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

pub(super) fn segments_intersect(a: P, b: P, c: P, d: P) -> bool {
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0) && d1 != 0.0 && d2 != 0.0) && ((d3 > 0.0) != (d4 > 0.0) && d3 != 0.0 && d4 != 0.0)
}

pub(super) fn dist2_segments(a: P, b: P, c: P, d: P) -> f32 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    dist2_point_segment(a, c, d).min(dist2_point_segment(b, c, d)).min(dist2_point_segment(c, a, b)).min(dist2_point_segment(d, a, b))
}

pub fn point_in_polygon(p: P, poly: &[P]) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Ramer–Douglas–Peucker: drop points within `tolerance` of the line between kept neighbours.
/// Ends are always kept. Iterative, so a long stroke cannot overflow the stack.
pub fn simplify(pts: &[P], tolerance: f32) -> Vec<P> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let tol2 = tolerance * tolerance;
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((lo, hi)) = stack.pop() {
        let (mut worst, mut at) = (0.0_f32, lo);
        for i in lo + 1..hi {
            let d = dist2_point_segment(pts[i], pts[lo], pts[hi]);
            if d > worst {
                (worst, at) = (d, i);
            }
        }
        if worst > tol2 {
            keep[at] = true;
            stack.push((lo, at));
            stack.push((at, hi));
        }
    }
    pts.iter().zip(&keep).filter(|(_, k)| **k).map(|(p, _)| *p).collect()
}

/// Drop points closer than `min_dist` to the previous kept one (the last point is kept).
pub fn thin(pts: &[P], min_dist: f32) -> Vec<P> {
    let mut out: Vec<P> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|&q| dist2(p, q) >= min_dist * min_dist) {
            out.push(p);
        }
    }
    if let (Some(&last), Some(&kept)) = (pts.last(), out.last())
        && last != kept
    {
        out.push(last);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_keeps_ends_and_corners_and_drops_collinear_points() {
        let pts: Vec<P> = (0..=10).map(|i| [i as f32, 0.0]).collect();
        assert_eq!(simplify(&pts, 0.1), vec![[0.0, 0.0], [10.0, 0.0]]);
        let l: Vec<P> = vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [10.0, 5.0], [10.0, 10.0]];
        assert_eq!(simplify(&l, 0.1), vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(simplify(&[[1.0, 1.0]], 1.0), vec![[1.0, 1.0]]);
        assert!(simplify(&[], 1.0).is_empty());
    }

    #[test]
    fn simplify_survives_a_huge_stroke() {
        let pts: Vec<P> = (0..200_000).map(|i| [i as f32, ((i as f32) * 0.37).sin()]).collect();
        let out = simplify(&pts, 0.05);
        assert!(out.len() > 2 && out.len() <= pts.len());
    }

    #[test]
    fn thin_drops_near_duplicates_but_keeps_the_last_point() {
        let pts = vec![[0.0, 0.0], [0.1, 0.0], [0.2, 0.0], [5.0, 0.0], [5.05, 0.0]];
        let t = thin(&pts, 1.0);
        assert_eq!(t, vec![[0.0, 0.0], [5.0, 0.0], [5.05, 0.0]]);
    }

    #[test]
    fn point_segment_distance() {
        assert_eq!(dist2_point_segment([5.0, 3.0], [0.0, 0.0], [10.0, 0.0]), 9.0);
        assert_eq!(dist2_point_segment([-4.0, 3.0], [0.0, 0.0], [10.0, 0.0]), 25.0);
        // A zero-length segment is a point.
        assert_eq!(dist2_point_segment([3.0, 4.0], [0.0, 0.0], [0.0, 0.0]), 25.0);
    }
}
