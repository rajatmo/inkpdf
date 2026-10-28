//! Stroke smoothing, in two parts that work together:
//!
//! * [`PenFilter`] steadies the pen as it moves: a one-euro filter, which smooths a slow, shaky
//!   hand a lot and a fast flick hardly at all, so it removes jitter without adding lag you can
//!   feel.
//! * [`curve_through`] turns the stored points into a smooth curve for drawing, so strokes stay
//!   round when zoomed in, while real corners stay sharp.
//!
//! Pure logic with no window or GPU types, so it is unit-tested.

use std::f32::consts::TAU;

use crate::plane::P;

/// How much steadying the pen gets by default, from 0 (none) to 1 (a lot).
pub const DEFAULT_AMOUNT: f32 = 0.5;

/// Turns sharper than this are corners: the curve runs straight into them instead of rounding.
const CORNER_DEGREES: f32 = 55.0;
/// Most pieces one segment of the curve is cut into.
const MAX_SUBDIVISIONS: usize = 24;

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn low_pass_alpha(dt: f32, cutoff_hz: f32) -> f32 {
    let tau = 1.0 / (TAU * cutoff_hz.max(1e-3));
    1.0 / (1.0 + tau / dt)
}

#[derive(Clone, Copy, Debug)]
struct Last {
    /// The filtered position and its smoothed speed (pixels per second).
    pos: P,
    velocity: P,
    t_ms: f64,
}

/// A one-euro filter over a stream of pen positions in device pixels.
///
/// The cutoff frequency rises with the pen's speed: slow and shaky gets a low cutoff (smooth),
/// fast gets a high one (responsive).
#[derive(Clone, Debug)]
pub struct PenFilter {
    /// Below this amount the filter does nothing.
    enabled: bool,
    min_cutoff: f32,
    speed_gain: f32,
    last: Option<Last>,
}

/// Smoothing of the pen's speed estimate, in hertz.
const SPEED_CUTOFF_HZ: f32 = 3.0;
/// Below this amount the pen is passed through untouched.
const OFF_BELOW: f32 = 0.01;

/// Interpolate between `a` and `b` on a log scale, which suits quantities spanning decades.
fn geometric(a: f32, b: f32, t: f32) -> f32 {
    a * (b / a).powf(t)
}

impl PenFilter {
    /// `amount` runs from 0 (the pen is passed through unchanged) to 1 (strong).
    pub fn new(amount: f32) -> Self {
        let a = amount.clamp(0.0, 1.0);
        // The cutoff when the pen is still: 30 Hz is barely a filter, 0.25 Hz a heavy one. The
        // gain says how fast the cutoff opens up as the pen speeds up (so flicks are not dragged).
        Self { enabled: a >= OFF_BELOW, min_cutoff: geometric(30.0, 0.25, a), speed_gain: geometric(0.12, 0.004, a), last: None }
    }

    /// The steadied position for a raw sample taken at `t_ms`.
    pub fn filter(&mut self, raw: P, t_ms: f64) -> P {
        if !raw[0].is_finite() || !raw[1].is_finite() {
            return self.last.map_or(raw, |l| l.pos);
        }
        if !self.enabled {
            return raw;
        }
        let Some(last) = self.last else {
            self.last = Some(Last { pos: raw, velocity: [0.0; 2], t_ms });
            return raw;
        };
        // Samples can share a timestamp or arrive in a burst: keep the step in a sane range.
        let dt = (((t_ms - last.t_ms) / 1000.0) as f32).clamp(0.001, 0.1);
        let speed_alpha = low_pass_alpha(dt, SPEED_CUTOFF_HZ);
        let raw_velocity = [(raw[0] - last.pos[0]) / dt, (raw[1] - last.pos[1]) / dt];
        let velocity = [lerp(last.velocity[0], raw_velocity[0], speed_alpha), lerp(last.velocity[1], raw_velocity[1], speed_alpha)];
        let speed = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
        let alpha = low_pass_alpha(dt, self.min_cutoff + self.speed_gain * speed);
        let pos = [lerp(last.pos[0], raw[0], alpha), lerp(last.pos[1], raw[1], alpha)];
        self.last = Some(Last { pos, velocity, t_ms });
        pos
    }
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn length(v: P) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn unit(v: P) -> Option<P> {
    let l = length(v);
    (l > 1e-6).then(|| [v[0] / l, v[1] / l])
}

/// The direction the curve passes through an interior point, or `None` at a corner or where the
/// neighbours coincide with it.
fn interior_tangent(points: &[P], i: usize) -> Option<P> {
    let before = unit(sub(points[i], points[i.checked_sub(1)?]))?;
    let after = unit(sub(*points.get(i + 1)?, points[i]))?;
    let cos_turn = (before[0] * after[0] + before[1] * after[1]).clamp(-1.0, 1.0);
    if cos_turn.acos().to_degrees() > CORNER_DEGREES { None } else { unit([before[0] + after[0], before[1] + after[1]]) }
}

/// The direction the curve leaves or arrives at `points[i]`, or `None` at a corner. At either end
/// of the stroke there is only one neighbour: the neighbour's tangent is reflected across the
/// chord, which is the direction a circular arc through the three points has there. (The chord
/// itself would be off by half the turn, bulging the ends.)
fn tangent(points: &[P], i: usize) -> Option<P> {
    let last = points.len() - 1;
    if i != 0 && i != last {
        return interior_tangent(points, i);
    }
    let neighbour = if i == 0 { 1 } else { last - 1 };
    let chord = unit(sub(points[neighbour], points[i]))?;
    match interior_tangent(points, neighbour) {
        Some(t) => {
            let along = t[0] * chord[0] + t[1] * chord[1];
            unit([2.0 * along * chord[0] - t[0], 2.0 * along * chord[1] - t[1]])
        }
        // Next to a corner the end simply runs along its one segment: towards the neighbour at the
        // start, away from it at the end (the chord points from the end back to the neighbour).
        None if i == 0 => Some(chord),
        None => Some([-chord[0], -chord[1]]),
    }
}

/// A smooth curve through `points`, as a polyline short enough to look round: pieces are about
/// `step` long. The original points are all on the curve. Corners (turns sharper than about 55°)
/// stay sharp and straight stretches stay straight. Fewer than three points are returned as they
/// are.
pub fn curve_through(points: &[P], step: f32) -> Vec<P> {
    if points.len() < 3 || !step.is_finite() || step <= 0.0 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    curve_segments(points, 0..points.len() - 1, step, &mut out);
    out
}

/// Append to `out` the curve's pieces for segments `segments` of `points` (segment `i` runs from
/// `points[i]` to `points[i + 1]`; the point it starts at is not added again). The tangents come from the
/// whole of `points`, so the pieces are exactly those [`curve_through`] makes. A segment is final once the
/// point after its end exists and is not the last: a stroke being drawn can keep what it has made and
/// make only the new segments as points arrive. (Needs at least three points; else nothing is added.)
pub fn curve_segments(points: &[P], segments: std::ops::Range<usize>, step: f32, out: &mut Vec<P>) {
    if points.len() < 3 || !step.is_finite() || step <= 0.0 {
        return;
    }
    for i in segments.start..segments.end.min(points.len() - 1) {
        let (a, b) = (points[i], points[i + 1]);
        let span = length(sub(b, a));
        let pieces = ((span / step).ceil() as usize).clamp(1, MAX_SUBDIVISIONS);
        if pieces > 1 {
            // A cubic Bézier whose handles follow the local tangents, a third of the span long.
            let handle = span / 3.0;
            let c1 = tangent(points, i).map_or(a, |t| [a[0] + t[0] * handle, a[1] + t[1] * handle]);
            let c2 = tangent(points, i + 1).map_or(b, |t| [b[0] - t[0] * handle, b[1] - t[1] * handle]);
            for k in 1..pieces {
                let t = k as f32 / pieces as f32;
                let u = 1.0 - t;
                let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                out.push([w0 * a[0] + w1 * c1[0] + w2 * c2[0] + w3 * b[0], w0 * a[1] + w1 * c1[1] + w2 * c2[1] + w3 * b[1]]);
            }
        }
        out.push(b);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A repeatable stream of noise in -1..1.
    fn noise(n: usize) -> Vec<f32> {
        let mut seed = 0x2545_F491_u32;
        (0..n)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 20_001) as f32 / 10_000.0 - 1.0
            })
            .collect()
    }

    fn variance(v: &[f32]) -> f32 {
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32
    }

    #[test]
    fn the_first_sample_passes_straight_through() {
        let mut f = PenFilter::new(0.8);
        assert_eq!(f.filter([12.0, 34.0], 1000.0), [12.0, 34.0]);
    }

    #[test]
    fn a_shaky_slow_hand_is_steadied() {
        // The pen is meant to sit on y = 300 moving slowly right, with a pixel and a half of tremor.
        let jitter = noise(400);
        let mut f = PenFilter::new(DEFAULT_AMOUNT);
        let (mut raw_y, mut out_y) = (Vec::new(), Vec::new());
        for (i, j) in jitter.iter().enumerate() {
            let raw = [100.0 + i as f32 * 0.5, 300.0 + j * 1.5];
            let p = f.filter(raw, i as f64 * 5.0);
            if i > 50 {
                raw_y.push(raw[1]);
                out_y.push(p[1]);
            }
        }
        assert!(variance(&out_y) < variance(&raw_y) * 0.25, "tremor {:.3} -> {:.3}", variance(&raw_y), variance(&out_y));
    }

    #[test]
    fn a_fast_stroke_is_not_dragged_behind() {
        // 3000 px/s across: the filter must stay close to the pen.
        let mut f = PenFilter::new(DEFAULT_AMOUNT);
        let mut worst_lag = 0.0_f32;
        for i in 0..200 {
            let raw = [i as f32 * 12.0, 400.0];
            let p = f.filter(raw, i as f64 * 4.0);
            if i > 30 {
                worst_lag = worst_lag.max(raw[0] - p[0]);
            }
        }
        assert!(worst_lag < 8.0, "lagged {worst_lag:.1} px behind a fast pen");
    }

    #[test]
    fn zero_smoothing_leaves_the_pen_alone() {
        let jitter = noise(100);
        let mut f = PenFilter::new(0.0);
        for (i, j) in jitter.iter().enumerate() {
            let raw = [i as f32 * 2.0, 100.0 + j * 2.0];
            let p = f.filter(raw, i as f64 * 5.0);
            assert!((p[0] - raw[0]).abs() < 0.3 && (p[1] - raw[1]).abs() < 0.3, "{p:?} vs {raw:?}");
        }
    }

    #[test]
    fn more_smoothing_steadies_more() {
        let jitter = noise(300);
        let run = |amount| {
            let mut f = PenFilter::new(amount);
            let ys: Vec<f32> = jitter.iter().enumerate().map(|(i, j)| f.filter([i as f32, 200.0 + j * 2.0], i as f64 * 5.0)[1]).skip(40).collect();
            variance(&ys)
        };
        assert!(run(1.0) < run(0.5) && run(0.5) < run(0.1));
    }

    #[test]
    fn the_filter_survives_odd_input() {
        let mut f = PenFilter::new(0.5);
        f.filter([0.0, 0.0], 100.0);
        // Same timestamp, going backwards in time, NaN, infinity: no panic and finite output.
        for (p, t) in [([5.0, 5.0], 100.0), ([6.0, 6.0], 50.0), ([f32::NAN, 1.0], 120.0), ([f32::INFINITY, 1.0], 130.0), ([7.0, 7.0], 1e12)] {
            let out = f.filter(p, t);
            assert!(out[0].is_finite() && out[1].is_finite(), "{out:?}");
        }
    }

    fn near(a: P, b: P) -> bool {
        length(sub(a, b)) < 1e-3
    }

    #[test]
    fn the_curve_passes_through_every_original_point() {
        let pts: Vec<P> = vec![[0.0, 0.0], [10.0, 6.0], [22.0, 8.0], [35.0, 4.0], [50.0, 0.0]];
        let curve = curve_through(&pts, 1.0);
        for p in &pts {
            assert!(curve.iter().any(|c| near(*c, *p)), "{p:?} is not on the curve");
        }
        assert!(curve.len() > pts.len() * 4, "it is cut into short pieces");
    }

    #[test]
    fn a_curve_is_smooth_where_the_points_are_gentle() {
        // Points along a circle of radius 50: the curve must stay within a hair of it.
        let pts: Vec<P> = (0..=12)
            .map(|i| {
                let a = i as f32 / 12.0 * std::f32::consts::PI;
                [50.0 * a.cos(), 50.0 * a.sin()]
            })
            .collect();
        for c in curve_through(&pts, 1.0) {
            let off = (length(c) - 50.0).abs();
            assert!(off < 0.15, "{c:?} is {off} from the circle");
        }
    }

    #[test]
    fn a_stroke_that_ends_after_a_corner_does_not_overshoot_its_end() {
        // A closed square, as the shape tool draws a rectangle: the curve must stay inside it, and
        // so must the same square drawn backwards.
        let square: Vec<P> = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]];
        for pts in [square.clone(), square.iter().rev().copied().collect()] {
            for c in curve_through(&pts, 0.5) {
                assert!(c[0] >= -1e-3 && c[0] <= 10.0 + 1e-3 && c[1] >= -1e-3 && c[1] <= 10.0 + 1e-3, "{c:?} left the square");
            }
        }
        // And a tick (a short stroke after a sharp turn) ends where its last point is.
        let tick: Vec<P> = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 6.0]];
        let curve = curve_through(&tick, 0.5);
        assert!(curve.iter().all(|c| c[1] <= 6.0 + 1e-3 && c[0] <= 20.0 + 1e-3));
    }

    #[test]
    fn straight_runs_stay_straight_and_do_not_overshoot() {
        let pts: Vec<P> = vec![[0.0, 5.0], [30.0, 5.0], [90.0, 5.0], [200.0, 5.0]];
        for c in curve_through(&pts, 2.0) {
            assert!((c[1] - 5.0).abs() < 1e-3, "{c:?} left the line");
            assert!(c[0] >= -1e-3 && c[0] <= 200.0 + 1e-3);
        }
    }

    #[test]
    fn a_sharp_corner_stays_sharp() {
        // A V: down 100 and up 100. Rounding it would pull the curve away from the point.
        let pts: Vec<P> = vec![[0.0, 0.0], [50.0, 100.0], [100.0, 0.0]];
        let curve = curve_through(&pts, 1.0);
        assert!(curve.iter().any(|c| near(*c, [50.0, 100.0])));
        let lowest = curve.iter().map(|c| c[1]).fold(f32::MIN, f32::max);
        assert!((lowest - 100.0).abs() < 1e-3, "the point of the V was shaved to {lowest}");
        // And each arm is a straight line.
        for c in curve.iter().filter(|c| c[0] <= 50.0) {
            assert!((c[1] - 2.0 * c[0]).abs() < 1e-2, "{c:?} bent off the first arm");
        }
    }

    #[test]
    fn degenerate_input_is_returned_untouched_or_safely() {
        assert!(curve_through(&[], 1.0).is_empty());
        assert_eq!(curve_through(&[[1.0, 2.0]], 1.0), vec![[1.0, 2.0]]);
        assert_eq!(curve_through(&[[1.0, 2.0], [3.0, 4.0]], 1.0).len(), 2);
        let same: Vec<P> = vec![[5.0, 5.0]; 6];
        assert!(curve_through(&same, 1.0).iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        // A bad step leaves the points alone rather than looping or exploding.
        let pts: Vec<P> = vec![[0.0, 0.0], [10.0, 0.0], [20.0, 5.0]];
        assert_eq!(curve_through(&pts, 0.0), pts);
        assert_eq!(curve_through(&pts, f32::NAN), pts);
    }

    #[test]
    fn a_huge_gap_is_cut_into_a_bounded_number_of_pieces() {
        let pts: Vec<P> = vec![[0.0, 0.0], [1_000_000.0, 0.0], [2_000_000.0, 10.0]];
        let curve = curve_through(&pts, 0.5);
        assert!(curve.len() <= 3 + 2 * MAX_SUBDIVISIONS);
    }

    #[test]
    fn a_curve_made_a_segment_at_a_time_is_the_curve_made_whole() {
        // Points arrive one at a time; the segments that are final are kept, the rest redone each time.
        let pts: Vec<P> = noise(200).chunks(2).enumerate().map(|(i, n)| [i as f32 * 3.0 + n[0] * 4.0, 100.0 + n[1] * 9.0]).collect();
        let mut kept = vec![pts[0]];
        let mut done = 0;
        for n in 3..=pts.len() {
            let so_far = &pts[..n];
            let stable = n - 2;
            curve_segments(so_far, done..stable, 0.75, &mut kept);
            done = stable;
            // What is kept, then the segments not yet final, is what the whole of `so_far` makes.
            let mut live = kept.clone();
            curve_segments(so_far, done..n - 1, 0.75, &mut live);
            assert_eq!(live, curve_through(so_far, 0.75), "after {n} points");
        }
    }
}
