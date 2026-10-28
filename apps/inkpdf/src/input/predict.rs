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
    let cap = 40.0;
    let s = if reach > cap { cap / reach } else { 1.0 };
    (reach > 0.5).then_some([b.pos[0] + dx * s, b.pos[1] + dy * s])
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
    fn no_prediction_without_motion_or_history() {
        assert_eq!(predict(&samples(&[([0.0, 0.0], 0.0), ([0.0, 0.0], 4.0), ([0.0, 0.0], 8.0)]), 8.0), None);
        assert_eq!(predict(&samples(&[([0.0, 0.0], 0.0), ([5.0, 0.0], 4.0)]), 8.0), None);
        // A long pause in the samples means the pen stopped and restarted.
        assert_eq!(predict(&samples(&[([0.0, 0.0], 0.0), ([5.0, 0.0], 100.0), ([10.0, 0.0], 200.0)]), 8.0), None);
    }
}
