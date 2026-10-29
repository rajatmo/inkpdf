//! The rules every source of pointer events keeps, as a function that checks a stream. A source is the OS's own pen and touch
//! reader (`platform::PointerSource`); the editor is written for streams that keep these rules, and a new source (Wayland's
//! tablet protocol, Android's `MotionEvent`) is right when what it makes passes [`check`] for any gesture it is given. The same
//! check is what a recorded session is validated with before it is replayed.
//!
//! The rules, per pointer (a device and an id):
//!
//! * a press goes down, moves and comes up (or is cancelled), in that order, and a pointer that is not down only hovers or leaves
//!   (a finger does not hover);
//! * every sample is a real place (finite), a pressure from 0 to 1, and a time (`t_ms`, on the clock of [`crate::clock`]) that
//!   never goes back, within an event and from one event of the pointer to the next;
//! * a pen that moves with contact is in contact, and one that hovers is not.

use std::collections::HashMap;

use super::{Device, Phase, PointerEvent};

/// Where in the stream, and what, a rule was broken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broken {
    /// The index of the event.
    pub at: usize,
    pub rule: String,
}

impl std::fmt::Display for Broken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "event {}: {}", self.at, self.rule)
    }
}

/// What a pointer is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Track {
    down: bool,
    /// The time of its last sample.
    t_ms: f64,
}

/// A checker that is given the events one at a time (a source's `drain` is called once a frame, so a stream arrives that way).
#[derive(Default)]
pub struct Checker {
    seen: usize,
    pointers: HashMap<(Device, u32), Track>,
}

impl Checker {
    /// Check the next event of the stream.
    pub fn next(&mut self, ev: &PointerEvent) -> Result<(), Broken> {
        let at = self.seen;
        self.seen += 1;
        let broken = |rule: &str| Broken { at, rule: rule.to_string() };
        let key = (ev.device, ev.id);
        let state = self.pointers.entry(key).or_insert(Track { down: false, t_ms: f64::NEG_INFINITY });

        if ev.samples.is_empty() && !matches!(ev.phase, Phase::Leave | Phase::Cancel) {
            return Err(broken("an event that moves, presses or hovers carries at least one sample"));
        }
        let mut t = state.t_ms;
        for s in &ev.samples {
            if !(s.pos[0].is_finite() && s.pos[1].is_finite()) {
                return Err(broken("a sample is at a place that is not a number"));
            }
            if !(0.0..=1.0).contains(&s.pressure) {
                return Err(broken("a pressure is from 0 to 1"));
            }
            if !s.t_ms.is_finite() || s.t_ms < 0.0 {
                return Err(broken("a time is a number of milliseconds, not before the clock began"));
            }
            if s.t_ms < t {
                return Err(broken("a sample is older than the one before it"));
            }
            t = s.t_ms;
        }
        state.t_ms = t;
        if !ev.contact_px.is_finite() || ev.contact_px < 0.0 {
            return Err(broken("a contact size is a size"));
        }
        match ev.phase {
            Phase::Down => {
                if state.down {
                    return Err(broken("a pointer that is down goes down again (an up was missed)"));
                }
                state.down = true;
            }
            Phase::Move => {
                if !state.down {
                    return Err(broken("a pointer moves with contact before it went down"));
                }
                if ev.device == Device::Pen && !ev.pen.contact {
                    return Err(broken("a pen that moves with contact is in contact"));
                }
            }
            Phase::Up | Phase::Cancel => {
                if !state.down && ev.phase == Phase::Up {
                    return Err(broken("a pointer comes up that was not down"));
                }
                state.down = false;
            }
            Phase::Hover | Phase::Leave => {
                if state.down {
                    return Err(broken("a pointer that is down hovers or leaves (it came up first)"));
                }
                if ev.device == Device::Touch {
                    return Err(broken("a finger does not hover"));
                }
                if ev.phase == Phase::Hover && ev.pen.contact {
                    return Err(broken("a pen that hovers is not in contact"));
                }
            }
        }
        Ok(())
    }

    /// Whether any pointer is still down (a stream that ends so has an up missing).
    pub fn all_up(&self) -> bool {
        self.pointers.values().all(|t| !t.down)
    }
}

/// Check a whole stream; it must also end with every pointer up.
pub fn check(events: &[PointerEvent]) -> Result<(), Broken> {
    let mut checker = Checker::default();
    for ev in events {
        checker.next(ev)?;
    }
    if checker.all_up() { Ok(()) } else { Err(Broken { at: events.len(), rule: "the stream ends with a pointer still down".to_string() }) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{PenButtons, Sample};

    const IN_CONTACT: PenButtons = PenButtons { in_range: true, contact: true, eraser: false, barrel: false };
    const ABOVE: PenButtons = PenButtons { in_range: true, contact: false, eraser: false, barrel: false };

    fn event(device: Device, id: u32, phase: Phase, pen: PenButtons, samples: &[(f32, f32, f32, f64)]) -> PointerEvent {
        PointerEvent {
            device,
            id,
            phase,
            samples: samples.iter().map(|&(x, y, pressure, t_ms)| Sample { pos: [x, y], pressure, t_ms }).collect(),
            pen,
            contact_px: 0.0,
        }
    }

    fn pen(phase: Phase, pen: PenButtons, samples: &[(f32, f32, f32, f64)]) -> PointerEvent {
        event(Device::Pen, 1, phase, pen, samples)
    }

    /// A pen coming in, drawing, and going away.
    pub(crate) fn a_stroke() -> Vec<PointerEvent> {
        vec![
            pen(Phase::Hover, ABOVE, &[(10.0, 10.0, 0.0, 1.0)]),
            pen(Phase::Down, IN_CONTACT, &[(12.0, 10.0, 0.3, 2.0)]),
            pen(Phase::Move, IN_CONTACT, &[(14.0, 11.0, 0.5, 3.0), (16.0, 12.0, 0.6, 4.0), (18.0, 12.0, 0.6, 4.0)]),
            pen(Phase::Up, PenButtons { contact: false, ..IN_CONTACT }, &[(18.0, 12.0, 0.0, 5.0)]),
            pen(Phase::Leave, ABOVE, &[]),
        ]
    }

    fn broken_at(events: Vec<PointerEvent>) -> (usize, String) {
        let b = check(&events).expect_err("a rule is broken");
        (b.at, b.rule)
    }

    #[test]
    fn a_stroke_that_keeps_the_rules_passes_and_so_does_a_finger_and_two_pointers_at_once() {
        assert_eq!(check(&a_stroke()), Ok(()));
        let finger = |id, phase, t| event(Device::Touch, id, phase, PenButtons::default(), &[(5.0, 5.0, 1.0, t)]);
        let two = vec![
            finger(1, Phase::Down, 1.0),
            finger(2, Phase::Down, 1.5),
            finger(1, Phase::Move, 2.0),
            finger(2, Phase::Up, 2.5),
            finger(1, Phase::Up, 3.0),
        ];
        assert_eq!(check(&two), Ok(()));
        assert_eq!(check(&[]), Ok(()), "nothing is fine");
        // Time is kept for each pointer: the second finger may be older than the first's last sample.
        let apart = vec![finger(1, Phase::Down, 10.0), finger(2, Phase::Down, 5.0), finger(2, Phase::Up, 6.0), finger(1, Phase::Up, 11.0)];
        assert_eq!(check(&apart), Ok(()));
    }

    #[test]
    fn a_pointer_that_moves_or_comes_up_before_it_went_down_or_goes_down_twice_is_caught() {
        let mut s = a_stroke();
        s.remove(1);
        assert_eq!(broken_at(s).0, 1, "a move before the down");
        let mut s = a_stroke();
        s.insert(2, s[1].clone());
        assert_eq!(broken_at(s).0, 2, "down twice");
        let mut s = a_stroke();
        s.remove(2);
        s.remove(1);
        assert_eq!(broken_at(s).0, 1, "an up that was never down");
        let mut s = a_stroke();
        s.truncate(3);
        assert_eq!(broken_at(s).1, "the stream ends with a pointer still down");
        let mut s = a_stroke();
        s.insert(3, pen(Phase::Hover, ABOVE, &[(1.0, 1.0, 0.0, 4.5)]));
        assert_eq!(broken_at(s).0, 3, "hovering while down");
    }

    #[test]
    fn a_cancelled_press_ends_and_a_new_one_may_follow() {
        let mut s = a_stroke();
        s[3] = pen(Phase::Cancel, IN_CONTACT, &[]);
        s.push(pen(Phase::Down, IN_CONTACT, &[(1.0, 1.0, 0.5, 9.0)]));
        s.push(pen(Phase::Up, ABOVE, &[(1.0, 1.0, 0.0, 9.5)]));
        assert_eq!(check(&s), Ok(()));
    }

    #[test]
    fn samples_that_are_not_places_or_pressures_or_times_are_caught() {
        for (bad, why) in [
            ((f32::NAN, 1.0, 0.5, 3.0), "place"),
            ((1.0, f32::INFINITY, 0.5, 3.0), "place"),
            ((1.0, 1.0, 1.5, 3.0), "pressure"),
            ((1.0, 1.0, -0.1, 3.0), "pressure"),
            ((1.0, 1.0, f32::NAN, 3.0), "pressure"),
            ((1.0, 1.0, 0.5, f64::NAN), "time"),
            ((1.0, 1.0, 0.5, -1.0), "time"),
            ((1.0, 1.0, 0.5, 1.0), "older"),
        ] {
            let mut s = a_stroke();
            s[2] = pen(Phase::Move, IN_CONTACT, &[bad]);
            let (at, rule) = broken_at(s);
            assert_eq!(at, 2);
            assert!(rule.contains(why), "{bad:?}: {rule}");
        }
        // Samples inside one event must be in time order too.
        let mut s = a_stroke();
        s[2] = pen(Phase::Move, IN_CONTACT, &[(1.0, 1.0, 0.5, 4.0), (2.0, 1.0, 0.5, 3.5)]);
        assert!(broken_at(s).1.contains("older"));
        // An event that carries nothing is only a leave or a cancel.
        let mut s = a_stroke();
        s[2] = pen(Phase::Move, IN_CONTACT, &[]);
        assert!(broken_at(s).1.contains("at least one sample"));
    }

    #[test]
    fn the_buttons_say_the_same_as_the_phase_and_a_finger_does_not_hover() {
        let mut s = a_stroke();
        s[2] = pen(Phase::Move, ABOVE, &[(14.0, 11.0, 0.5, 3.0)]);
        assert!(broken_at(s).1.contains("in contact"));
        let mut s = a_stroke();
        s[0] = pen(Phase::Hover, IN_CONTACT, &[(10.0, 10.0, 0.0, 1.0)]);
        assert!(broken_at(s).1.contains("hovers is not in contact"));
        let hover = event(Device::Touch, 3, Phase::Hover, PenButtons::default(), &[(1.0, 1.0, 1.0, 1.0)]);
        assert!(broken_at(vec![hover]).1.contains("finger"));
        let mut s = a_stroke();
        s[1].contact_px = f32::NAN;
        assert!(broken_at(s).1.contains("size"));
    }

    #[test]
    fn a_checker_given_the_stream_a_piece_at_a_time_says_the_same_as_the_whole() {
        let stroke = a_stroke();
        let mut checker = Checker::default();
        for ev in &stroke {
            assert_eq!(checker.next(ev), Ok(()));
        }
        assert!(checker.all_up());
        let mut checker = Checker::default();
        checker.next(&stroke[0]).unwrap();
        assert_eq!(checker.next(&stroke[2]).expect_err("move before down").at, 1, "the index counts the events it was given");
    }
}
