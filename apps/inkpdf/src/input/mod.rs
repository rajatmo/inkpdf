//! What each input device is for, and the logic that follows from it. Pure, so it is unit-tested.
//!
//! Each device does one kind of thing, whatever the tool:
//!
//! * **Stylus**: everything done to the page. The pen tool draws, erases (eraser end) and lassos
//!   (barrel button); the shape tool does that too, and selects and edits shapes; the pointer tool
//!   points.
//! * **Touch**: pans and zooms; a tap follows a link; pulling past the last page adds a page.
//! * **Mouse**: scrolls. (It also presses the buttons of the toolbars.)
//! * **Keyboard**: types into a text box being edited.
//!
//! That is the *tablet* mode. In *desktop* mode (see [`Mode`]) the mouse also draws, as a stylus does,
//! and the keyboard has shortcuts and works on the selection.
//!
//! What the plain tip does (draw, select or erase) is chosen on screen ([`Intent`]), so a device with no
//! eraser end, barrel button or right button can still select and erase; those are shortcuts for the same.

mod contract;
mod event;
mod gestures;
mod palm;
mod predict;
pub mod record;

#[allow(unused_imports, dead_code, reason = "the checker is for the sources' tests and for replay, which validates a recorded session with it")]
pub use contract::{Broken, Checker, check};
pub use event::{ButtonState, Dir, InputEvent, Key, KeyInput, MouseButton, Wheel};
pub use gestures::{GestureDelta, TouchGestures};
pub use palm::PalmGuard;
pub use predict::predict;

/// How the machine is being held, which decides what the devices do: with a keyboard and mouse in front of
/// you (desktop), or as a slate you touch and write on (tablet). It follows the machine's posture (see
/// [`crate::platform`]) and can be forced from the command line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Desktop,
    Tablet,
}

impl Mode {
    /// `desktop` or `tablet`; anything else (`auto` is the caller's to handle) is `None`.
    pub fn parse(name: &str) -> Option<Mode> {
        match name.to_ascii_lowercase().as_str() {
            "desktop" => Some(Mode::Desktop),
            "tablet" => Some(Mode::Tablet),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    Mouse,
    Touch,
    Pen,
}

/// The pen's state as reported with a sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PenButtons {
    pub in_range: bool,
    /// The tip (or eraser end) is touching the screen.
    pub contact: bool,
    /// The eraser end is the one in use.
    pub eraser: bool,
    pub barrel: bool,
}

impl PenButtons {
    /// What a press with these buttons does: the eraser end erases, the barrel button lassos, the tip draws.
    pub fn role(&self) -> Role {
        if self.eraser {
            Role::Erase
        } else if self.barrel {
            Role::Lasso
        } else {
            Role::Draw
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Client-area position in device pixels.
    pub pos: [f32; 2],
    pub pressure: f32,
    /// Milliseconds on a monotonic clock.
    pub t_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    // (Only Windows says it so far: `WM_POINTERCAPTURECHANGED`. Android's `ACTION_CANCEL` will.)
    #[cfg_attr(not(windows), allow(dead_code))]
    Cancel,
    /// A pen moving above the screen.
    Hover,
    /// A pen leaving detection range.
    Leave,
}

/// One pointer message: the samples it carried (oldest first; Windows batches the points between
/// frames) and the state of the device.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerEvent {
    pub device: Device,
    pub id: u32,
    pub phase: Phase,
    pub samples: Vec<Sample>,
    pub pen: PenButtons,
    /// A touch's contact size: its longest side, in device pixels.
    pub contact_px: f32,
}

/// What a stylus press does: the tip draws, the eraser end erases, the barrel button makes a lasso.
/// (What "draws" means depends on the tool: ink, a shape, or the laser's pointing.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Draw,
    Erase,
    Lasso,
}

/// What the plain tip of the stylus (or the left mouse button) does, as chosen on screen: the way for a device with
/// no eraser end, no barrel button and no right button to draw, to select and to erase. The stylus's own buttons
/// and the mouse's other buttons are shortcuts for the same two things.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Intent {
    /// Make what the tool makes.
    #[default]
    Draw,
    /// Select what the tool made (a tap on it, or a loop around it), and move it.
    Select,
    /// Erase what the tool made.
    Erase,
}

impl Intent {
    pub fn role(self) -> Role {
        match self {
            Intent::Draw => Role::Draw,
            Intent::Select => Role::Lasso,
            Intent::Erase => Role::Erase,
        }
    }
}

/// What a press does: the pen's own buttons if one is held, else what was chosen on screen.
pub fn role_with(pen: &PenButtons, intent: Intent) -> Role {
    match pen.role() {
        Role::Draw => intent.role(),
        held => held,
    }
}
#[cfg(test)]
mod fixtures {
    use super::*;

    pub fn pen(eraser: bool, barrel: bool) -> PenButtons {
        PenButtons { in_range: true, contact: true, eraser, barrel }
    }

    pub fn touch_ev(id: u32, phase: Phase, size: f32) -> PointerEvent {
        PointerEvent { device: Device::Touch, id, phase, samples: vec![], pen: PenButtons::default(), contact_px: size }
    }

    pub fn pen_ev(phase: Phase, buttons: PenButtons) -> PointerEvent {
        PointerEvent { device: Device::Pen, id: 1, phase, samples: vec![], pen: buttons, contact_px: 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn pen_roles_follow_the_hardware() {
        assert_eq!(pen(false, false).role(), Role::Draw);
        assert_eq!(pen(true, false).role(), Role::Erase);
        assert_eq!(pen(false, true).role(), Role::Lasso);
        // The eraser end wins over the barrel button.
        assert_eq!(pen(true, true).role(), Role::Erase);
        // Hovering with the button down is already a lasso.
        assert_eq!(PenButtons { in_range: true, contact: false, eraser: false, barrel: true }.role(), Role::Lasso);
    }
}
