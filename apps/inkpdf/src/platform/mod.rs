//! Everything that differs between operating systems, behind one small surface. The shell (`app`) and `main` talk to
//! this module and to nothing named `windows`, `wayland` or `android`; each OS lives in its own sub-module and is
//! chosen at compile time, so a build for one OS contains none of the others' code.
//!
//! ```text
//!   platform/windows   Win32 pointer messages (pen history at 200-1000 Hz), message boxes, clipboard, posture
//!   platform/linux     Wayland / X11 through winit; a native tablet-pad reader goes beside it (see docs/PORTING.md)
//!   platform/android   NativeActivity through winit; stylus MotionEvents (historical samples) go beside it
//! ```
//!
//! # Why the pointer source is its own part
//!
//! The pen path is what the app is for, and its latency is `input -> app -> GPU -> scan-out`. winit delivers pen and
//! touch late (one position per event-loop turn, no eraser, no barrel button, no tilt), so on every OS the pen is read
//! by a small component of the OS's own, and the rest of the program sees only [`PointerEvent`]s, which are the same
//! everywhere. The frame loop's contract with it is exactly the one Windows has always had:
//!
//! 1. wait for the display (`Gpu::acquire`),
//! 2. [`PointerSource::pump`]: take whatever the OS has queued *right now*, bypassing the event loop,
//! 3. [`PointerSource::drain`]: hand the samples to the editor, then draw.
//!
//! A new OS therefore adds one `PointerSource`; it changes nothing above this module.

use std::sync::Arc;

use winit::window::Window;

use crate::input::PointerEvent;

#[cfg(target_os = "android")]
mod android;
#[cfg(not(windows))]
mod generic;
#[cfg(all(unix, not(target_os = "android")))]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "android")]
use android as os;
#[cfg(all(unix, not(target_os = "android")))]
use linux as os;
#[cfg(windows)]
use windows as os;

pub use os::{
    SystemClipboard, attach_parent_console, detect_mode, documents_dir, error_box, motion_prefs, open_url, places, report_fatal, set_text_input,
};
#[cfg(target_os = "android")]
pub use os::{android_app_started, welcome_document};

/// A pen, touch and posture reader that is the OS's own: the one place input is taken from before winit sees it.
pub trait PointerSource {
    /// Take the input the OS has queued but the event loop has not delivered, without waiting. Called once per frame,
    /// right after the display wait, so the frame carries the newest samples. Must be cheap and bounded.
    fn pump(&mut self);

    /// Whether any events are waiting to be [`drain`](Self::drain)ed (the shell draws at once when there are).
    fn pending(&self) -> bool;

    /// The events received since the last call, oldest first. A pen's batched points are in `samples`.
    fn drain(&mut self) -> Vec<PointerEvent>;

    /// Whether the machine's posture changed since this was last asked (a convertible became a tablet, or back).
    fn take_posture_change(&mut self) -> bool;

    /// Stop receiving; called when the app exits.
    fn uninstall(&mut self);
}

/// Start the OS's own pen and touch reader for `window`. An `Err` is not fatal: the shell falls back to the basic
/// mouse and touch winit reports (no pressure, eraser or buttons for a pen), and says why on the terminal.
pub fn install_pointer(window: &Arc<Window>) -> Result<Box<dyn PointerSource>, String> {
    os::install_pointer(window)
}
