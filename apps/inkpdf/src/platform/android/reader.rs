//! The Android reader of pen, touch and keys. winit's Android backend turns every `MotionEvent` into a plain touch (it does not
//! pass on the tool type, so a stylus is a finger; it drops the stylus's buttons, hover and the points a pen reported between two
//! events) and then reads the activity's input queue itself; so this reader **reads the queue first**: at the start of every turn
//! of winit's loop (`pump_early`, which winit's `new_events` comes before its own reading of the queue) and again after the
//! display wait (`pump`, the late sampling the Windows reader does). What it takes winit never sees; a few events arrive in the
//! moment between, and winit reports those as touch (`late_touch`), which are made the pointer they already were.
//!
//! The reading is a thin copy of what the NDK says into [`RawMotion`] and [`RawKey`]; what they mean is decided by
//! `crate::platform::motion_event`, which is tested on every platform.

use winit::platform::android::activity::input::{InputEvent as Event, KeyAction, KeyEvent, KeyMapChar, Keycode, MotionAction, MotionEvent, ToolType};
use winit::platform::android::activity::{AndroidApp, InputStatus};

use crate::platform::motion_event::{Action, Decoder, HistoryPoint, KeyCode, RawKey, RawMotion, RawPointer, Tool};
use crate::input::{InputEvent, PointerEvent};
use crate::platform::{MAX_EVENTS_PER_PUMP, PointerSource};

/// Meta state bits for Ctrl (left, right, either), which are taken off when asking what character a key makes: with Ctrl held
/// Android gives a control character, and the program wants the letter.
const CTRL: u32 = ndk_sys_ctrl::ON | ndk_sys_ctrl::LEFT | ndk_sys_ctrl::RIGHT;

mod ndk_sys_ctrl {
    pub const ON: u32 = 0x1000;
    pub const LEFT: u32 = 0x2000;
    pub const RIGHT: u32 = 0x4000;
}

/// Milliseconds on `CLOCK_MONOTONIC`: the clock Android stamps input events with, and the one `Instant` (so `crate::clock`) reads.
fn monotonic_ms() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid out-pointer and CLOCK_MONOTONIC always exists.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1.0e6
}

fn tool(t: ToolType) -> Tool {
    match t {
        ToolType::Finger => Tool::Finger,
        ToolType::Stylus => Tool::Stylus,
        ToolType::Eraser => Tool::Eraser,
        ToolType::Mouse => Tool::Mouse,
        ToolType::Palm => Tool::Palm,
        _ => Tool::Unknown,
    }
}

fn raw_motion(m: &MotionEvent<'_>) -> RawMotion {
    let action = match m.action() {
        MotionAction::Down | MotionAction::PointerDown => Action::Down,
        MotionAction::Up | MotionAction::PointerUp => Action::Up,
        MotionAction::Move => Action::Move,
        MotionAction::Cancel => Action::Cancel,
        MotionAction::HoverEnter => Action::HoverEnter,
        MotionAction::HoverMove => Action::HoverMove,
        MotionAction::HoverExit => Action::HoverExit,
        _ => Action::Other,
    };
    let buttons = m.button_state();
    let pointers = m
        .pointers()
        .map(|p| RawPointer {
            id: p.pointer_id().max(0) as u32,
            tool: tool(p.tool_type()),
            pos: [p.x(), p.y()],
            pressure: p.pressure(),
            contact_px: p.touch_major(),
            history: p.history().map(|h| HistoryPoint { pos: [h.x(), h.y()], pressure: h.pressure(), t_ns: h.event_time() }).collect(),
        })
        .collect();
    RawMotion {
        action,
        action_index: m.pointer_index(),
        t_ns: m.event_time(),
        stylus_primary: buttons.stylus_primary(),
        stylus_secondary: buttons.stylus_secondary(),
        primary: buttons.primary(),
        secondary: buttons.secondary(),
        pointers,
    }
}

fn key_code(code: Keycode) -> KeyCode {
    match code {
        Keycode::Del => KeyCode::Backspace,
        Keycode::ForwardDel => KeyCode::Delete,
        Keycode::Escape => KeyCode::Escape,
        Keycode::Back => KeyCode::Back,
        Keycode::Enter | Keycode::NumpadEnter => KeyCode::Enter,
        Keycode::DpadLeft => KeyCode::Left,
        Keycode::DpadRight => KeyCode::Right,
        Keycode::DpadUp => KeyCode::Up,
        Keycode::DpadDown => KeyCode::Down,
        Keycode::MoveHome => KeyCode::Home,
        Keycode::MoveEnd => KeyCode::End,
        _ => KeyCode::Other,
    }
}

/// The source: the activity's input queue, read through the decoder.
pub struct AndroidPointer {
    app: AndroidApp,
    decoder: Decoder,
    ready: Vec<PointerEvent>,
    other: Vec<InputEvent>,
    /// What was decoded, before it is sorted into pointers and the rest (kept to reuse its room).
    scratch: Vec<InputEvent>,
}

impl AndroidPointer {
    pub fn new(app: AndroidApp) -> Self {
        // The toolkit's clock less the program's, read together: what turns an event's time into the program's.
        let offset_ms = monotonic_ms() - crate::clock::now_ms();
        Self { app, decoder: Decoder::new(offset_ms), ready: Vec::new(), other: Vec::new(), scratch: Vec::new() }
    }

    /// What a key event comes to, and whether it was the program's (`Unhandled` is for the system: the volume keys).
    fn key(app: &AndroidApp, k: &KeyEvent<'_>, out: &mut Vec<InputEvent>) -> InputStatus {
        let code = key_code(k.key_code());
        let meta = k.meta_state();
        let ctrl = meta.0 & CTRL != 0;
        // The character a key makes, if it is not one of the named ones: asked of the device's key map with Ctrl taken off.
        let unicode = if code == KeyCode::Other {
            app.device_key_character_map(k.device_id()).ok().and_then(|map| {
                match map.get(k.key_code(), winit::platform::android::activity::input::MetaState(meta.0 & !CTRL)) {
                    Ok(KeyMapChar::Unicode(c)) => Some(c),
                    _ => None,
                }
            })
        } else {
            None
        };
        let raw = RawKey { code, down: matches!(k.action(), KeyAction::Down), ctrl, shift: meta.shift_on(), unicode };
        if crate::platform::motion_event::key(&raw, out) { InputStatus::Handled } else { InputStatus::Unhandled }
    }

    /// Take what is in the queue, decode it, and sort it.
    fn read(&mut self) {
        let app = self.app.clone();
        let Ok(mut queue) = app.input_events_iter() else { return };
        let mut taken = 0;
        let (decoder, scratch) = (&mut self.decoder, &mut self.scratch);
        while taken < MAX_EVENTS_PER_PUMP {
            let more = queue.next(|event| match event {
                Event::MotionEvent(m) => {
                    decoder.motion(&raw_motion(m), scratch);
                    InputStatus::Handled
                }
                Event::KeyEvent(k) => Self::key(&app, k, scratch),
                _ => InputStatus::Unhandled,
            });
            if !more {
                break;
            }
            taken += 1;
        }
        self.sort();
    }

    fn sort(&mut self) {
        for event in self.scratch.drain(..) {
            match event {
                InputEvent::Pointer(p) => self.ready.push(p),
                other => self.other.push(other),
            }
        }
    }
}

impl PointerSource for AndroidPointer {
    fn pump(&mut self) {
        self.read();
    }

    fn pump_early(&mut self) {
        self.read();
    }

    fn pending(&self) -> bool {
        !self.ready.is_empty() || !self.other.is_empty()
    }

    fn drain_into(&mut self, out: &mut Vec<PointerEvent>) {
        out.append(&mut self.ready);
    }

    fn drain_other_into(&mut self, out: &mut Vec<InputEvent>) {
        out.append(&mut self.other);
    }

    fn late_touch(&mut self, ev: PointerEvent) {
        if let Some(s) = ev.samples.first() {
            self.decoder.late_touch(ev.id, ev.phase, s.pos, s.pressure, s.t_ms, &mut self.scratch);
            self.sort();
        }
    }

    fn take_posture_change(&mut self) -> bool {
        false
    }

    fn uninstall(&mut self) {}
}
