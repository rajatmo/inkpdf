//! Palm rejection.

use std::collections::HashSet;

use super::{Phase, PointerEvent};

/// Touch is off for as long as a pen is near: hovering above the screen or touching it. The hand that holds the pen rests on the
/// glass, and nothing it does is meant, so (as in note-taking apps) a finger needs no more judging than that. A finger already
/// down when the pen arrives is given up. A contact too big to be a fingertip is a palm whatever the pen does.
///
/// "Near" is what the source says: `PenButtons::in_range` of the last pen event. A source whose screen does not report hover says
/// a lifted pen is out of range, so touch is not locked out after a stroke.
#[derive(Debug, Default)]
pub struct PalmGuard {
    pen_near: bool,
    admitted: HashSet<u32>,
    rejected: HashSet<u32>,
}

impl PalmGuard {
    /// Take note of the pen. Returns whether fingers that were being used are now given up (the pen came near while they were
    /// down), so the caller can drop what they had started.
    pub fn pen_event(&mut self, ev: &PointerEvent) -> bool {
        self.pen_near = !matches!(ev.phase, Phase::Leave | Phase::Cancel) && (ev.pen.in_range || ev.pen.contact);
        if !self.pen_near {
            return false;
        }
        let revoked = !self.admitted.is_empty();
        self.rejected.extend(self.admitted.drain());
        revoked
    }

    /// Forget the pen: the window lost focus or the app was suspended, and a "left" may never come.
    pub fn reset(&mut self) {
        self.pen_near = false;
    }

    /// Whether this touch event should be used. A touch is judged when it goes down and keeps that verdict until it lifts.
    pub fn admit_touch(&mut self, ev: &PointerEvent, max_contact_px: f32) -> bool {
        match ev.phase {
            Phase::Down => {
                if self.pen_near || ev.contact_px > max_contact_px {
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

    fn hover() -> PointerEvent {
        pen_ev(Phase::Hover, PenButtons { in_range: true, ..PenButtons::default() })
    }

    #[test]
    fn touch_is_off_while_the_pen_hovers_however_long_that_is_and_back_the_moment_it_leaves() {
        let mut g = PalmGuard::default();
        g.pen_event(&hover());
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 100.0));
        // Its later events and its lift are ignored too.
        assert!(!g.admit_touch(&touch_ev(1, Phase::Move, 20.0), 100.0));
        assert!(!g.admit_touch(&touch_ev(1, Phase::Up, 20.0), 100.0));
        // Touching with the tip and lifting (still in range) changes nothing.
        g.pen_event(&pen_ev(Phase::Down, pen(false, false)));
        g.pen_event(&pen_ev(Phase::Up, PenButtons { in_range: true, ..PenButtons::default() }));
        assert!(!g.admit_touch(&touch_ev(2, Phase::Down, 20.0), 100.0));
        g.pen_event(&pen_ev(Phase::Leave, PenButtons::default()));
        assert!(g.admit_touch(&touch_ev(3, Phase::Down, 20.0), 100.0), "at once, with no wait");
        assert!(g.admit_touch(&touch_ev(3, Phase::Move, 20.0), 100.0));
    }

    #[test]
    fn a_pen_that_says_it_is_out_of_range_after_a_lift_does_not_lock_touch_out() {
        let mut g = PalmGuard::default();
        g.pen_event(&pen_ev(Phase::Down, pen(false, false)));
        g.pen_event(&pen_ev(Phase::Up, PenButtons::default()));
        assert!(g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 100.0));
    }

    #[test]
    fn a_large_contact_is_a_palm() {
        let mut g = PalmGuard::default();
        assert!(!g.admit_touch(&touch_ev(1, Phase::Down, 180.0), 100.0));
        assert!(g.admit_touch(&touch_ev(2, Phase::Down, 40.0), 100.0));
    }

    #[test]
    fn a_finger_down_when_the_pen_comes_near_is_given_up() {
        let mut g = PalmGuard::default();
        assert!(g.admit_touch(&touch_ev(1, Phase::Down, 40.0), 100.0));
        assert!(g.pen_event(&hover()), "it says fingers were given up");
        assert!(!g.pen_event(&hover()), "and only once");
        assert!(!g.admit_touch(&touch_ev(1, Phase::Move, 40.0), 100.0));
        assert!(!g.admit_touch(&touch_ev(1, Phase::Up, 40.0), 100.0), "its lift is not used either");
    }

    #[test]
    fn losing_focus_forgets_a_hover_that_never_said_it_left() {
        let mut g = PalmGuard::default();
        g.pen_event(&hover());
        g.reset();
        assert!(g.admit_touch(&touch_ev(1, Phase::Down, 20.0), 100.0));
    }
}
