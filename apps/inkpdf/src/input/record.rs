//! Recording and replaying a session's input. The program's input vocabulary ([`InputEvent`]) is closed and owns its data, so a
//! session is a list of events with the time each happened, written as text (one line per event) and read back: the same stream
//! gives the same document on every OS, a bug seen on one machine can be played on another, and a latency run needs no display
//! input at all.
//!
//! ```text
//! inkpdf-input 1
//! 1204.500 ptr pen 1 down 3 0 1 412.5 300.25 0.5 1204.5
//! 1212.000 mouse 100.5 200
//! 1300.000 key text 68656c6c6f
//! ```
//!
//! Times are milliseconds on the [`crate::clock`]; a pointer's samples carry their own, on the same clock.

use super::{ButtonState, Device, Dir, InputEvent, KeyInput, MouseButton, PenButtons, Phase, PointerEvent, Sample, Wheel};

/// The first line of a recording.
const HEADER: &str = "inkpdf-input 1";

/// One event, and when it was given to the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct Recorded {
    pub at_ms: f64,
    pub event: InputEvent,
}

impl Recorded {
    /// The same event `delta_ms` later, its samples too (for playing a recording against the clock of another run).
    pub fn shifted(&self, delta_ms: f64) -> Recorded {
        let mut event = self.event.clone();
        if let InputEvent::Pointer(p) = &mut event {
            p.samples.iter_mut().for_each(|s| s.t_ms += delta_ms);
        }
        Recorded { at_ms: self.at_ms + delta_ms, event }
    }
}

/// The session as text.
pub fn encode(events: &[Recorded]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for r in events {
        out.push_str(&line(r));
        out.push('\n');
    }
    out
}

fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<String> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

fn dev(d: Device) -> &'static str {
    match d {
        Device::Mouse => "mouse",
        Device::Touch => "touch",
        Device::Pen => "pen",
    }
}

fn phase(p: Phase) -> &'static str {
    match p {
        Phase::Down => "down",
        Phase::Move => "move",
        Phase::Up => "up",
        Phase::Cancel => "cancel",
        Phase::Hover => "hover",
        Phase::Leave => "leave",
    }
}

fn dir(d: Dir) -> &'static str {
    match d {
        Dir::Left => "left",
        Dir::Right => "right",
        Dir::Up => "up",
        Dir::Down => "down",
    }
}

fn flags(p: PenButtons) -> u8 {
    u8::from(p.in_range) | u8::from(p.contact) << 1 | u8::from(p.eraser) << 2 | u8::from(p.barrel) << 3
}

fn line(r: &Recorded) -> String {
    let at = r.at_ms;
    match &r.event {
        InputEvent::Pointer(p) => {
            let mut s = format!("{at} ptr {} {} {} {} {} {}", dev(p.device), p.id, phase(p.phase), flags(p.pen), p.contact_px, p.samples.len());
            for m in &p.samples {
                s.push_str(&format!(" {} {} {} {}", m.pos[0], m.pos[1], m.pressure, m.t_ms));
            }
            s
        }
        InputEvent::MouseMoved([x, y]) => format!("{at} mouse {x} {y}"),
        InputEvent::MouseButton { state, button } => {
            let state = if *state == ButtonState::Pressed { "pressed" } else { "released" };
            let button = match button {
                MouseButton::Left => "left",
                MouseButton::Right => "right",
                MouseButton::Middle => "middle",
                MouseButton::Other => "other",
            };
            format!("{at} button {state} {button}")
        }
        InputEvent::MouseLeft => format!("{at} mouse-left"),
        InputEvent::Wheel(Wheel::Lines(x, y)) => format!("{at} wheel lines {x} {y}"),
        InputEvent::Wheel(Wheel::Pixels(x, y)) => format!("{at} wheel pixels {x} {y}"),
        InputEvent::Key(key) => match key {
            KeyInput::Text(t) => format!("{at} key text {}", hex(t)),
            KeyInput::Backspace => format!("{at} key backspace"),
            KeyInput::Delete => format!("{at} key delete"),
            KeyInput::Escape => format!("{at} key escape"),
            KeyInput::Arrow(d, shift) => format!("{at} key arrow {} {}", dir(*d), u8::from(*shift)),
            KeyInput::Home(shift) => format!("{at} key home {}", u8::from(*shift)),
            KeyInput::End(shift) => format!("{at} key end {}", u8::from(*shift)),
            KeyInput::Chord(c) => format!("{at} key chord {}", hex(&c.to_string())),
        },
        InputEvent::Shortcut { letter, shift } => format!("{at} shortcut {} {}", hex(letter), u8::from(*shift)),
        InputEvent::FocusLost => format!("{at} focus-lost"),
        InputEvent::Resized([w, h]) => format!("{at} resized {w} {h}"),
        InputEvent::ScaleChanged(s) => format!("{at} scale {s}"),
    }
}

/// A recording read back. An error names the line it is in.
pub fn decode(text: &str) -> Result<Vec<Recorded>, String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(HEADER) {
        return Err(format!("not an InkPDF input recording (it should begin with \"{HEADER}\")"));
    }
    lines
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| parse_line(l).ok_or_else(|| format!("line {}: cannot read \"{}\"", i + 2, l.trim())))
        .collect()
}

fn parse_line(line: &str) -> Option<Recorded> {
    let mut t = line.split_whitespace();
    let at_ms: f64 = t.next()?.parse().ok()?;
    let kind = t.next()?;
    let mut num = || -> Option<f32> { t.next()?.parse().ok() };
    let event = match kind {
        "mouse" => InputEvent::MouseMoved([num()?, num()?]),
        "mouse-left" => InputEvent::MouseLeft,
        "focus-lost" => InputEvent::FocusLost,
        "resized" => InputEvent::Resized([num()?, num()?]),
        "scale" => InputEvent::ScaleChanged(num()?),
        "wheel" => {
            let pixels = t.next()? == "pixels";
            let (x, y) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?);
            InputEvent::Wheel(if pixels { Wheel::Pixels(x, y) } else { Wheel::Lines(x, y) })
        }
        "button" => {
            let state = if t.next()? == "pressed" { ButtonState::Pressed } else { ButtonState::Released };
            let button = match t.next()? {
                "left" => MouseButton::Left,
                "right" => MouseButton::Right,
                "middle" => MouseButton::Middle,
                "other" => MouseButton::Other,
                _ => return None,
            };
            InputEvent::MouseButton { state, button }
        }
        "shortcut" => InputEvent::Shortcut { letter: unhex(t.next()?)?, shift: t.next()? == "1" },
        "key" => InputEvent::Key(match t.next()? {
            "text" => KeyInput::Text(unhex(t.next()?)?),
            "backspace" => KeyInput::Backspace,
            "delete" => KeyInput::Delete,
            "escape" => KeyInput::Escape,
            "arrow" => {
                let d = match t.next()? {
                    "left" => Dir::Left,
                    "right" => Dir::Right,
                    "up" => Dir::Up,
                    "down" => Dir::Down,
                    _ => return None,
                };
                KeyInput::Arrow(d, t.next()? == "1")
            }
            "home" => KeyInput::Home(t.next()? == "1"),
            "end" => KeyInput::End(t.next()? == "1"),
            "chord" => KeyInput::Chord(unhex(t.next()?)?.chars().next()?),
            _ => return None,
        }),
        "ptr" => {
            let device = match t.next()? {
                "mouse" => Device::Mouse,
                "touch" => Device::Touch,
                "pen" => Device::Pen,
                _ => return None,
            };
            let id: u32 = t.next()?.parse().ok()?;
            let phase = match t.next()? {
                "down" => Phase::Down,
                "move" => Phase::Move,
                "up" => Phase::Up,
                "cancel" => Phase::Cancel,
                "hover" => Phase::Hover,
                "leave" => Phase::Leave,
                _ => return None,
            };
            let f: u8 = t.next()?.parse().ok()?;
            let pen = PenButtons { in_range: f & 1 != 0, contact: f & 2 != 0, eraser: f & 4 != 0, barrel: f & 8 != 0 };
            let contact_px: f32 = t.next()?.parse().ok()?;
            let n: usize = t.next()?.parse().ok()?;
            let mut samples = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let (x, y, pressure): (f32, f32, f32) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?, t.next()?.parse().ok()?);
                samples.push(Sample { pos: [x, y], pressure, t_ms: t.next()?.parse().ok()? });
            }
            InputEvent::Pointer(PointerEvent { device, id, phase, samples, pen, contact_px })
        }
        _ => return None,
    };
    Some(Recorded { at_ms, event })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pen_event() -> InputEvent {
        InputEvent::Pointer(PointerEvent {
            device: Device::Pen,
            id: 7,
            phase: Phase::Move,
            samples: vec![Sample { pos: [10.5, 20.25], pressure: 0.3, t_ms: 100.125 }, Sample { pos: [11.0, 21.0], pressure: 0.35, t_ms: 101.0 }],
            pen: PenButtons { in_range: true, contact: true, eraser: false, barrel: true },
            contact_px: 0.0,
        })
    }

    fn every_kind() -> Vec<Recorded> {
        let events = vec![
            pen_event(),
            InputEvent::MouseMoved([1.5, 2.5]),
            InputEvent::MouseButton { state: ButtonState::Pressed, button: MouseButton::Left },
            InputEvent::MouseButton { state: ButtonState::Released, button: MouseButton::Other },
            InputEvent::MouseLeft,
            InputEvent::Wheel(Wheel::Lines(0.0, -3.0)),
            InputEvent::Wheel(Wheel::Pixels(4.5, 1.0)),
            InputEvent::Key(KeyInput::Text("héllo\nwörld".into())),
            InputEvent::Key(KeyInput::Backspace),
            InputEvent::Key(KeyInput::Delete),
            InputEvent::Key(KeyInput::Escape),
            InputEvent::Key(KeyInput::Arrow(Dir::Up, true)),
            InputEvent::Key(KeyInput::Home(false)),
            InputEvent::Key(KeyInput::End(true)),
            InputEvent::Key(KeyInput::Chord('v')),
            InputEvent::Shortcut { letter: "z".into(), shift: true },
            InputEvent::FocusLost,
            InputEvent::Resized([1100.0, 900.0]),
            InputEvent::ScaleChanged(1.25),
        ];
        events.into_iter().enumerate().map(|(i, event)| Recorded { at_ms: 1000.0 + i as f64 * 0.1, event }).collect()
    }

    #[test]
    fn every_kind_of_event_is_read_back_exactly() {
        let events = every_kind();
        assert_eq!(decode(&encode(&events)), Ok(events));
    }

    #[test]
    fn floats_come_back_to_the_last_bit() {
        let tricky = Recorded {
            at_ms: 0.1 + 0.2,
            event: InputEvent::Pointer(PointerEvent {
                device: Device::Touch,
                id: 1,
                phase: Phase::Down,
                samples: vec![Sample { pos: [1.0 / 3.0, f32::MIN_POSITIVE], pressure: 0.1, t_ms: 1e15 + 0.5 }],
                pen: PenButtons::default(),
                contact_px: 28.7,
            }),
        };
        assert_eq!(decode(&encode(std::slice::from_ref(&tricky))), Ok(vec![tricky]));
    }

    #[test]
    fn what_is_not_a_recording_or_is_damaged_says_where() {
        assert!(decode("hello").unwrap_err().contains("not an InkPDF input recording"));
        let bad = format!("{HEADER}\n1.0 mouse 1 2\n2.0 mouse one two\n");
        assert!(decode(&bad).unwrap_err().starts_with("line 3:"));
        assert!(decode(&format!("{HEADER}\n1.0 teleport 3\n")).is_err());
        assert!(decode(&format!("{HEADER}\n1.0 key text 6\n")).is_err(), "odd hex");
        assert_eq!(decode(&format!("{HEADER}\n\n")), Ok(vec![]));
    }

    #[test]
    fn shifting_moves_the_samples_with_the_event() {
        let r = Recorded { at_ms: 10.0, event: pen_event() };
        let s = r.shifted(5.0);
        assert_eq!(s.at_ms, 15.0);
        let InputEvent::Pointer(p) = &s.event else { panic!("a pointer") };
        assert_eq!((p.samples[0].t_ms, p.samples[1].t_ms), (105.125, 106.0));
    }
}
