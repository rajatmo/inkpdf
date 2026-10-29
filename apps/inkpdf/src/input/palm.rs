//! Palm rejection.

use std::collections::HashSet;

use super::{Phase, PointerEvent};

/// Touches are ignored for as long as a pen is near (hovering above the screen or touching it), the way note-taking apps
/// do it: the hand that holds the pen is resting on the glass, and nothing it does is meant. A finger already down when the
/// pen comes near is given up. A pen whose digitiser does not report hovering is only known near while it touches, so
/// touches stay ignored for a moment after it lifts. A contact too big to be a fingertip is a palm whatever the pen does.
#[derive(Debug, Default)]
pub struct PalmGuard {
    pen_near: bool,
    /// Whether this pen has reported hovering: only then is "in range" after a lift believed, until it says it left.
    hover_seen: bool,
    pen_last_near_ms: Option<f64>,
    admitted: HashSet<u32>,
    rejected: HashSet<u32>,
}

const PEN_GRACE_MS: f64 = 450.0;

impl PalmGuard {
    /// Take note of the pen. Returns whether touches that were being used are now given up (the pen came near while
    /// fingers were down), so the caller can drop what they had started.
    pub fn pen_event(&mut self, ev: &PointerEvent, now_ms: f64) -> bool {
        self.hover_seen |= ev.phase == Phase::Hover;
        let near = ev.phase != Phase::Leave && (ev.pen.contact || ev.pen.in_range && self.hover_seen);
        self.pen_near = near;
        if near {
            self.pen_last_near_ms = Some(now_ms);
            let revoked = !self.admitted.is_empty();
            self.rejected.extend(self.admitted.drain());
            return revoked;
        }
        if ev.phase == Phase::Leave {
            self.pen_last_near_ms = Some(now_ms);
        }
        false
    }

    /// Forget the pen: the window lost focus or the app was suspended, and a "left" may never come.
    pub fn reset(&mut self) {
        self.pen_near = false;
        self.pen_last_near_ms = None;
    }

    /// Whether this touch event should be used. A touch is judged when it goes down and keeps
    /// that verdict until it lifts.
    pub fn admit_touch(&mut self, ev: &PointerEvent, now_ms: f64, max_contact_px: f32) -> bool {
        match ev.phase {
            Phase::Down => {
                let pen_recent = self.pen_near || self.pen_last_near_ms.is_some_and(|t| now_ms - t < PEN_GRACE_MS);
                let too_big = ev.contact_px > max_contact_px;
                if pen_recent || too_big {
                    self.rejected.insert(ev.id);
                    false
                } else {
                    self.admitted.insert(ev.id);
                    true
                }
            }
            Phase::Up | Phase::Cancel => {
                self.admitted.remove(&ev.id);
                !self.rejected.remove(&ev.id)
            }
            _ => self.admitted.contains(&ev.id) && !self.rejected.contains(&ev.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::PenButtons;
    use super::super::fixtures::*;
    use super::*;

    #[test]
    fn touches_are_ignored_while_the_pen_is_near() {
        let mut g = PalmGuard::default();
        g.pen_event(&pen_ev(Phase::Hover, PenButtons { in_range: true, ..PenButtons::default() }), 1000.0);
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 1010.0, 100.0));
        // Its later events and its lift are ignored too.
        assert!(!g.admit_touch(&touch_ev(1, Phase::Move, 20.0), 1020.0, 100.0));
        assert!(!g.admit_touch(&touch_ev(1, Phase::Up, 20.0), 1030.0, 100.0));
    }

    #[test]
    fn touches_return_a_moment_after_the_pen_leaves() {
        let mut g = PalmGuard::default();
        g.pen_event(&pen_ev(Phase::Hover, PenButtons { in_range: true, ..PenButtons::default() }), 1000.0);
        g.pen_event(&pen_ev(Phase::Leave, PenButtons::default()), 1100.0);
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 1300.0, 100.0), "still within the grace period");
        assert!(g.admit_touch(&touch_ev(2, Phase::Down, 20.0), 1700.0, 100.0));
        assert!(g.admit_touch(&touch_ev(2, Phase::Move, 20.0), 1710.0, 100.0));
    }

    #[test]
    fn a_large_contact_is_a_palm() {
        let mut g = PalmGuard::default();
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 180.0), 0.0, 100.0));
        assert!(g.admit_touch(&touch_ev(2, Phase::Down, 40.0), 0.0, 100.0));
    }

    #[test]
    fn the_pen_touching_down_drops_fingers_already_resting() {
        let mut g = PalmGuard::default();
        assert!(g.admit_touch(&touch_ev(1, Phase::Down, 40.0), 0.0, 100.0));
        g.pen_event(&pen_ev(Phase::Down, pen(false, false)), 500.0);
        assert!(!g.admit_touch(&touch_ev(1, Phase::Move, 40.0), 510.0, 100.0));
    }

    #[test]
    fn a_finger_down_when_the_pen_comes_near_is_given_up() {
        let mut g = PalmGuard::default();
        assert!(g.admit_touch(&touch_ev(1, Phase::Down, 40.0), 0.0, 100.0));
        let hover = pen_ev(Phase::Hover, PenButtons { in_range: true, ..PenButtons::default() });
        assert!(g.pen_event(&hover, 10.0), "it says touches were revoked");
        assert!(!g.pen_event(&hover, 20.0), "and only once");
        assert!(!g.admit_touch(&touch_ev(1, Phase::Move, 40.0), 30.0, 100.0));
        assert!(!g.admit_touch(&touch_ev(1, Phase::Up, 40.0), 40.0, 100.0), "its lift is not used either");
    }

    #[test]
    fn a_pen_that_hovers_keeps_touch_off_until_it_leaves_however_long_it_waits() {
        let mut g = PalmGuard::default();
        g.pen_event(&pen_ev(Phase::Hover, PenButtons { in_range: true, ..PenButtons::default() }), 0.0);
        g.pen_event(&pen_ev(Phase::Up, PenButtons { in_range: true, ..PenButtons::default() }), 100.0);
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 60_000.0, 100.0), "a minute later, still hovering");
        g.pen_event(&pen_ev(Phase::Leave, PenButtons::default()), 61_000.0);
        assert!(g.admit_touch(&touch_ev(2, Phase::Down, 20.0), 62_000.0, 100.0));
    }

    #[test]
    fn a_pen_that_never_hovers_does_not_lock_touch_out_after_it_lifts() {
        let mut g = PalmGuard::default();
        g.pen_event(&pen_ev(Phase::Down, pen(false, false)), 0.0);
        g.pen_event(&pen_ev(Phase::Up, PenButtons { in_range: true, ..PenButtons::default() }), 100.0);
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 300.0, 100.0), "a moment of grace");
        assert!(g.admit_touch(&touch_ev(2, Phase::Down, 20.0), 1000.0, 100.0), "then fingers work again");
    }

    #[test]
    fn losing_focus_forgets_a_hover_that_never_said_it_left() {
        let mut g = PalmGuard::default();
        g.pen_event(&pen_ev(Phase::Hover, PenButtons { in_range: true, ..PenButtons::default() }), 0.0);
        g.reset();
        assert!(g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 5000.0, 100.0));
    }
}
