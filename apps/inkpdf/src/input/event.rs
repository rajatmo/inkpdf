//! Everything an input device can say to the editor, as plain data the program owns: the mouse, the wheel, the keyboard, a touch
//! pen and touch, the window's size and focus. The shell
//! translates what its toolkit says into these; the editor knows no toolkit, so any shell can drive it: a window of winit's, a
//! host that is not winit, or a replay of a recorded session (see `replay`).
//!
//! Pen and touch are [`PointerEvent`](super::PointerEvent)s, whether the OS's own reader made them or the shell made them from the
//! toolkit's touch events (which cannot tell a pen from a finger, but press the buttons and pan and zoom as the reader's do).

use super::PointerEvent;

/// Whether a button went down or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

/// A mouse button. The three that mean something to the program, and `Other` for the rest (back, forward, …), which it ignores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other,
}

/// A turn of a wheel, or a touchpad's scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wheel {
    /// Notches of a wheel: sideways and up (positive is away from the person).
    Lines(f32, f32),
    /// Pixels, which a touchpad already glides: sideways and down.
    Pixels(f32, f32),
}

/// An arrow key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// What the keyboard does to what takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key<'a> {
    /// Characters typed (a new line is `"\n"`).
    Text(&'a str),
    Backspace,
    Delete,
    Escape,
    /// An arrow, with whether Shift is held.
    Arrow(Dir, bool),
    /// Home or End, with whether Shift is held.
    Home(bool),
    End(bool),
    /// A letter pressed with Ctrl (the ones that work on text being edited: a, c, x, v).
    Chord(char),
    /// Page Down (`true`) or Page Up, or the space bar when nothing is being typed.
    Page(bool),
    /// Forward (`true`) or back through the places a link, a contents entry or a search result has taken the reader from: Alt and an arrow, or
    /// the mouse's side buttons.
    Trail(bool),
}

/// A [`Key`] that owns its text, so that it can be kept (in an event that is queued, or recorded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Text(String),
    Backspace,
    Delete,
    Escape,
    Arrow(Dir, bool),
    Home(bool),
    End(bool),
    Chord(char),
    Page(bool),
    Trail(bool),
}

impl KeyInput {
    /// The key as the tools and the editor take it.
    pub fn as_key(&self) -> Key<'_> {
        match self {
            KeyInput::Text(text) => Key::Text(text),
            KeyInput::Backspace => Key::Backspace,
            KeyInput::Delete => Key::Delete,
            KeyInput::Escape => Key::Escape,
            KeyInput::Arrow(dir, shift) => Key::Arrow(*dir, *shift),
            KeyInput::Home(shift) => Key::Home(*shift),
            KeyInput::End(shift) => Key::End(*shift),
            KeyInput::Chord(c) => Key::Chord(*c),
            KeyInput::Page(down) => Key::Page(*down),
            KeyInput::Trail(forward) => Key::Trail(*forward),
        }
    }
}

impl From<Key<'_>> for KeyInput {
    fn from(key: Key<'_>) -> Self {
        match key {
            Key::Text(text) => KeyInput::Text(text.to_string()),
            Key::Backspace => KeyInput::Backspace,
            Key::Delete => KeyInput::Delete,
            Key::Escape => KeyInput::Escape,
            Key::Arrow(dir, shift) => KeyInput::Arrow(dir, shift),
            Key::Home(shift) => KeyInput::Home(shift),
            Key::End(shift) => KeyInput::End(shift),
            Key::Chord(c) => KeyInput::Chord(c),
            Key::Page(down) => KeyInput::Page(down),
            Key::Trail(forward) => KeyInput::Trail(forward),
        }
    }
}

/// One thing that happened to the program's window, from any device. `Editor::handle` is the one way in.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// Pen or touch from the OS's own reader.
    Pointer(PointerEvent),
    /// The mouse is at this place in the window, in pixels.
    MouseMoved([f32; 2]),
    MouseButton {
        state: ButtonState,
        button: MouseButton,
    },
    /// The mouse left the window.
    MouseLeft,
    Wheel(Wheel),
    /// The wheel turned with Ctrl held: zoom, by this many notches (positive is in), about the mouse.
    Zoom(f32),
    /// Shift went down or up.
    Shift(bool),
    Key(KeyInput),
    /// A letter pressed with Ctrl (a shortcut), with whether Shift is held.
    Shortcut {
        letter: String,
        shift: bool,
    },
    /// The window lost the keyboard focus.
    FocusLost,
    /// The window's size in pixels.
    Resized([f32; 2]),
    /// The scale of the window's display (1.0 is 100%).
    ScaleChanged(f32),
}
