//! Palm rejection.

use std::collections::HashSet;

use super::{Phase, PointerEvent};

/// Touches are ignored while a pen is near, and for a moment after, and when too big to be a
/// fingertip: a resting palm must not pan the page or select anything.
#[derive(Debug, Default)]
pub struct PalmGuard {
    pen_near: bool,
    pen_last_near_ms: Option<f64>,
    admitted: HashSet<u32>,
    rejected: HashSet<u32>,
}

const PEN_GRACE_MS: f64 = 450.0;

impl PalmGuard {
    pub fn pen_event(&mut self, ev: &PointerEvent, now_ms: f64) {
        let near = ev.phase != Phase::Leave && (ev.pen.in_range || ev.pen.contact);
        self.pen_near = near;
        if near {
            self.pen_last_near_ms = Some(now_ms);
        }
        // A pen touching down while fingers are down: those fingers are a hand resting.
        if ev.pen.contact {
            self.rejected.extend(self.admitted.drain());
        }
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
}
