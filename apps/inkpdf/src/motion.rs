//! How things move: the curves, springs and tweens the interface's animations are made of, and the rule for when none
//! of it should (the user asked for less motion). Plain arithmetic: it knows nothing of buttons, pages or the window, so
//! what is animated is the caller's to say and the time is an argument, which is what makes every animation testable.
//!
//! The feel is Material's: things start fast and settle softly (the emphasized curves), a press answers within a
//! tenth of a second, and nothing takes longer than about three tenths.

use std::collections::HashMap;
use std::hash::Hash;

/// The longest a single step of time is taken to be: a frame that stalled (a dialog, a drag of the window) does not make
/// an animation jump over its middle.
const MAX_STEP: f32 = 0.05;

/// How long things take, in seconds.
pub mod seconds {
    /// A press or a hover answering.
    pub const QUICK: f32 = 0.1;
    /// Something small coming or going.
    pub const SHORT: f32 = 0.18;
    /// A panel or a card.
    pub const MEDIUM: f32 = 0.28;
}

/// Whether to move at all. Off (`reduced`), every duration is nothing and a value is at its target at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub reduced: bool,
}

impl Default for Prefs {
    /// Full motion (under the tests' clocks, which do not run, none: what a test looks at is where things end up).
    fn default() -> Self {
        Self { reduced: cfg!(test) }
    }
}

impl Prefs {
    pub const FULL: Prefs = Prefs { reduced: false };
    pub const REDUCED: Prefs = Prefs { reduced: true };

    /// `seconds`, or nothing when motion is reduced.
    pub fn time(self, seconds: f32) -> f32 {
        if self.reduced { 0.0 } else { seconds }
    }
}

/// The shape of a motion over its time: how much of the way has been covered at each point of it (0 to 1 each).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Linear,
    /// Material's standard: a quick start, a soft stop. For things that stay on screen.
    Standard,
    /// Material's emphasized decelerate: arrives fast, settles slowly. For things that come in.
    Decelerate,
    /// Material's emphasized accelerate: leaves slowly, then goes. For things that go out.
    Accelerate,
}

impl Curve {
    /// The control points of the cubic Bézier the curve is, from (0, 0) to (1, 1).
    fn points(self) -> Option<[f32; 4]> {
        match self {
            Curve::Linear => None,
            Curve::Standard => Some([0.2, 0.0, 0.0, 1.0]),
            Curve::Decelerate => Some([0.05, 0.7, 0.1, 1.0]),
            Curve::Accelerate => Some([0.3, 0.0, 0.8, 0.15]),
        }
    }

    /// How far along (0 to 1) it is when `t` (0 to 1) of the time has gone.
    pub fn at(self, t: f32) -> f32 {
        let t = if t.is_nan() { 1.0 } else { t.clamp(0.0, 1.0) };
        let Some([x1, y1, x2, y2]) = self.points() else { return t };
        if t <= 0.0 || t >= 1.0 {
            return t;
        }
        // Find the parameter whose x is `t` (x is increasing in it), then take that parameter's y.
        let bezier = |a: f32, b: f32, s: f32| 3.0 * a * (1.0 - s) * (1.0 - s) * s + 3.0 * b * (1.0 - s) * s * s + s * s * s;
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if bezier(x1, x2, mid) < t { lo = mid } else { hi = mid }
        }
        bezier(y1, y2, (lo + hi) / 2.0).clamp(0.0, 1.0)
    }
}

/// A value that goes from where it is to a target over a time, along a curve. Given a new target on the way, it goes from
/// where it has got to, so a button pressed and let go again at once never jumps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    /// How much of its time has gone, in seconds, and how long it takes.
    spent: f32,
    time: f32,
    curve: Curve,
}

impl Default for Tween {
    fn default() -> Self {
        Self::at(0.0)
    }
}

impl Tween {
    /// At rest at `value`.
    pub fn at(value: f32) -> Self {
        Self { from: value, to: value, spent: 0.0, time: 0.0, curve: Curve::Standard }
    }

    pub fn value(&self) -> f32 {
        if self.spent >= self.time { self.to } else { self.from + (self.to - self.from) * self.curve.at(self.spent / self.time) }
    }

    /// Go to `target` in `seconds` along `curve` (at once for no time), from wherever it is now.
    pub fn go(&mut self, target: f32, seconds: f32, curve: Curve) {
        // (The same target again, as a caller that says it every frame does, leaves it as it is: it is not started over.)
        if target == self.to {
            return;
        }
        let now = self.value();
        *self = Self { from: now, to: target, spent: 0.0, time: seconds.max(0.0), curve };
    }

    /// Put it at `value` with no motion.
    pub fn jump(&mut self, value: f32) {
        *self = Self::at(value);
    }

    pub fn moving(&self) -> bool {
        self.spent < self.time
    }

    /// Let `dt` seconds go by. Whether it is still moving.
    pub fn step(&mut self, dt: f32) -> bool {
        if self.moving() {
            self.spent += dt.clamp(0.0, MAX_STEP);
        }
        self.moving()
    }
}

/// Something that comes and goes: it is kept while it is going so that it can be drawn fading out, after the state it
/// came from has already moved on.
#[derive(Clone, Debug)]
pub struct Presence<T> {
    held: Option<T>,
    amount: Tween,
    /// Whether it is wanted (it is held a while after it stops being).
    wanted: bool,
}

impl<T> Default for Presence<T> {
    fn default() -> Self {
        Self { held: None, amount: Tween::at(0.0), wanted: false }
    }
}

impl<T: Clone> Presence<T> {
    /// It is wanted now, looking like `value`. It comes in over `seconds` unless it was already there.
    pub fn show(&mut self, value: T, seconds: f32) {
        self.held = Some(value);
        if !self.wanted {
            self.wanted = true;
            self.amount.go(1.0, seconds, Curve::Decelerate);
        }
    }

    /// It is no longer wanted: it goes out over `seconds`, still drawn as it was.
    pub fn hide(&mut self, seconds: f32) {
        if self.wanted {
            self.wanted = false;
            self.amount.go(0.0, seconds, Curve::Accelerate);
        }
    }

    /// What there is to draw and how much of it (0 to 1), or nothing once it has gone.
    pub fn current(&self) -> Option<(&T, f32)> {
        let held = self.held.as_ref()?;
        let amount = self.amount.value();
        (self.wanted || amount > 0.0).then_some((held, amount))
    }

    #[cfg(test)]
    pub fn moving(&self) -> bool {
        self.amount.moving()
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let moving = self.amount.step(dt);
        if !self.wanted && !moving {
            self.held = None;
        }
        moving
    }

    /// Gone at once, or there at once.
    pub fn snap(&mut self) {
        self.amount.jump(if self.wanted { 1.0 } else { 0.0 });
        if !self.wanted {
            self.held = None;
        }
    }
}

/// Things that slide from where they were to where they are (the cells of a grid after a page is moved): each has an
/// offset that goes to nothing.
#[derive(Clone, Debug)]
pub struct Slides<K: Eq + Hash> {
    moving: HashMap<K, ([f32; 2], Tween)>,
}

impl<K: Eq + Hash> Default for Slides<K> {
    fn default() -> Self {
        Self { moving: HashMap::new() }
    }
}

impl<K: Eq + Hash + Copy> Slides<K> {
    /// `key` is `offset` away from where it should be, and goes there over `seconds` (from wherever it is now, if it was
    /// already on its way: `offset` is then added to what is left).
    pub fn start(&mut self, key: K, offset: [f32; 2], seconds: f32) {
        let left = self.offset(key);
        let total = [offset[0] + left[0], offset[1] + left[1]];
        if seconds <= 0.0 || (total[0].abs() < 0.5 && total[1].abs() < 0.5) {
            self.moving.remove(&key);
            return;
        }
        let mut tween = Tween::at(1.0);
        tween.go(0.0, seconds, Curve::Standard);
        self.moving.insert(key, (total, tween));
    }

    /// How far `key` is from where it should be now.
    pub fn offset(&self, key: K) -> [f32; 2] {
        self.moving.get(&key).map_or([0.0, 0.0], |(o, t)| [o[0] * t.value(), o[1] * t.value()])
    }

    /// Change what the keys mean (the page that was 5 is now 3): the slides go to the keys they now belong to.
    pub fn rekey(&mut self, mut to: impl FnMut(K) -> Option<K>) {
        let old = std::mem::take(&mut self.moving);
        for (key, slide) in old {
            if let Some(new) = to(key) {
                self.moving.insert(new, slide);
            }
        }
    }

    pub fn moving(&self) -> bool {
        !self.moving.is_empty()
    }

    pub fn step(&mut self, dt: f32) -> bool {
        for (_, tween) in self.moving.values_mut() {
            tween.step(dt);
        }
        self.moving.retain(|_, (_, tween)| tween.moving());
        self.moving()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_nothing_ends_at_all_and_never_goes_backwards_nor_outside() {
        for curve in [Curve::Linear, Curve::Standard, Curve::Decelerate, Curve::Accelerate] {
            assert_eq!(curve.at(0.0), 0.0, "{curve:?}");
            assert!((curve.at(1.0) - 1.0).abs() < 1e-3, "{curve:?}");
            let mut last = 0.0;
            for k in 0..=200 {
                let v = curve.at(k as f32 / 200.0);
                assert!((0.0..=1.0).contains(&v) && v >= last - 1e-4, "{curve:?} at {k}: {v} after {last}");
                last = v;
            }
            assert_eq!(curve.at(f32::NAN), 1.0, "a time that is not a number is the end");
            assert_eq!(curve.at(-3.0), 0.0);
            assert!((curve.at(9.0) - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn the_emphasized_curves_come_in_fast_and_go_out_slowly_and_the_other_way_round() {
        assert!(Curve::Decelerate.at(0.25) > 0.5, "arrives early");
        assert!(Curve::Accelerate.at(0.25) < 0.15, "leaves late");
        assert!(Curve::Standard.at(0.25) > Curve::Linear.at(0.25));
    }

    #[test]
    fn a_tween_goes_there_in_its_time_and_a_new_target_starts_from_where_it_is() {
        let mut t = Tween::at(0.0);
        t.go(1.0, 0.2, Curve::Linear);
        assert!(t.moving() && t.value() == 0.0);
        t.step(0.05);
        t.step(0.05);
        assert!((t.value() - 0.5).abs() < 1e-5, "{}", t.value());
        t.go(0.0, 0.2, Curve::Linear);
        assert!((t.value() - 0.5).abs() < 1e-5, "no jump when turned round");
        for _ in 0..8 {
            t.step(0.05);
        }
        assert!(!t.moving() && t.value() == 0.0);
        // No time: there at once.
        t.go(0.7, 0.0, Curve::Standard);
        assert_eq!(t.value(), 0.7);
        assert!(!t.moving());
    }

    #[test]
    fn the_same_target_said_every_frame_does_not_start_it_over() {
        let mut t = Tween::at(0.0);
        let mut frames = 0;
        loop {
            t.go(1.0, 0.1, Curve::Standard);
            if !t.step(0.016) || frames > 100 {
                break;
            }
            frames += 1;
        }
        assert!(frames < 10 && t.value() == 1.0, "{frames} frames, at {}", t.value());
    }

    #[test]
    fn a_long_stall_is_a_short_step_and_nonsense_time_does_nothing() {
        let mut t = Tween::at(0.0);
        t.go(1.0, 0.4, Curve::Linear);
        t.step(5.0);
        assert!(t.value() < 0.2, "{}", t.value());
        t.step(f32::NAN);
        t.step(-1.0);
        t.step(f32::INFINITY);
        assert!(t.value().is_finite() && t.value() <= 1.0);
    }

    #[test]
    fn a_presence_is_kept_while_it_goes_out_and_then_let_go() {
        let mut p: Presence<&str> = Presence::default();
        assert!(p.current().is_none());
        p.show("card", 0.2);
        assert_eq!(p.current().map(|c| c.1), Some(0.0));
        for _ in 0..20 {
            p.step(0.016);
        }
        assert_eq!(p.current(), Some((&"card", 1.0)));
        p.hide(0.2);
        p.step(0.05);
        let (held, amount) = p.current().expect("still going out");
        assert_eq!(*held, "card");
        assert!(amount < 1.0 && amount > 0.0);
        for _ in 0..20 {
            p.step(0.016);
        }
        assert!(p.current().is_none() && !p.moving(), "gone");
        // Brought back while it was going: it comes in from where it was.
        p.show("again", 0.2);
        p.step(0.05);
        p.hide(0.2);
        p.step(0.05);
        p.show("back", 0.2);
        assert!(p.current().is_some_and(|(v, a)| *v == "back" && a > 0.0));
        p.snap();
        assert_eq!(p.current(), Some((&"back", 1.0)));
    }

    #[test]
    fn slides_start_away_and_arrive_and_follow_a_change_of_keys() {
        let mut s: Slides<usize> = Slides::default();
        s.start(3, [100.0, -40.0], 0.3);
        assert_eq!(s.offset(3), [100.0, -40.0]);
        assert_eq!(s.offset(4), [0.0, 0.0]);
        s.step(0.05);
        s.step(0.05);
        let mid = s.offset(3);
        assert!(mid[0] < 100.0 && mid[0] > 0.0);
        // Page 3 became page 1; what was 7 is gone.
        s.start(7, [5.0, 5.0], 0.3);
        s.rekey(|k| (k == 3).then_some(1));
        assert_eq!(s.offset(1), mid);
        assert_eq!(s.offset(3), [0.0, 0.0]);
        assert_eq!(s.offset(7), [0.0, 0.0]);
        for _ in 0..20 {
            s.step(0.05);
        }
        assert!(!s.moving() && s.offset(1) == [0.0, 0.0]);
        // No time, or nothing to move: nothing starts.
        s.start(1, [50.0, 0.0], 0.0);
        s.start(2, [0.1, 0.1], 0.3);
        assert!(!s.moving());
    }

    #[test]
    fn reduced_motion_takes_no_time() {
        assert_eq!(Prefs::REDUCED.time(seconds::MEDIUM), 0.0);
        assert_eq!(Prefs::FULL.time(seconds::MEDIUM), seconds::MEDIUM);
        const { assert!(seconds::QUICK < seconds::SHORT && seconds::SHORT < seconds::MEDIUM) };
        const { assert!(seconds::MEDIUM <= 0.4, "nothing takes longer than four tenths") };
    }
}
