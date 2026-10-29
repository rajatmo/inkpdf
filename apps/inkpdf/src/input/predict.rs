//! Pen prediction: a short extrapolation drawn ahead of the real stroke.

use super::Sample;

/// Where the pen is heading in `ahead_ms`, from the last few samples; `None` when it is not
/// moving enough to say. Only ever drawn ahead of the real stroke, never saved.
pub fn predict(samples: &[Sample], ahead_ms: f64) -> Option<[f32; 2]> {
    let n = samples.len();
    if n < 3 {
        return None;
    }
    let (a, b) = (samples[n - 3], samples[n - 1]);
    let dt = b.t_ms - a.t_ms;
    if dt <= 0.0 || dt > 80.0 {
        return None;
    }
    let k = (ahead_ms / dt) as f32;
    let (dx, dy) = ((b.pos[0] - a.pos[0]) * k, (b.pos[1] - a.pos[1]) * k);
    // Never reach further than a sensible distance: a jerk must not draw a long spike.
    let reach = (dx * dx + dy * dy).sqrt();
    if reach <= 0.5 {
        return None;
    }
    let at = |back: usize| samples[n - back].pos;
    let sharpest = if n >= 4 { Some(at(4)) } else { None };
    let (heading, cap) = heading_and_cap([dx / reach, dy / reach], sharpest, at(3), at(2), at(1));
    let reach = reach.min(cap).min(40.0);
    Some([b.pos[0] + heading[0] * reach, b.pos[1] + heading[1] * reach])
}

/// The turn at `b` going from `a` through it to `c`, in radians, positive one way and negative the other.
fn turn_at(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    let (u, w) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
    (u[0] * w[1] - u[1] * w[0]).atan2(u[0] * w[0] + u[1] * w[1])
}

/// The way the pen is going at `c`, the newest of the last positions, and how far a straight guess along that may
/// lead it. On a straight run the way is `chord` (from the oldest of three to the newest) and there is no limit. On
/// a turn the chord points where the pen was a sample ago, so the way is turned on by half a step's turn; and a
/// straight guess strays from a path of radius `r` by about its length squared over `2r`, so it is cut where that
/// is half a pixel: ink that sticks out of a turn is seen to swing out and snap back. (One turn is measured from
/// three noisy positions and can come out too small, so the sharper of the last two sets the limit.)
fn heading_and_cap(chord: [f32; 2], before: Option<[f32; 2]>, a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> ([f32; 2], f32) {
    const SAG_PX: f32 = 0.5;
    let w = [c[0] - b[0], c[1] - b[1]];
    let step = (w[0] * w[0] + w[1] * w[1]).sqrt();
    let turn = turn_at(a, b, c);
    let sharpest = before.map_or(turn.abs(), |p| turn.abs().max(turn_at(p, a, b).abs()));
    if step < 1e-3 || sharpest < 1e-3 {
        return (chord, f32::INFINITY);
    }
    let (sin, cos) = (turn / 2.0).sin_cos();
    let heading = [w[0] / step * cos - w[1] / step * sin, w[0] / step * sin + w[1] / step * cos];
    (heading, (2.0 * SAG_PX * step / sharpest).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(points: &[([f32; 2], f64)]) -> Vec<Sample> {
        points.iter().map(|&(pos, t_ms)| Sample { pos, pressure: 0.5, t_ms }).collect()
    }

    #[test]
    fn prediction_continues_the_motion_and_is_capped() {
        let s = samples(&[([0.0, 0.0], 0.0), ([5.0, 0.0], 4.0), ([10.0, 0.0], 8.0)]);
        // 10 px per 8 ms ahead 8 ms is 10 px further.
        let p = predict(&s, 8.0).expect("prediction");
        assert!((p[0] - 20.0).abs() < 1e-3 && p[1].abs() < 1e-3);
        // A very fast move cannot predict beyond the cap.
        let fast = samples(&[([0.0, 0.0], 0.0), ([500.0, 0.0], 1.0), ([1000.0, 0.0], 2.0)]);
        let p = predict(&fast, 16.0).expect("prediction");
        assert!((p[0] - 1040.0).abs() < 1e-2);
    }

    #[test]
    fn a_guess_on_a_tight_turn_stays_on_the_turn_and_one_on_a_straight_run_is_not_shortened() {
        // A loop of radius 8 written at 600 px/s, a sample every 4 ms: 12 ms ahead the pen is 7 px on, but a straight
        // guess that long would leave the loop by about 3 px. It is cut to where it is still near the loop.
        let on_loop = |i: usize| {
            let angle = i as f32 * 0.3;
            ([8.0 * angle.cos(), 8.0 * angle.sin()], i as f64 * 4.0)
        };
        let guess = predict(&samples(&[on_loop(0), on_loop(1), on_loop(2)]), 12.0).expect("prediction");
        let off = (guess[0].hypot(guess[1]) - 8.0).abs();
        let reach = (guess[0] - on_loop(2).0[0]).hypot(guess[1] - on_loop(2).0[1]);
        assert!(off < 0.7, "the guess is {off} px off the loop");
        assert!(reach > 1.5, "but it still leads the pen by {reach} px");
        // The same speed on a straight line: the whole 12 ms.
        let line = samples(&[([0.0, 0.0], 0.0), ([2.4, 0.0], 4.0), ([4.8, 0.0], 8.0)]);
        assert!((predict(&line, 12.0).expect("prediction")[0] - 11.999).abs() < 0.01);
    }

    #[test]
    fn no_prediction_without_motion_or_history() {
        assert_eq!(predict(&samples(&[([0.0, 0.0], 0.0), ([0.0, 0.0], 4.0), ([0.0, 0.0], 8.0)]), 8.0), None);
        assert_eq!(predict(&samples(&[([0.0, 0.0], 0.0), ([5.0, 0.0], 4.0)]), 8.0), None);
        // A long pause in the samples means the pen stopped and restarted.
        assert_eq!(predict(&samples(&[([0.0, 0.0], 0.0), ([5.0, 0.0], 100.0), ([10.0, 0.0], 200.0)]), 8.0), None);
    }
}
