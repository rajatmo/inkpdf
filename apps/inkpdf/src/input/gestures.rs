//! Touch gestures: pan, pinch, tap and fling from raw finger positions.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureDelta {
    /// Movement of the centre of the touches, in pixels.
    pub pan: [f32; 2],
    /// Change in the distance between two touches; 1.0 with one touch.
    pub zoom: f32,
    pub centre: [f32; 2],
    pub fingers: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Released {
    /// Where a quick, still tap with a single finger happened.
    pub tap: Option<[f32; 2]>,
    /// Pixels per second to glide on with, when the last finger left while moving.
    pub fling: Option<[f32; 2]>,
}

#[derive(Debug)]
struct Track {
    pos: [f32; 2],
    start: [f32; 2],
    start_ms: f64,
}

#[derive(Debug, Default)]
pub struct TouchGestures {
    touches: BTreeMap<u32, Track>,
    /// Recent centre positions, for the speed at lift-off.
    trail: Vec<(f64, [f32; 2])>,
    moved_far: bool,
    peak_fingers: usize,
    /// Screen pixels per logical pixel (what the distances below are in), 0 until told: taken as 1. A finger's jitter is millimetres, not pixels.
    scale: f32,
}

/// How far a finger may wander and still be tapping, in logical pixels (Android's own touch slop is 8, and a finger on a tablet wanders more).
const TAP_SLOP_PX: f32 = 12.0;
const TAP_MAX_MS: f64 = 350.0;
const FLING_WINDOW_MS: f64 = 90.0;
const FLING_MIN_SPEED: f32 = 250.0;

impl TouchGestures {
    /// Tell it how many screen pixels a logical pixel is, so that a tap or a fling is judged by what the hand did, not by how dense the screen is.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    }

    fn scale(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }

    pub fn fingers(&self) -> usize {
        self.touches.len()
    }

    fn centre(&self) -> Option<[f32; 2]> {
        let n = self.touches.len();
        if n == 0 {
            return None;
        }
        let sum = self.touches.values().fold([0.0, 0.0], |s, t| [s[0] + t.pos[0], s[1] + t.pos[1]]);
        Some([sum[0] / n as f32, sum[1] / n as f32])
    }

    /// The distance between the first two touches, or 0.
    fn spread(&self) -> f32 {
        let mut it = self.touches.values();
        match (it.next(), it.next()) {
            (Some(a), Some(b)) => ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2)).sqrt(),
            _ => 0.0,
        }
    }

    pub fn down(&mut self, id: u32, pos: [f32; 2], t_ms: f64) {
        if self.touches.is_empty() {
            self.moved_far = false;
            self.peak_fingers = 0;
            self.trail.clear();
        }
        self.touches.insert(id, Track { pos, start: pos, start_ms: t_ms });
        self.peak_fingers = self.peak_fingers.max(self.touches.len());
        // A different set of fingers starts a new motion: no jump from the change in centre.
        self.trail.clear();
        if let Some(c) = self.centre() {
            self.trail.push((t_ms, c));
        }
    }

    pub fn moved(&mut self, id: u32, pos: [f32; 2], t_ms: f64) -> Option<GestureDelta> {
        let (before_centre, before_spread) = (self.centre()?, self.spread());
        let track = self.touches.get_mut(&id)?;
        track.pos = pos;
        if ((pos[0] - track.start[0]).powi(2) + (pos[1] - track.start[1]).powi(2)).sqrt() > TAP_SLOP_PX * self.scale() {
            self.moved_far = true;
        }
        let (centre, spread) = (self.centre()?, self.spread());
        self.trail.push((t_ms, centre));
        self.trail.retain(|(t, _)| t_ms - t <= FLING_WINDOW_MS * 2.0);
        let zoom = if self.touches.len() >= 2 && before_spread > 1.0 && spread > 1.0 { spread / before_spread } else { 1.0 };
        Some(GestureDelta { pan: [centre[0] - before_centre[0], centre[1] - before_centre[1]], zoom, centre, fingers: self.touches.len() })
    }

    pub fn up(&mut self, id: u32, pos: [f32; 2], t_ms: f64) -> Released {
        let Some(track) = self.touches.remove(&id) else { return Released { tap: None, fling: None } };
        let last = self.touches.is_empty();
        let tap = (last && self.peak_fingers == 1 && !self.moved_far && t_ms - track.start_ms <= TAP_MAX_MS).then_some(pos);
        let fling = if last && self.moved_far && self.peak_fingers == 1 { self.velocity(t_ms) } else { None };
        // The remaining fingers' centre has jumped; restart the trail from it.
        self.trail.clear();
        if let Some(c) = self.centre() {
            self.trail.push((t_ms, c));
        }
        Released { tap, fling }
    }

    /// Speed over the last moments before `t_ms`.
    fn velocity(&self, t_ms: f64) -> Option<[f32; 2]> {
        let recent: Vec<&(f64, [f32; 2])> = self.trail.iter().filter(|(t, _)| t_ms - t <= FLING_WINDOW_MS).collect();
        let (first, last) = (recent.first()?, recent.last()?);
        let dt = ((last.0 - first.0) / 1000.0) as f32;
        if dt <= 0.0 {
            return None;
        }
        let v = [(last.1[0] - first.1[0]) / dt, (last.1[1] - first.1[1]) / dt];
        ((v[0] * v[0] + v[1] * v[1]).sqrt() >= FLING_MIN_SPEED * self.scale()).then_some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_finger_pans_by_how_far_it_moves() {
        let mut g = TouchGestures::default();
        g.down(1, [100.0, 100.0], 0.0);
        let d = g.moved(1, [110.0, 130.0], 16.0).expect("delta");
        assert_eq!(d.pan, [10.0, 30.0]);
        assert_eq!((d.zoom, d.fingers), (1.0, 1));
    }

    #[test]
    fn two_fingers_pinch_and_pan_about_their_centre() {
        let mut g = TouchGestures::default();
        g.down(1, [100.0, 100.0], 0.0);
        g.down(2, [200.0, 100.0], 0.0);
        // The second finger moves away: the spread grows from 100 to 150.
        let d = g.moved(2, [250.0, 100.0], 16.0).expect("delta");
        assert!((d.zoom - 1.5).abs() < 1e-4);
        assert_eq!(d.fingers, 2);
        assert_eq!(d.centre, [175.0, 100.0]);
        // Both move together: pure pan, no zoom.
        let d = g.moved(1, [110.0, 110.0], 32.0).expect("delta");
        let d2 = g.moved(2, [260.0, 110.0], 33.0).expect("delta");
        assert!(d.zoom < 1.0 && d2.zoom > 1.0);
        assert!((d.zoom * d2.zoom - 1.0).abs() < 1e-3, "net zoom for a pure pan is 1");
    }

    #[test]
    fn a_tap_is_judged_in_logical_pixels_not_screen_pixels() {
        // A finger wobbles 20 screen pixels: a swipe on a plain screen, a hair's breadth on one at 3 times the scale.
        for (scale, tap) in [(1.0, false), (3.0, true)] {
            let mut g = TouchGestures::default();
            g.set_scale(scale);
            g.down(1, [50.0, 60.0], 0.0);
            g.moved(1, [70.0, 60.0], 30.0);
            assert_eq!(g.up(1, [70.0, 60.0], 120.0).tap.is_some(), tap, "scale {scale}");
        }
    }

    #[test]
    fn a_quick_still_touch_is_a_tap() {
        let mut g = TouchGestures::default();
        g.down(1, [50.0, 60.0], 0.0);
        g.moved(1, [52.0, 61.0], 30.0);
        let r = g.up(1, [52.0, 61.0], 120.0);
        assert_eq!(r.tap, Some([52.0, 61.0]));
        assert_eq!(r.fling, None);
    }

    #[test]
    fn a_long_press_or_a_drag_or_two_fingers_is_not_a_tap() {
        let mut g = TouchGestures::default();
        g.down(1, [0.0, 0.0], 0.0);
        assert_eq!(g.up(1, [0.0, 0.0], 800.0).tap, None, "too slow");

        g.down(1, [0.0, 0.0], 0.0);
        g.moved(1, [40.0, 0.0], 50.0);
        assert_eq!(g.up(1, [40.0, 0.0], 100.0).tap, None, "moved too far");

        g.down(1, [0.0, 0.0], 0.0);
        g.down(2, [50.0, 0.0], 0.0);
        g.up(2, [50.0, 0.0], 50.0);
        assert_eq!(g.up(1, [0.0, 0.0], 60.0).tap, None, "two fingers were down");
    }

    #[test]
    fn lifting_a_moving_finger_flings() {
        let mut g = TouchGestures::default();
        g.down(1, [0.0, 0.0], 0.0);
        for i in 1..=6 {
            g.moved(1, [0.0, i as f32 * 16.0], i as f64 * 16.0);
        }
        // 16 px every 16 ms is 1000 px/s downwards.
        let r = g.up(1, [0.0, 96.0], 100.0);
        let v = r.fling.expect("fling");
        assert!(v[0].abs() < 1.0 && (v[1] - 1000.0).abs() < 60.0, "{v:?}");
        // A finger that stopped before lifting does not fling.
        let mut g = TouchGestures::default();
        g.down(1, [0.0, 0.0], 0.0);
        g.moved(1, [0.0, 100.0], 16.0);
        g.moved(1, [0.0, 100.0], 200.0);
        assert_eq!(g.up(1, [0.0, 100.0], 400.0).fling, None);
    }
}
