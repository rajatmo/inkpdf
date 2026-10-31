//! What a Wayland tablet tool says (`zwp_tablet_tool_v2`), turned into the program's pointer events: a pure state machine, so that it is tested on every OS
//! with scripted lists of what the compositor sends, and the part that talks to a compositor (`linux/tablet.rs`) only has to hand it what arrives.
//!
//! The protocol sends a tool's news as separate small events (it came near, it moved, its pressure changed, a button went down) and ends each group with
//! `frame`: what happened together happened at one moment. So events are held until the frame and then become the pen's events: a hover when the pen comes
//! near, a press, the moves of a stroke (frames that follow one another between two takes of the frame loop are one event with several samples: the
//! batch the OS gave, kept), a lift, and a leave when it goes. The stream keeps the rules every source keeps (`input::check`), whatever order the
//! compositor's events come in.

use crate::input::{Device, PenButtons, Phase, PointerEvent, Sample};

/// The button numbers of the pen's side buttons (Linux's `BTN_STYLUS` and `BTN_STYLUS2`): both are the barrel button here.
const BTN_STYLUS: u32 = 0x14b;
const BTN_STYLUS2: u32 = 0x14c;
/// The pen's one pointer id (a pen and a touch screen number their pointers apart: a pointer is told by its device and its id).
const PEN: u32 = 1;
/// The most a pressure event can say (the protocol's range is 0 to 65535).
const FULL_PRESSURE: f32 = 65_535.0;

/// What the compositor said, as far as the program cares.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code, reason = "Wayland's: tested on every OS, built into the reader only on Linux")]
pub enum Raw {
    /// The tool came near the screen over this window (`eraser`: the eraser end is the one in use).
    ProximityIn { eraser: bool },
    /// The tool went away.
    ProximityOut,
    /// The tip touched.
    Down,
    /// The tip lifted.
    Up,
    /// A place, in the window's logical pixels.
    Motion { x: f32, y: f32 },
    /// A pressure, 0 to 65535.
    Pressure(u32),
    /// A button of the pen's, by its Linux number.
    Button { code: u32, pressed: bool },
    /// The end of a group of events; `ms` is the compositor's clock (which the program's is not: only the differences matter).
    Frame { ms: u32 },
}

/// What has happened since the last frame.
#[derive(Clone, Copy, Debug, Default)]
struct Since {
    came: bool,
    eraser: bool,
    went: bool,
    down: bool,
    up: bool,
    moved: bool,
    pressed: bool,
    button: bool,
}

#[derive(Debug)]
#[allow(dead_code, reason = "Wayland's: tested on every OS, built into the reader only on Linux")]
pub struct Frames {
    /// Device pixels per logical pixel of the window.
    scale: f32,
    since: Since,
    in_range: bool,
    contact: bool,
    eraser: bool,
    barrel: bool,
    /// Where the pen is, in device pixels, and its pressure, 0 to 1.
    pos: [f32; 2],
    pressure: f32,
    /// The compositor's clock and the program's at the last frame: the program's time of a frame is the last one's plus the compositor's difference.
    clock: Option<(u32, f64)>,
    out: Vec<PointerEvent>,
}

#[allow(dead_code, reason = "Wayland's: tested on every OS, built into the reader only on Linux")]
impl Frames {
    pub fn new(scale: f32) -> Frames {
        Frames {
            scale: if scale.is_finite() && scale > 0.0 { scale } else { 1.0 },
            since: Since::default(),
            in_range: false,
            contact: false,
            eraser: false,
            barrel: false,
            pos: [0.0; 2],
            pressure: 0.0,
            clock: None,
            out: Vec::new(),
        }
    }

    /// The window's scale changed (logical pixels become this many device pixels).
    pub fn set_scale(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            self.scale = scale;
        }
    }

    /// Whether any events are waiting to be taken.
    pub fn pending(&self) -> bool {
        !self.out.is_empty()
    }

    /// Take the events made so far, appending them to `out` (the caller keeps its room).
    pub fn drain_into(&mut self, out: &mut Vec<PointerEvent>) {
        out.append(&mut self.out);
    }

    /// The compositor said `raw`; `now_ms` is the program's clock (see [`crate::clock`]).
    pub fn raw(&mut self, raw: Raw, now_ms: f64) {
        match raw {
            Raw::ProximityIn { eraser } => {
                self.since.came = true;
                self.since.eraser = eraser;
            }
            Raw::ProximityOut => self.since.went = true,
            Raw::Down => self.since.down = true,
            Raw::Up => self.since.up = true,
            Raw::Motion { x, y } => {
                if x.is_finite() && y.is_finite() {
                    self.pos = [x * self.scale, y * self.scale];
                    self.since.moved = true;
                }
            }
            Raw::Pressure(p) => {
                self.pressure = (p as f32 / FULL_PRESSURE).clamp(0.0, 1.0);
                self.since.pressed = true;
            }
            Raw::Button { code, pressed } if code == BTN_STYLUS || code == BTN_STYLUS2 => {
                self.barrel = pressed;
                self.since.button = true;
            }
            Raw::Button { .. } => {}
            Raw::Frame { ms } => self.frame(ms, now_ms),
        }
    }

    /// The program's time for a frame the compositor stamped `ms`: the last one's plus how much later the compositor says this is (its clock wraps, and
    /// is not the program's), never before the last and never after now.
    fn time(&mut self, ms: u32, now: f64) -> f64 {
        let t = match self.clock {
            None => now,
            Some((last_ms, last_t)) => (last_t + f64::from(ms.wrapping_sub(last_ms) as i32)).clamp(last_t, now.max(last_t)),
        };
        self.clock = Some((ms, t));
        t
    }

    fn buttons(&self) -> PenButtons {
        PenButtons { in_range: self.in_range, contact: self.contact, eraser: self.eraser, barrel: self.barrel }
    }

    fn push(&mut self, phase: Phase, t_ms: f64) {
        let sample = Sample { pos: self.pos, pressure: if self.contact { self.pressure } else { 0.0 }, t_ms };
        let pen = self.buttons();
        // (Moves that follow one another are one event with their samples: what the frame loop takes at once is what was batched.)
        if phase == Phase::Move
            && let Some(last) = self.out.last_mut()
            && last.phase == Phase::Move
            && last.pen == pen
        {
            last.samples.push(sample);
            return;
        }
        let samples = if phase == Phase::Leave { Vec::new() } else { vec![sample] };
        self.out.push(PointerEvent { device: Device::Pen, id: PEN, phase, samples, pen, contact_px: 0.0 });
    }

    fn frame(&mut self, ms: u32, now: f64) {
        let was = std::mem::take(&mut self.since);
        let t = self.time(ms, now);
        // A tool that says it is down or moving without having said it came near is taken as near (a compositor that skips what it implies).
        let near = was.came || (!self.in_range && (was.down || was.moved) && !was.went);
        if was.came {
            self.eraser = was.eraser;
        }
        if near && !self.in_range {
            self.in_range = true;
            self.push(Phase::Hover, t);
        }
        if !self.in_range {
            return;
        }
        if was.down && !self.contact {
            self.contact = true;
            self.push(Phase::Down, t);
        } else if self.contact && (was.moved || was.pressed || was.button) {
            self.push(Phase::Move, t);
        } else if !self.contact && (was.moved || was.button) && !was.down && !was.came {
            self.push(Phase::Hover, t);
        }
        if (was.up || was.went) && self.contact {
            self.pressure = 0.0;
            self.contact = false;
            self.push(Phase::Up, t);
        }
        if was.went {
            self.in_range = false;
            self.barrel = false;
            self.push(Phase::Leave, t);
        }
    }

    /// The reader is being put away: a pen that is down is lifted and one that is near leaves, so that nothing is left held.
    pub fn finish(&mut self, now_ms: f64) {
        let t = self.clock.map_or(now_ms, |(_, t)| t.max(now_ms));
        if self.contact {
            self.contact = false;
            self.push(Phase::Up, t);
        }
        if self.in_range {
            self.in_range = false;
            self.push(Phase::Leave, t);
        }
        self.since = Since::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input;

    /// Feed `raws`, each group ending with a frame stamped `ms`, and take what comes out.
    fn run(frames: &mut Frames, groups: &[(&[Raw], u32)], now: f64) -> Vec<PointerEvent> {
        for (raws, ms) in groups {
            for raw in *raws {
                frames.raw(*raw, now);
            }
            frames.raw(Raw::Frame { ms: *ms }, now);
        }
        let mut out = Vec::new();
        frames.drain_into(&mut out);
        out
    }

    fn at(x: f32, y: f32) -> Raw {
        Raw::Motion { x, y }
    }

    #[test]
    fn a_pen_that_comes_near_touches_draws_lifts_and_goes_makes_a_hover_a_press_moves_a_lift_and_a_leave() {
        let mut f = Frames::new(2.0);
        let events = run(
            &mut f,
            &[
                (&[Raw::ProximityIn { eraser: false }, at(10.0, 20.0)], 1000),
                (&[at(11.0, 21.0)], 1008),
                (&[Raw::Down, Raw::Pressure(32_768), at(12.0, 22.0)], 1016),
                (&[at(13.0, 23.0), Raw::Pressure(65_535)], 1024),
                (&[at(14.0, 24.0)], 1032),
                (&[Raw::Up], 1040),
                (&[at(15.0, 25.0)], 1048),
                (&[Raw::ProximityOut], 1056),
            ],
            5000.0,
        );
        let phases: Vec<Phase> = events.iter().map(|e| e.phase).collect();
        assert_eq!(phases, [Phase::Hover, Phase::Hover, Phase::Down, Phase::Move, Phase::Up, Phase::Hover, Phase::Leave], "{events:?}");
        assert_eq!(events[0].samples[0].pos, [20.0, 40.0], "logical pixels times the scale");
        assert!(events[0].pen.in_range && !events[0].pen.contact && events[0].samples[0].pressure == 0.0);
        assert!(events[2].pen.contact && (events[2].samples[0].pressure - 0.5).abs() < 1e-3, "pressure over 65535");
        assert_eq!(events[3].samples.len(), 2, "the two frames of the stroke that followed one another are one event");
        assert_eq!(events[3].samples[1].pressure, 1.0);
        assert!(!events[4].pen.contact && events[4].samples[0].pressure == 0.0, "lifted, with no pressure");
        assert!(events[6].samples.is_empty() && events.iter().all(|e| e.device == Device::Pen && e.id == PEN));
        assert_eq!(input::check(&events), Ok(()));
    }

    #[test]
    fn the_eraser_end_and_the_side_buttons_are_the_pens_flags_and_they_are_let_go_with_the_pen() {
        let mut f = Frames::new(1.0);
        let events = run(
            &mut f,
            &[
                (&[Raw::ProximityIn { eraser: true }, at(1.0, 1.0)], 10),
                (&[Raw::Down], 20),
                (&[Raw::Button { code: BTN_STYLUS, pressed: true }, at(2.0, 2.0)], 30),
                (&[Raw::Button { code: 0x110, pressed: true }], 40),
                (&[Raw::Up], 50),
                (&[Raw::ProximityOut], 60),
                (&[Raw::ProximityIn { eraser: false }, at(3.0, 3.0)], 70),
            ],
            1000.0,
        );
        assert!(events.iter().take(5).all(|e| e.pen.eraser), "the eraser end is in use from the moment it came near");
        assert!(events[2].pen.barrel && !events[1].pen.barrel, "the side button, from the frame it went down in");
        let last = events.last().expect("an event");
        assert_eq!((last.phase, last.pen.eraser, last.pen.barrel), (Phase::Hover, false, false), "the other end, nothing held");
        assert_eq!(input::check(&events), Ok(()));
    }

    #[test]
    fn a_pen_that_goes_away_while_it_touches_is_lifted_first_and_a_tool_that_skips_what_it_implies_is_taken_as_near() {
        let mut f = Frames::new(1.0);
        let events = run(&mut f, &[(&[Raw::ProximityIn { eraser: false }, at(1.0, 1.0)], 1), (&[Raw::Down], 2), (&[Raw::ProximityOut], 3)], 100.0);
        assert_eq!(events.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Hover, Phase::Down, Phase::Up, Phase::Leave]);
        assert_eq!(input::check(&events), Ok(()));
        let mut skipping = Frames::new(1.0);
        let events = run(&mut skipping, &[(&[at(5.0, 5.0), Raw::Down], 1), (&[at(6.0, 6.0)], 2), (&[Raw::Up], 3)], 100.0);
        assert_eq!(events.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Hover, Phase::Down, Phase::Move, Phase::Up]);
        assert_eq!(input::check(&events), Ok(()));
        // Nothing before it came near, and nothing after it went, makes anything.
        let mut quiet = Frames::new(1.0);
        assert!(run(&mut quiet, &[(&[Raw::Up], 1), (&[Raw::ProximityOut], 2), (&[Raw::Pressure(9)], 3)], 100.0).is_empty());
        assert!(!quiet.pending());
    }

    #[test]
    fn time_goes_forward_with_the_compositors_differences_never_back_never_past_now_and_across_the_wrap_of_its_clock() {
        let mut f = Frames::new(1.0);
        let groups: Vec<(Vec<Raw>, u32)> = vec![
            (vec![Raw::ProximityIn { eraser: false }, at(0.0, 0.0)], u32::MAX - 10),
            (vec![Raw::Down], u32::MAX - 2),
            (vec![at(1.0, 1.0)], 6),
            (vec![at(2.0, 2.0)], 3),
            (vec![Raw::Up], 20_000),
        ];
        let borrowed: Vec<(&[Raw], u32)> = groups.iter().map(|(r, ms)| (r.as_slice(), *ms)).collect();
        let events = run(&mut f, &borrowed, 1_000.0);
        let times: Vec<f64> = events.iter().flat_map(|e| e.samples.iter().map(|s| s.t_ms)).collect();
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
        assert!(times.iter().all(|t| (0.0..=1_000.0).contains(t)), "{times:?}");
        assert_eq!(input::check(&events), Ok(()));
    }

    #[test]
    fn bad_numbers_and_a_scale_that_changes_are_survived_and_finishing_lets_go_of_everything() {
        let mut f = Frames::new(f32::NAN);
        let events = run(
            &mut f,
            &[
                (&[Raw::ProximityIn { eraser: false }, at(f32::NAN, 1.0), at(f32::INFINITY, 0.0), Raw::Pressure(u32::MAX)], 1),
                (&[Raw::Down, at(4.0, 4.0)], 2),
            ],
            50.0,
        );
        assert!(events.iter().all(|e| e.samples.iter().all(|s| s.pos.iter().all(|v| v.is_finite()) && (0.0..=1.0).contains(&s.pressure))));
        f.set_scale(3.0);
        f.set_scale(f32::NAN);
        f.set_scale(-1.0);
        f.raw(at(1.0, 1.0), 60.0);
        f.raw(Raw::Frame { ms: 3 }, 60.0);
        let mut more = Vec::new();
        f.drain_into(&mut more);
        assert_eq!(more[0].samples[0].pos, [3.0, 3.0], "the scale that was set stays, the bad ones are not taken");
        f.finish(70.0);
        let mut rest = Vec::new();
        f.drain_into(&mut rest);
        assert_eq!(rest.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Up, Phase::Leave]);
        let all: Vec<PointerEvent> = events.into_iter().chain(more).chain(rest).collect();
        assert_eq!(input::check(&all), Ok(()));
        f.finish(80.0);
        assert!(!f.pending(), "finishing twice lets go of nothing more");
    }

    #[test]
    fn whatever_order_the_compositor_sends_things_in_the_stream_keeps_the_rules_of_a_stream() {
        let mut seed = 0x9e37_79b9_u32;
        let mut next = move |n: u32| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) % n
        };
        for round in 0..200 {
            let mut f = Frames::new(1.0 + (round % 3) as f32 * 0.5);
            let mut events = Vec::new();
            let mut ms = next(1000);
            for _ in 0..400 {
                let raw = match next(9) {
                    0 => Raw::ProximityIn { eraser: next(2) == 0 },
                    1 => Raw::ProximityOut,
                    2 => Raw::Down,
                    3 => Raw::Up,
                    4 | 5 => Raw::Motion { x: next(2000) as f32 / 3.0 - 100.0, y: next(2000) as f32 / 3.0 - 100.0 },
                    6 => Raw::Pressure(next(80_000)),
                    7 => Raw::Button { code: [BTN_STYLUS, BTN_STYLUS2, 0x110][next(3) as usize], pressed: next(2) == 0 },
                    _ => {
                        ms = ms.wrapping_add(next(40));
                        Raw::Frame { ms }
                    }
                };
                f.raw(raw, 10_000.0 + round as f64);
                if next(5) == 0 {
                    f.drain_into(&mut events);
                }
            }
            f.finish(20_000.0);
            f.drain_into(&mut events);
            assert_eq!(input::check(&events), Ok(()), "round {round}");
        }
    }
}
