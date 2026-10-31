//! Turning what a reader of motion events gets (Android's `MotionEvent` and `KeyEvent` are the shape; other hosts that hand
//! over a gesture as one event with its pointers, a tool type and a history of points are alike) into the program's own
//! [`InputEvent`]s. It is plain data in and plain data out, so it is tested here whatever the platform; the platform's reader only
//! copies what its toolkit says into [`RawMotion`] and [`RawKey`].
//!
//! What it knows that the toolkit's touch events do not carry: **which tool it is** (a stylus draws, a finger pans and zooms, an
//! eraser end erases, a palm is rejected, a mouse is a mouse), the **stylus's buttons**, **hover** (a pen above the screen), and the
//! **history** (the points a pen reported between two events, which is what keeps a fast stroke smooth).

use std::collections::HashMap;

use crate::input::{ButtonState, Device, InputEvent, KeyInput, MouseButton, PenButtons, Phase, PointerEvent, Sample};

/// What a pointer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Finger,
    Stylus,
    /// The eraser end of a stylus (or a stylus held inverted).
    Eraser,
    Mouse,
    /// A palm the system has recognised: never a stroke.
    Palm,
    Unknown,
}

/// What a motion event reports happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The first pointer went down, or another did (`action_index` says which).
    Down,
    /// Pointers moved (all of them are in the event, with the points they passed through).
    Move,
    /// A pointer went up (`action_index` says which).
    Up,
    Cancel,
    /// A pen or mouse above the screen: in range, moving, gone.
    HoverEnter,
    HoverMove,
    HoverExit,
    /// Anything else (scroll, button presses of a mouse): left alone.
    Other,
}

/// A point a pointer passed through since the last event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HistoryPoint {
    pub pos: [f32; 2],
    pub pressure: f32,
    /// When, in nanoseconds on the monotonic clock the toolkit stamps with.
    pub t_ns: i64,
}

/// One pointer of a motion event.
#[derive(Clone, Debug, PartialEq)]
pub struct RawPointer {
    pub id: u32,
    pub tool: Tool,
    pub pos: [f32; 2],
    pub pressure: f32,
    /// The longest side of the contact, in pixels (0 if not known).
    pub contact_px: f32,
    /// What it passed through since the last event, oldest first.
    pub history: Vec<HistoryPoint>,
}

/// One motion event.
#[derive(Clone, Debug, PartialEq)]
pub struct RawMotion {
    pub action: Action,
    /// Which pointer of `pointers` a `Down` or `Up` is about.
    pub action_index: usize,
    /// When, in nanoseconds on the monotonic clock.
    pub t_ns: i64,
    /// The stylus's first button. On Android it is the one button an S Pen has, and it erases for a
    /// pen with no eraser end; the eraser end itself is `Tool::Eraser`.
    pub stylus_primary: bool,
    /// The stylus's second button: the barrel button, which lassos.
    pub stylus_secondary: bool,
    /// A mouse's left and right buttons.
    pub primary: bool,
    pub secondary: bool,
    pub pointers: Vec<RawPointer>,
}

/// What the decoder remembers of a pointer that is down.
#[derive(Clone, Copy, Debug)]
struct Down {
    device: Device,
    eraser: bool,
    /// The time of its last sample, so that what it reports never goes back.
    t_ms: f64,
    /// Where it was last, and how hard it pressed: what its cancel is stamped with, so a cancel says where it happened.
    pos: [f32; 2],
    pressure: f32,
}

/// A pointer is told by its device and its id: a stylus and a finger can have the same id (a pen digitiser and a touch screen number
/// their pointers apart), and neither is the other.
type Key = (Device, u32);

/// The decoder: it remembers which pointers are down and what they are.
#[derive(Debug, Default)]
pub struct Decoder {
    /// Milliseconds to take off the toolkit's clock to get the program's (see `clock`).
    offset_ms: f64,
    down: HashMap<Key, Down>,
    /// A pen above the screen, by id (so that it is told when it goes out of range).
    hovering: HashMap<u32, f64>,
    /// Whether the stylus has ever been seen hovering: only a screen that reports hover can say a lifted pen is still near.
    hover_capable: bool,
    mouse_button: Option<MouseButton>,
}

impl Decoder {
    /// A decoder for a toolkit whose clock reads `offset_ms` more than the program's.
    pub fn new(offset_ms: f64) -> Self {
        Self { offset_ms, ..Self::default() }
    }

    /// The program's time for the toolkit's `t_ns`.
    pub fn ms(&self, t_ns: i64) -> f64 {
        (t_ns as f64 / 1.0e6 - self.offset_ms).max(0.0)
    }

    /// Whether a pointer that is down is one the program was told of (its up is still to come).
    pub fn is_down(&self, id: u32) -> bool {
        self.device_of(id).is_some()
    }

    /// The device of a pointer that is down.
    pub fn device_of(&self, id: u32) -> Option<Device> {
        // (A finger first: the toolkit's own touch events, which this is asked for, cannot tell a stylus from a finger.)
        [Device::Touch, Device::Pen].into_iter().find(|&d| self.down.contains_key(&(d, id)))
    }

    fn sample(&self, pos: [f32; 2], pressure: f32, t_ns: i64, floor: f64) -> Sample {
        let pressure = if pressure.is_finite() { pressure.clamp(0.0, 1.0) } else { 0.5 };
        Sample { pos, pressure, t_ms: self.ms(t_ns).max(floor) }
    }

    /// What one motion event comes to.
    pub fn motion(&mut self, m: &RawMotion, out: &mut Vec<InputEvent>) {
        match m.action {
            Action::Down => {
                if let Some(p) = m.pointers.get(m.action_index) {
                    self.pointer_down(m, p, out);
                }
            }
            Action::Move => {
                for p in &m.pointers {
                    self.pointer_moved(m, p, out);
                }
            }
            Action::Up => {
                if let Some(p) = m.pointers.get(m.action_index) {
                    self.pointer_up(m, p, out);
                }
            }
            Action::Cancel => self.cancel_pointers(m, out),
            Action::HoverEnter | Action::HoverMove => {
                for p in &m.pointers {
                    self.hover(m, p, out);
                }
            }
            Action::HoverExit => {
                for p in &m.pointers {
                    self.hover_exit(m, p, out);
                }
            }
            Action::Other => {}
        }
    }

    fn device_for(tool: Tool) -> Option<Device> {
        match tool {
            Tool::Stylus | Tool::Eraser => Some(Device::Pen),
            Tool::Finger | Tool::Unknown => Some(Device::Touch),
            Tool::Mouse => Some(Device::Mouse),
            Tool::Palm => None,
        }
    }

    fn pen_buttons(m: &RawMotion, eraser: bool, contact: bool) -> PenButtons {
        PenButtons { in_range: true, contact, eraser: eraser || m.stylus_primary, barrel: m.stylus_secondary }
    }

    fn pointer_down(&mut self, m: &RawMotion, p: &RawPointer, out: &mut Vec<InputEvent>) {
        let Some(device) = Self::device_for(p.tool) else { return };
        if device == Device::Mouse {
            let button = if m.secondary && !m.primary { MouseButton::Right } else { MouseButton::Left };
            self.mouse_button = Some(button);
            out.push(InputEvent::MouseMoved(p.pos));
            out.push(InputEvent::MouseButton { state: ButtonState::Pressed, button });
            return;
        }
        // (A pointer that goes down twice missed its up: it is ended before it starts again.)
        if self.down.contains_key(&(device, p.id)) {
            self.cancel((device, p.id), out);
        }
        // A pen that was above the screen comes down: it is no longer only hovering.
        if device == Device::Pen {
            self.hovering.remove(&p.id);
        }
        let eraser = p.tool == Tool::Eraser;
        let first = self.sample(p.pos, p.pressure, m.t_ns, 0.0);
        self.down.insert((device, p.id), Down { device, eraser, t_ms: first.t_ms, pos: first.pos, pressure: first.pressure });
        let pen = if device == Device::Pen { Self::pen_buttons(m, eraser, true) } else { PenButtons::default() };
        let contact_px = if device == Device::Touch { p.contact_px.max(0.0) } else { 0.0 };
        out.push(InputEvent::Pointer(PointerEvent { device, id: p.id, phase: Phase::Down, samples: vec![first], pen, contact_px }));
    }

    fn pointer_moved(&mut self, m: &RawMotion, p: &RawPointer, out: &mut Vec<InputEvent>) {
        if p.tool == Tool::Mouse {
            out.push(InputEvent::MouseMoved(p.pos));
            return;
        }
        let Some(device) = Self::device_for(p.tool) else { return };
        let Some(state) = self.down.get(&(device, p.id)).copied() else { return };
        // The points it passed through, then where it is now: what a fast pen draws smoothly from.
        let mut floor = state.t_ms;
        let mut samples = Vec::with_capacity(p.history.len() + 1);
        for h in &p.history {
            let s = self.sample(h.pos, h.pressure, h.t_ns, floor);
            floor = s.t_ms;
            samples.push(s);
        }
        samples.push(self.sample(p.pos, p.pressure, m.t_ns, floor));
        if let (Some(d), Some(last)) = (self.down.get_mut(&(device, p.id)), samples.last()) {
            (d.t_ms, d.pos, d.pressure) = (last.t_ms, last.pos, last.pressure);
        }
        let pen = if state.device == Device::Pen { Self::pen_buttons(m, state.eraser, true) } else { PenButtons::default() };
        let contact_px = if state.device == Device::Touch { p.contact_px.max(0.0) } else { 0.0 };
        out.push(InputEvent::Pointer(PointerEvent { device: state.device, id: p.id, phase: Phase::Move, samples, pen, contact_px }));
    }

    fn pointer_up(&mut self, m: &RawMotion, p: &RawPointer, out: &mut Vec<InputEvent>) {
        if p.tool == Tool::Mouse {
            if let Some(button) = self.mouse_button.take() {
                out.push(InputEvent::MouseMoved(p.pos));
                out.push(InputEvent::MouseButton { state: ButtonState::Released, button });
            }
            return;
        }
        let Some(device) = Self::device_for(p.tool) else { return };
        let Some(state) = self.down.remove(&(device, p.id)) else { return };
        let last = self.sample(p.pos, p.pressure, m.t_ns, state.t_ms);
        // (A pen that lifts has no contact: its up says so, as a pointer message's does.)
        let pen = if state.device == Device::Pen {
            PenButtons { in_range: self.hover_capable, ..Self::pen_buttons(m, state.eraser, false) }
        } else {
            PenButtons::default()
        };
        out.push(InputEvent::Pointer(PointerEvent { device: state.device, id: p.id, phase: Phase::Up, samples: vec![last], pen, contact_px: 0.0 }));
    }

    fn cancel(&mut self, key: Key, out: &mut Vec<InputEvent>) {
        if let Some(state) = self.down.remove(&key) {
            let pen = if state.device == Device::Pen {
                PenButtons { in_range: true, contact: true, eraser: state.eraser, barrel: false }
            } else {
                PenButtons::default()
            };
            // (It carries where it was: whoever is told of a cancel needs a place to cancel at.)
            let at = Sample { pos: state.pos, pressure: state.pressure, t_ms: state.t_ms };
            out.push(InputEvent::Pointer(PointerEvent {
                device: state.device,
                id: key.1,
                phase: Phase::Cancel,
                samples: vec![at],
                pen,
                contact_px: 0.0,
            }));
        }
    }

    /// The system gave up on the pointers a motion event lists (a screen cancels its touches when a pen comes near): those, and not
    /// the pointers of another device. A cancel that lists none is for all.
    fn cancel_pointers(&mut self, m: &RawMotion, out: &mut Vec<InputEvent>) {
        let mut keys: Vec<Key> = m.pointers.iter().filter_map(|p| Some((Self::device_for(p.tool)?, p.id))).collect();
        if keys.is_empty() {
            return self.cancel_all(out);
        }
        keys.sort_unstable_by_key(|&(d, id)| (id, d as u8));
        for key in keys {
            self.cancel(key, out);
        }
    }

    fn cancel_all(&mut self, out: &mut Vec<InputEvent>) {
        let mut keys: Vec<Key> = self.down.keys().copied().collect();
        keys.sort_unstable_by_key(|&(d, id)| (id, d as u8));
        for key in keys {
            self.cancel(key, out);
        }
        if let Some(button) = self.mouse_button.take() {
            out.push(InputEvent::MouseButton { state: ButtonState::Released, button });
        }
    }

    fn hover(&mut self, m: &RawMotion, p: &RawPointer, out: &mut Vec<InputEvent>) {
        match p.tool {
            Tool::Mouse => out.push(InputEvent::MouseMoved(p.pos)),
            Tool::Stylus | Tool::Eraser => {
                if self.down.contains_key(&(Device::Pen, p.id)) {
                    return;
                }
                self.hover_capable = true;
                let floor = self.hovering.get(&p.id).copied().unwrap_or(0.0);
                let s = self.sample(p.pos, 0.0, m.t_ns, floor);
                self.hovering.insert(p.id, s.t_ms);
                out.push(InputEvent::Pointer(PointerEvent {
                    device: Device::Pen,
                    id: p.id,
                    phase: Phase::Hover,
                    samples: vec![s],
                    pen: Self::pen_buttons(m, p.tool == Tool::Eraser, false),
                    contact_px: 0.0,
                }));
            }
            // (A finger does not hover.)
            _ => {}
        }
    }

    fn hover_exit(&mut self, m: &RawMotion, p: &RawPointer, out: &mut Vec<InputEvent>) {
        match p.tool {
            Tool::Mouse => out.push(InputEvent::MouseLeft),
            Tool::Stylus | Tool::Eraser if self.hovering.remove(&p.id).is_some() => {
                out.push(InputEvent::Pointer(PointerEvent {
                    device: Device::Pen,
                    id: p.id,
                    phase: Phase::Leave,
                    samples: Vec::new(),
                    pen: Self::pen_buttons(m, p.tool == Tool::Eraser, false),
                    contact_px: 0.0,
                }));
            }
            _ => {}
        }
    }

    /// A touch that the toolkit's own event system reported after the reader had taken the rest of the queue (a few arrive in the
    /// moment between the two): it is a pointer like the others, and is whatever its id already is if it is down, else a finger.
    pub fn late_touch(&mut self, id: u32, phase: Phase, pos: [f32; 2], pressure: f32, t_ms: f64, out: &mut Vec<InputEvent>) {
        let device = self.device_of(id).unwrap_or(Device::Touch);
        let key = (device, id);
        let pressure = if pressure.is_finite() { pressure.clamp(0.0, 1.0) } else { 0.5 };
        match phase {
            Phase::Down => {
                if self.down.contains_key(&key) {
                    self.cancel(key, out);
                }
                self.down.insert(key, Down { device, eraser: false, t_ms, pos, pressure });
            }
            Phase::Move | Phase::Up => {
                let Some(state) = self.down.get_mut(&key) else { return };
                (state.t_ms, state.pos, state.pressure) = (state.t_ms.max(t_ms), pos, pressure);
            }
            Phase::Cancel => {
                self.cancel(key, out);
                return;
            }
            Phase::Hover | Phase::Leave => return,
        }
        let t_ms = self.down.get(&key).map_or(t_ms, |d| d.t_ms);
        if phase == Phase::Up {
            self.down.remove(&key);
        }
        let pen = if device == Device::Pen {
            PenButtons { in_range: phase != Phase::Up || self.hover_capable, contact: phase != Phase::Up, eraser: false, barrel: false }
        } else {
            PenButtons::default()
        };
        out.push(InputEvent::Pointer(PointerEvent { device, id, phase, samples: vec![Sample { pos, pressure, t_ms }], pen, contact_px: 0.0 }));
    }
}

// ── keys ─────────────────────────────────────────────────────────────────────────────────────

/// The keys the program has any use for, whatever the toolkit calls them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Backspace,
    Delete,
    Escape,
    /// The system's back key (and button): the program takes it as Escape, which puts away a card or ends an edit.
    Back,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// Any other: a letter or symbol, which `unicode` says, or nothing the program uses (volume, power, …).
    Other,
}

/// One key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawKey {
    pub code: KeyCode,
    /// A press (or a repeat of one); a release is not heard.
    pub down: bool,
    pub ctrl: bool,
    pub shift: bool,
    /// Alt is held: with an arrow, back and forward through the places a link took the reader from.
    pub alt: bool,
    /// The character the key makes with the modifiers held, if it makes one.
    pub unicode: Option<char>,
}

/// What a key comes to, and whether the program took it (`false`: it is for the system, as the volume keys are).
pub fn key(k: &RawKey, out: &mut Vec<InputEvent>) -> bool {
    let named = |key: KeyInput, out: &mut Vec<InputEvent>| {
        out.push(InputEvent::Key(key));
        true
    };
    if !k.down {
        // (A release of a key the program takes is taken too, so that the system does not act on half of a press.)
        return matches!(k.code, KeyCode::Back | KeyCode::Backspace | KeyCode::Delete | KeyCode::Escape | KeyCode::Enter);
    }
    match k.code {
        KeyCode::Backspace => named(KeyInput::Backspace, out),
        KeyCode::Delete => named(KeyInput::Delete, out),
        KeyCode::Escape | KeyCode::Back => named(KeyInput::Escape, out),
        KeyCode::Enter => named(KeyInput::Text("\n".to_string()), out),
        KeyCode::Left if k.alt => named(KeyInput::Trail(false), out),
        KeyCode::Right if k.alt => named(KeyInput::Trail(true), out),
        KeyCode::Left => named(KeyInput::Arrow(crate::input::Dir::Left, k.shift), out),
        KeyCode::Right => named(KeyInput::Arrow(crate::input::Dir::Right, k.shift), out),
        KeyCode::Up => named(KeyInput::Arrow(crate::input::Dir::Up, k.shift), out),
        KeyCode::Down => named(KeyInput::Arrow(crate::input::Dir::Down, k.shift), out),
        KeyCode::Home => named(KeyInput::Home(k.shift), out),
        KeyCode::End => named(KeyInput::End(k.shift), out),
        KeyCode::PageUp => named(KeyInput::Page(false), out),
        KeyCode::PageDown => named(KeyInput::Page(true), out),
        KeyCode::Other => match k.unicode.filter(|c| !c.is_control()) {
            Some(c) if k.ctrl => {
                out.push(InputEvent::Shortcut { letter: c.to_string(), shift: k.shift });
                true
            }
            Some(c) => named(KeyInput::Text(c.to_string()), out),
            None => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::check;

    const MS: i64 = 1_000_000;

    fn pointer(id: u32, tool: Tool, x: f32, y: f32, pressure: f32, history: &[(f32, f32, f32, i64)]) -> RawPointer {
        RawPointer {
            id,
            tool,
            pos: [x, y],
            pressure,
            contact_px: 0.0,
            history: history.iter().map(|&(x, y, pressure, t)| HistoryPoint { pos: [x, y], pressure, t_ns: t * MS }).collect(),
        }
    }

    fn motion(action: Action, index: usize, t_ms: i64, pointers: Vec<RawPointer>) -> RawMotion {
        RawMotion {
            action,
            action_index: index,
            t_ns: t_ms * MS,
            stylus_primary: false,
            stylus_secondary: false,
            primary: false,
            secondary: false,
            pointers,
        }
    }

    fn run(decoder: &mut Decoder, motions: &[RawMotion]) -> Vec<InputEvent> {
        let mut out = Vec::new();
        for m in motions {
            decoder.motion(m, &mut out);
        }
        out
    }

    fn pointers(events: &[InputEvent]) -> Vec<PointerEvent> {
        events.iter().filter_map(|e| if let InputEvent::Pointer(p) = e { Some(p.clone()) } else { None }).collect()
    }

    #[test]
    fn a_lifted_pen_is_in_range_only_on_a_screen_that_reports_hover() {
        let lifted = |events: &[RawMotion]| {
            let mut d = Decoder::new(0.0);
            let p = pointers(&run(&mut d, events));
            p.into_iter().find(|e| e.phase == Phase::Up).expect("an up").pen.in_range
        };
        let down = motion(Action::Down, 0, 10, vec![pointer(3, Tool::Stylus, 8.0, 8.0, 0.3, &[])]);
        let up = motion(Action::Up, 0, 20, vec![pointer(3, Tool::Stylus, 8.0, 8.0, 0.0, &[])]);
        let hover = motion(Action::HoverMove, 0, 5, vec![pointer(3, Tool::Stylus, 5.0, 5.0, 0.0, &[])]);
        assert!(lifted(&[hover, down.clone(), up.clone()]), "it hovered before, so it hovers after");
        assert!(!lifted(&[down, up]), "a screen that never reports hover cannot say the lifted pen is near: palm rejection must not wait for it");
    }

    #[test]
    fn a_stylus_is_a_pen_with_its_history_pressure_and_the_buttons_it_held_and_the_stream_keeps_the_rules() {
        let mut d = Decoder::new(0.0);
        let mut barrel =
            motion(Action::Move, 0, 30, vec![pointer(3, Tool::Stylus, 30.0, 12.0, 0.7, &[(10.0, 10.0, 0.4, 20), (20.0, 11.0, 0.5, 25)])]);
        barrel.stylus_secondary = true;
        let events = run(
            &mut d,
            &[
                motion(Action::HoverEnter, 0, 5, vec![pointer(3, Tool::Stylus, 5.0, 5.0, 0.0, &[])]),
                motion(Action::HoverMove, 0, 8, vec![pointer(3, Tool::Stylus, 6.0, 6.0, 0.0, &[])]),
                motion(Action::Down, 0, 10, vec![pointer(3, Tool::Stylus, 8.0, 8.0, 0.3, &[])]),
                barrel,
                motion(Action::Up, 0, 40, vec![pointer(3, Tool::Stylus, 31.0, 12.0, 0.0, &[])]),
                // (A pen that has lifted is above the screen again, until it goes out of range.)
                motion(Action::HoverMove, 0, 45, vec![pointer(3, Tool::Stylus, 32.0, 12.0, 0.0, &[])]),
                motion(Action::HoverExit, 0, 50, vec![pointer(3, Tool::Stylus, 32.0, 12.0, 0.0, &[])]),
            ],
        );
        let p = pointers(&events);
        assert_eq!(
            p.iter().map(|e| (e.device, e.phase)).collect::<Vec<_>>(),
            [
                (Device::Pen, Phase::Hover),
                (Device::Pen, Phase::Hover),
                (Device::Pen, Phase::Down),
                (Device::Pen, Phase::Move),
                (Device::Pen, Phase::Up),
                (Device::Pen, Phase::Hover),
                (Device::Pen, Phase::Leave),
            ]
        );
        let moved = &p[3];
        assert_eq!(moved.samples.iter().map(|s| s.pos[0]).collect::<Vec<_>>(), [10.0, 20.0, 30.0], "the points it passed through, then where it is");
        assert_eq!(moved.samples.iter().map(|s| s.t_ms).collect::<Vec<_>>(), [20.0, 25.0, 30.0]);
        assert!(moved.pen.contact && moved.pen.barrel && !moved.pen.eraser, "{:?}", moved.pen);
        assert!(!p[4].pen.contact, "it has lifted");
        assert_eq!(p[0].pen, PenButtons { in_range: true, contact: false, eraser: false, barrel: false });
        let stream: Vec<PointerEvent> = p;
        assert_eq!(check(&stream), Ok(()), "what it makes is a stream the editor can rely on");
    }

    #[test]
    fn a_finger_is_touch_with_its_contact_size_and_two_fingers_are_two_pointers_and_a_finger_does_not_hover() {
        let mut d = Decoder::new(0.0);
        let mut f1 = pointer(0, Tool::Finger, 100.0, 100.0, 1.0, &[]);
        f1.contact_px = 40.0;
        let f2 = pointer(1, Tool::Finger, 300.0, 100.0, 1.0, &[]);
        let events = run(
            &mut d,
            &[
                motion(Action::HoverMove, 0, 1, vec![pointer(0, Tool::Finger, 1.0, 1.0, 0.0, &[])]),
                motion(Action::Down, 0, 2, vec![f1.clone()]),
                motion(Action::Down, 1, 3, vec![f1.clone(), f2.clone()]),
                motion(Action::Move, 0, 4, vec![pointer(0, Tool::Finger, 110.0, 100.0, 1.0, &[]), pointer(1, Tool::Finger, 290.0, 100.0, 1.0, &[])]),
                motion(Action::Up, 1, 5, vec![pointer(0, Tool::Finger, 110.0, 100.0, 1.0, &[]), pointer(1, Tool::Finger, 290.0, 100.0, 1.0, &[])]),
                motion(Action::Up, 0, 6, vec![pointer(0, Tool::Finger, 110.0, 100.0, 1.0, &[])]),
            ],
        );
        let p = pointers(&events);
        assert!(p.iter().all(|e| e.device == Device::Touch), "all fingers");
        assert_eq!(
            p.iter().map(|e| (e.id, e.phase)).collect::<Vec<_>>(),
            [(0, Phase::Down), (1, Phase::Down), (0, Phase::Move), (1, Phase::Move), (1, Phase::Up), (0, Phase::Up),]
        );
        assert_eq!(p[0].contact_px, 40.0);
        assert_eq!(check(&p), Ok(()));
    }

    #[test]
    fn a_pen_and_a_finger_at_once_stay_what_they_are_and_the_eraser_end_is_an_eraser() {
        let mut d = Decoder::new(0.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 1, vec![pointer(0, Tool::Finger, 10.0, 10.0, 1.0, &[])]),
                motion(Action::Down, 1, 2, vec![pointer(0, Tool::Finger, 10.0, 10.0, 1.0, &[]), pointer(1, Tool::Eraser, 50.0, 50.0, 0.5, &[])]),
                motion(Action::Move, 0, 3, vec![pointer(0, Tool::Finger, 12.0, 10.0, 1.0, &[]), pointer(1, Tool::Eraser, 55.0, 50.0, 0.5, &[])]),
                motion(Action::Up, 1, 4, vec![pointer(0, Tool::Finger, 12.0, 10.0, 1.0, &[]), pointer(1, Tool::Eraser, 55.0, 50.0, 0.5, &[])]),
                motion(Action::Up, 0, 5, vec![pointer(0, Tool::Finger, 12.0, 10.0, 1.0, &[])]),
            ],
        );
        let p = pointers(&events);
        let eraser: Vec<&PointerEvent> = p.iter().filter(|e| e.device == Device::Pen).collect();
        assert_eq!(eraser.len(), 3);
        assert!(eraser.iter().all(|e| e.pen.eraser), "the eraser end says so on every event");
        assert!(p.iter().filter(|e| e.device == Device::Touch).count() == 3);
        assert_eq!(check(&p), Ok(()));
    }

    #[test]
    fn a_palm_is_never_a_stroke_and_a_cancel_ends_everything_that_was_down() {
        let mut d = Decoder::new(0.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 1, vec![pointer(5, Tool::Palm, 10.0, 10.0, 1.0, &[])]),
                motion(Action::Move, 0, 2, vec![pointer(5, Tool::Palm, 12.0, 10.0, 1.0, &[])]),
                motion(Action::Down, 0, 3, vec![pointer(1, Tool::Stylus, 50.0, 50.0, 0.4, &[])]),
                motion(Action::Down, 1, 4, vec![pointer(1, Tool::Stylus, 50.0, 50.0, 0.4, &[]), pointer(2, Tool::Finger, 90.0, 90.0, 1.0, &[])]),
                motion(Action::Cancel, 0, 5, vec![pointer(1, Tool::Stylus, 50.0, 50.0, 0.4, &[]), pointer(2, Tool::Finger, 90.0, 90.0, 1.0, &[])]),
                motion(Action::Move, 0, 6, vec![pointer(1, Tool::Stylus, 51.0, 50.0, 0.4, &[])]),
            ],
        );
        let p = pointers(&events);
        assert!(p.iter().all(|e| e.id != 5), "the palm never reaches the editor");
        assert_eq!(
            p.iter().map(|e| (e.id, e.phase)).collect::<Vec<_>>(),
            [(1, Phase::Down), (2, Phase::Down), (1, Phase::Cancel), (2, Phase::Cancel)]
        );
        assert!(!d.is_down(1) && !d.is_down(2), "and a move after it is not a stroke");
        assert_eq!(check(&p), Ok(()));
    }

    #[test]
    fn a_pointer_that_goes_down_twice_is_ended_first_and_an_up_that_was_never_down_is_nothing() {
        let mut d = Decoder::new(0.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 1, vec![pointer(1, Tool::Finger, 1.0, 1.0, 1.0, &[])]),
                motion(Action::Down, 0, 2, vec![pointer(1, Tool::Finger, 2.0, 2.0, 1.0, &[])]),
                motion(Action::Up, 0, 3, vec![pointer(1, Tool::Finger, 2.0, 2.0, 1.0, &[])]),
                motion(Action::Up, 0, 4, vec![pointer(1, Tool::Finger, 2.0, 2.0, 1.0, &[])]),
            ],
        );
        let p = pointers(&events);
        assert_eq!(p.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Down, Phase::Cancel, Phase::Down, Phase::Up]);
        assert_eq!(check(&p), Ok(()));
    }

    #[test]
    fn a_mouse_is_the_mouse_and_its_buttons_are_its_own() {
        let mut d = Decoder::new(0.0);
        let mut right = motion(Action::Down, 0, 3, vec![pointer(0, Tool::Mouse, 20.0, 20.0, 0.5, &[])]);
        right.secondary = true;
        let mut left = motion(Action::Down, 0, 1, vec![pointer(0, Tool::Mouse, 10.0, 10.0, 0.5, &[])]);
        left.primary = true;
        let events = run(
            &mut d,
            &[
                motion(Action::HoverMove, 0, 0, vec![pointer(0, Tool::Mouse, 5.0, 5.0, 0.0, &[])]),
                left,
                motion(Action::Move, 0, 2, vec![pointer(0, Tool::Mouse, 15.0, 10.0, 0.5, &[])]),
                motion(Action::Up, 0, 2, vec![pointer(0, Tool::Mouse, 15.0, 10.0, 0.0, &[])]),
                right,
                motion(Action::Up, 0, 4, vec![pointer(0, Tool::Mouse, 20.0, 20.0, 0.0, &[])]),
                motion(Action::HoverExit, 0, 5, vec![pointer(0, Tool::Mouse, 20.0, 20.0, 0.0, &[])]),
            ],
        );
        assert!(pointers(&events).is_empty(), "a mouse is not a pointer of the pen and touch kind");
        assert_eq!(
            events,
            vec![
                InputEvent::MouseMoved([5.0, 5.0]),
                InputEvent::MouseMoved([10.0, 10.0]),
                InputEvent::MouseButton { state: ButtonState::Pressed, button: MouseButton::Left },
                InputEvent::MouseMoved([15.0, 10.0]),
                InputEvent::MouseMoved([15.0, 10.0]),
                InputEvent::MouseButton { state: ButtonState::Released, button: MouseButton::Left },
                InputEvent::MouseMoved([20.0, 20.0]),
                InputEvent::MouseButton { state: ButtonState::Pressed, button: MouseButton::Right },
                InputEvent::MouseMoved([20.0, 20.0]),
                InputEvent::MouseButton { state: ButtonState::Released, button: MouseButton::Right },
                InputEvent::MouseLeft,
            ]
        );
    }

    #[test]
    fn times_are_the_programs_and_never_go_back_and_pressures_are_always_from_0_to_1() {
        let mut d = Decoder::new(1_000.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 1_010, vec![pointer(1, Tool::Stylus, 1.0, 1.0, f32::NAN, &[])]),
                // A history older than the down, and one out of order: neither makes time go back.
                motion(
                    Action::Move,
                    0,
                    1_020,
                    vec![pointer(1, Tool::Stylus, 2.0, 1.0, 3.0, &[(1.5, 1.0, -1.0, 1_005), (1.7, 1.0, 0.5, 1_018), (1.8, 1.0, 0.5, 1_012)])],
                ),
                motion(Action::Up, 0, 1_015, vec![pointer(1, Tool::Stylus, 2.0, 1.0, 0.0, &[])]),
                // An event from before the program's clock began.
                motion(Action::Down, 0, 500, vec![pointer(2, Tool::Finger, 1.0, 1.0, 1.0, &[])]),
                motion(Action::Up, 0, 501, vec![pointer(2, Tool::Finger, 1.0, 1.0, 1.0, &[])]),
            ],
        );
        let p = pointers(&events);
        assert_eq!(p[0].samples[0].t_ms, 10.0, "1010 ms on the toolkit's clock is 10 on the program's");
        assert_eq!(p[0].samples[0].pressure, 0.5, "a pressure that is not a number is the middle");
        assert!(p[1].samples.iter().all(|s| (0.0..=1.0).contains(&s.pressure)));
        assert_eq!(p[3].samples[0].t_ms, 0.0, "not before the clock began");
        assert_eq!(check(&p), Ok(()), "time never goes back, whatever order the toolkit gave");
    }

    #[test]
    fn a_touch_that_arrives_late_from_the_toolkit_is_the_pointer_it_already_was() {
        let mut d = Decoder::new(0.0);
        let mut out = Vec::new();
        d.motion(&motion(Action::Down, 0, 1, vec![pointer(4, Tool::Stylus, 5.0, 5.0, 0.5, &[])]), &mut out);
        out.clear();
        // The up of the pen came through the toolkit's own events: it is still the pen's.
        d.late_touch(4, Phase::Up, [6.0, 5.0], 0.0, 2.0, &mut out);
        let p = pointers(&out);
        assert_eq!((p[0].device, p[0].id, p[0].phase), (Device::Pen, 4, Phase::Up));
        assert!(!d.is_down(4));
        // One that was never seen is a finger.
        out.clear();
        d.late_touch(9, Phase::Down, [1.0, 1.0], 1.0, 3.0, &mut out);
        d.late_touch(9, Phase::Move, [2.0, 1.0], 1.0, 4.0, &mut out);
        d.late_touch(9, Phase::Up, [2.0, 1.0], 1.0, 5.0, &mut out);
        let p = pointers(&out);
        assert!(p.iter().all(|e| e.device == Device::Touch));
        assert_eq!(check(&p), Ok(()));
        // A move of a pointer that is not down is nothing.
        out.clear();
        d.late_touch(77, Phase::Move, [1.0, 1.0], 1.0, 6.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn keys_become_the_editors_keys_and_what_is_for_the_system_is_left_to_it() {
        let press = |code, unicode: Option<char>, ctrl: bool, shift: bool| {
            let mut out = Vec::new();
            let taken = key(&RawKey { code, down: true, ctrl, shift, alt: false, unicode }, &mut out);
            (taken, out)
        };
        assert_eq!(press(KeyCode::Backspace, None, false, false), (true, vec![InputEvent::Key(KeyInput::Backspace)]));
        assert_eq!(
            press(KeyCode::Back, None, false, false),
            (true, vec![InputEvent::Key(KeyInput::Escape)]),
            "back puts away a card, as Escape does"
        );
        assert_eq!(press(KeyCode::Enter, None, false, false).1, vec![InputEvent::Key(KeyInput::Text("\n".to_string()))]);
        assert_eq!(press(KeyCode::PageDown, None, false, false), (true, vec![InputEvent::Key(KeyInput::Page(true))]));
        assert_eq!(press(KeyCode::PageUp, None, false, false), (true, vec![InputEvent::Key(KeyInput::Page(false))]));
        // Alt and a sideways arrow go back and forward through the places a link took the reader from; an arrow alone is an arrow.
        let alt = |code| {
            let mut out = Vec::new();
            let taken = key(&RawKey { code, down: true, ctrl: false, shift: false, alt: true, unicode: None }, &mut out);
            (taken, out)
        };
        assert_eq!(alt(KeyCode::Left), (true, vec![InputEvent::Key(KeyInput::Trail(false))]));
        assert_eq!(alt(KeyCode::Right), (true, vec![InputEvent::Key(KeyInput::Trail(true))]));
        assert_eq!(alt(KeyCode::Up).1, vec![InputEvent::Key(KeyInput::Arrow(crate::input::Dir::Up, false))]);
        assert_eq!(press(KeyCode::Left, None, false, true).1, vec![InputEvent::Key(KeyInput::Arrow(crate::input::Dir::Left, true))]);
        assert_eq!(press(KeyCode::Other, Some('é'), false, false).1, vec![InputEvent::Key(KeyInput::Text("é".to_string()))]);
        assert_eq!(press(KeyCode::Other, Some('z'), true, false).1, vec![InputEvent::Shortcut { letter: "z".to_string(), shift: false }]);
        assert_eq!(press(KeyCode::Other, None, false, false), (false, vec![]), "volume and the like are the system's");
        assert_eq!(press(KeyCode::Other, Some('\u{7}'), false, false), (false, vec![]), "a control character types nothing");
        let mut out = Vec::new();
        assert!(
            key(&RawKey { code: KeyCode::Back, down: false, ctrl: false, shift: false, alt: false, unicode: None }, &mut out),
            "its release is taken too"
        );
        assert!(out.is_empty());
        assert!(!key(&RawKey { code: KeyCode::Other, down: false, ctrl: false, shift: false, alt: false, unicode: Some('a') }, &mut out));
    }

    #[test]
    fn a_cancel_says_where_it_happened_so_whoever_is_told_can_let_the_pointer_go() {
        let mut d = Decoder::new(0.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 10, vec![pointer(1, Tool::Finger, 50.0, 60.0, 0.5, &[])]),
                motion(Action::Move, 0, 20, vec![pointer(1, Tool::Finger, 70.0, 80.0, 0.5, &[])]),
                motion(Action::Cancel, 0, 30, vec![pointer(1, Tool::Finger, 70.0, 80.0, 0.5, &[])]),
            ],
        );
        let p = pointers(&events);
        let cancel = p.last().expect("a cancel");
        assert_eq!((cancel.phase, cancel.samples.iter().map(|s| s.pos).collect::<Vec<_>>()), (Phase::Cancel, vec![[70.0, 80.0]]));
        assert_eq!(check(&p), Ok(()));
    }

    #[test]
    fn a_stylus_and_a_finger_with_the_same_id_are_two_pointers() {
        // A pen digitiser and a touch screen number their pointers apart: both can be pointer 0.
        let mut d = Decoder::new(0.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 10, vec![pointer(0, Tool::Stylus, 10.0, 10.0, 0.5, &[])]),
                motion(Action::Down, 0, 12, vec![pointer(0, Tool::Finger, 300.0, 300.0, 0.5, &[])]),
                // The screen gives up on its touch (the pen is near): the stroke goes on.
                motion(Action::Cancel, 0, 14, vec![pointer(0, Tool::Finger, 300.0, 300.0, 0.5, &[])]),
                motion(Action::Move, 0, 16, vec![pointer(0, Tool::Stylus, 20.0, 20.0, 0.5, &[])]),
                motion(Action::Up, 0, 18, vec![pointer(0, Tool::Stylus, 20.0, 20.0, 0.0, &[])]),
            ],
        );
        let p = pointers(&events);
        assert_eq!(
            p.iter().map(|e| (e.device, e.phase)).collect::<Vec<_>>(),
            [
                (Device::Pen, Phase::Down),
                (Device::Touch, Phase::Down),
                (Device::Touch, Phase::Cancel),
                (Device::Pen, Phase::Move),
                (Device::Pen, Phase::Up)
            ]
        );
        // And a finger 0 down does not hide a stylus 0 hovering.
        let mut d = Decoder::new(0.0);
        let events = run(
            &mut d,
            &[
                motion(Action::Down, 0, 10, vec![pointer(0, Tool::Finger, 300.0, 300.0, 0.5, &[])]),
                motion(Action::HoverMove, 0, 12, vec![pointer(0, Tool::Stylus, 10.0, 10.0, 0.0, &[])]),
            ],
        );
        assert_eq!(pointers(&events).last().map(|e| (e.device, e.phase)), Some((Device::Pen, Phase::Hover)));
    }
}
