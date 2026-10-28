//! What each input device is for, and the logic that follows from it. Pure, so it is unit-tested.
//!
//! Each device does one kind of thing, whatever the tool:
//!
//! * **Stylus**: everything done to the page. The pen tool draws, erases (eraser end) and lassos
//!   (barrel button); the shape tool does that too, and selects and edits shapes; the pointer tool
//!   points.
//! * **Touch**: pans and zooms; a tap follows a link; pulling past the last page adds a page.
//! * **Mouse**: scrolls. (It also presses the buttons of the toolbars.)
//! * **Keyboard**: nothing.

mod gestures;
mod palm;
mod predict;

pub use gestures::{GestureDelta, TouchGestures};
pub use palm::PalmGuard;
pub use predict::predict;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenTool {
    Draw,
    Erase,
    Lasso,
}

impl PenButtons {
    pub fn tool(&self) -> PenTool {
        if self.eraser {
            PenTool::Erase
        } else if self.barrel {
            PenTool::Lasso
        } else {
            PenTool::Draw
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
    Cancel,
    /// A pen moving above the screen.
    Hover,
    /// A pen leaving detection range.
    Leave,
}

/// One pointer message: the samples it carried (oldest first; Windows batches the points between
/// frames) and the state of the device.
#[derive(Clone, Debug)]
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

/// What a stylus press does.
pub fn role(pen: &PenButtons) -> Role {
    match pen.tool() {
        PenTool::Draw => Role::Draw,
        PenTool::Erase => Role::Erase,
        PenTool::Lasso => Role::Lasso,
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
        assert_eq!(role(&pen(false, false)), Role::Draw);
        assert_eq!(role(&pen(true, false)), Role::Erase);
        assert_eq!(role(&pen(false, true)), Role::Lasso);
        // The eraser end wins over the barrel button.
        assert_eq!(role(&pen(true, true)), Role::Erase);
        // Hovering with the button down is already a lasso.
        assert_eq!(PenButtons { in_range: true, contact: false, eraser: false, barrel: true }.tool(), PenTool::Lasso);
    }
}
