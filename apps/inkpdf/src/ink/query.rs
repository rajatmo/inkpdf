//! What can be asked of the strokes without changing them: which is under a point, which an eraser's segment touches, which a lasso
//! encloses, and the box round some of them.

use std::collections::HashSet;

use super::{P, Stroke};

/// The topmost stroke within `radius` of `p`.
#[cfg(test)]
pub fn hit_test(strokes: &[Stroke], p: P, radius: f32) -> Option<u64> {
    strokes.iter().rev().find(|s| s.touches_circle(p, radius)).map(|s| s.id)
}

/// Strokes the eraser touches moving from `a` to `b` with the given radius.
pub fn touched_by_segment(strokes: &[Stroke], a: P, b: P, radius: f32) -> Vec<u64> {
    strokes.iter().filter(|s| s.touches_segment(a, b, radius)).map(|s| s.id).collect()
}

/// Strokes mostly (at least half their points) inside the lasso.
pub fn in_polygon(strokes: &[Stroke], poly: &[P]) -> Vec<u64> {
    strokes.iter().filter(|s| s.fraction_inside(poly) >= 0.5).map(|s| s.id).collect()
}

/// The bounding box of the strokes with these ids.
pub fn bounds_of(strokes: &[Stroke], ids: &[u64]) -> Option<[f32; 4]> {
    let wanted: HashSet<u64> = ids.iter().copied().collect();
    let mut out: Option<[f32; 4]> = None;
    for s in strokes.iter().filter(|s| wanted.contains(&s.id)) {
        let b = s.bbox();
        out = Some(out.map_or(b, |o| [o[0].min(b[0]), o[1].min(b[1]), o[2].max(b[2]), o[3].max(b[3])]));
    }
    out
}
