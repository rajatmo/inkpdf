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
//! 3. [`PointerSource::drain_into`]: hand the samples to the editor, then draw.
//!
//! A new OS therefore adds one `PointerSource`; it changes nothing above this module.

use std::path::PathBuf;
use std::sync::Arc;

use winit::raw_window_handle::HasWindowHandle;

use crate::browse::Place;
use crate::clipboard::TextClipboard;
use crate::input::{Mode, PointerEvent};
use crate::motion::Prefs;
use crate::path_store::PathStore;
use crate::store::FileStore;

/// Asks the event loop for a frame from any thread (a native input callback uses it to wake a sleeping loop).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// What the shell asks of the operating system, as one object built once at start (see [`native`]). The window comes in as a
/// raw handle, so nothing here names the windowing toolkit; a test builds its own `Platform`, and a new OS or backend is one
/// `impl`. (Only [`PointerSource`] is on the pen's path, and it was already a trait object, so this costs the frame nothing.)
pub trait Platform {
    /// Where a new document is first offered a place to be saved.
    fn documents_dir(&self) -> PathBuf;
    /// The places the save dialog offers in one press.
    fn places(&self) -> Vec<Place>;
    /// The folder the app keeps what it remembers in (the user's configuration folder), if there is one.
    fn config_dir(&self) -> Option<PathBuf>;
    /// The mode the machine's posture calls for.
    fn detect_mode(&self) -> Mode;
    /// What the user has asked of motion.
    fn motion_prefs(&self) -> Prefs;
    /// Where this platform keeps files: by path on the file system, or in whatever its own picker hands out.
    fn file_store(&self) -> Arc<dyn FileStore>;
    /// The bytes of the font the system draws text in, if its widths are the ones PDF readers use (Arial). Without one the program's
    /// own Liberation Sans is used, which is as wide.
    fn system_font(&self) -> Option<Vec<u8>>;
    /// Open a web address in the default program for it. The shell has checked it (`links::safe_url`): a platform does not.
    fn open_url(&self, url: &str);
    /// Tell the user something went wrong, over `window`.
    fn error_box(&self, window: &dyn HasWindowHandle, text: &str);
    /// Show or hide an on-screen keyboard.
    fn set_text_input(&self, active: bool);
    /// The system clipboard for the text of text boxes, if there is one.
    fn clipboard(&self, window: &dyn HasWindowHandle) -> Option<Box<dyn TextClipboard>>;
    /// Start the OS's own pen and touch reader for `window`. An `Err` is not fatal: the shell falls back to the basic mouse
    /// and touch the toolkit reports (no pressure, eraser or buttons for a pen), and says why on the terminal. `wake` asks
    /// the event loop for a frame.
    fn install_pointer(&self, window: &dyn HasWindowHandle, wake: Waker) -> Result<Box<dyn PointerSource>, String>;
}

#[cfg(test)]
mod contract;
#[cfg(test)]
pub(crate) mod fake;

#[cfg(target_os = "android")]
mod android;
#[cfg(not(windows))]
mod stubs;
#[cfg(not(windows))]
mod unix_dirs;

#[cfg(all(unix, not(target_os = "android")))]
mod linux;
#[allow(dead_code, reason = "read by the Android reader, tested everywhere")]
pub(crate) mod motion_event;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "android")]
use android as os;
#[cfg(all(unix, not(target_os = "android")))]
use linux as os;
#[cfg(windows)]
use windows as os;

#[cfg(target_os = "android")]
pub use os::{android_app_started, welcome_document};
pub use os::{attach_parent_console, report_fatal};

/// This build's operating system as a [`Platform`]: each fact is the one the OS's own module names.
struct Native;

impl Platform for Native {
    fn documents_dir(&self) -> PathBuf {
        os::documents_dir()
    }

    fn places(&self) -> Vec<Place> {
        os::places()
    }

    fn config_dir(&self) -> Option<PathBuf> {
        os::config_dir()
    }

    fn detect_mode(&self) -> Mode {
        os::detect_mode()
    }

    fn motion_prefs(&self) -> Prefs {
        os::motion_prefs()
    }

    fn file_store(&self) -> Arc<dyn FileStore> {
        Arc::new(PathStore)
    }

    fn system_font(&self) -> Option<Vec<u8>> {
        os::system_font()
    }

    fn open_url(&self, url: &str) {
        os::open_url(url);
    }

    fn error_box(&self, window: &dyn HasWindowHandle, text: &str) {
        os::error_box(window, text);
    }

    fn set_text_input(&self, active: bool) {
        os::set_text_input(active);
    }

    fn clipboard(&self, window: &dyn HasWindowHandle) -> Option<Box<dyn TextClipboard>> {
        os::SystemClipboard::new(window).map(|c| Box::new(c) as Box<dyn TextClipboard>)
    }

    fn install_pointer(&self, window: &dyn HasWindowHandle, wake: Waker) -> Result<Box<dyn PointerSource>, String> {
        os::install_pointer(window, wake)
    }
}

/// The platform this program was built for.
pub fn native() -> Box<dyn Platform> {
    Box::new(Native)
}

/// The most events one [`PointerSource::pump`] brings in: a flood of input must not hold the frame, so what is left waits for
/// the next one (and is not lost).
#[cfg_attr(not(any(windows, test)), allow(dead_code, reason = "the readers for the other platforms will take it"))]
pub const MAX_EVENTS_PER_PUMP: usize = 512;

/// A pen, touch and posture reader that is the OS's own: the one place input is taken from before winit sees it.
pub trait PointerSource {
    /// Take the input the OS has queued but the event loop has not delivered, without waiting. Called once per frame,
    /// right after the display wait, so the frame carries the newest samples. Must be cheap and bounded.
    fn pump(&mut self);

    /// Whether any events are waiting to be drained (the shell draws at once when there are).
    fn pending(&self) -> bool;

    /// Append the events received since the last call to `out`, oldest first. A pen's batched points are in `samples`. The
    /// caller keeps `out` from frame to frame, and the source its own queue, so a steady stream takes no memory from the allocator
    /// (a `Vec` handed over whole would have to grow again every frame).
    fn drain_into(&mut self, out: &mut Vec<PointerEvent>);

    /// The events received since the last call, as a new `Vec` (for tests: the frame loop uses [`drain_into`](Self::drain_into)).
    #[cfg(test)]
    fn drain(&mut self) -> Vec<PointerEvent> {
        let mut out = Vec::new();
        self.drain_into(&mut out);
        out
    }

    /// Whether the machine's posture changed since this was last asked (a convertible became a tablet, or back).
    fn take_posture_change(&mut self) -> bool;

    /// Take the input as early as the loop allows (the start of each of the toolkit's iterations), for a source that shares the
    /// OS's input queue with the toolkit and must read it first (Android's does: the toolkit would otherwise take pen events as
    /// touch). Nothing for the others.
    fn pump_early(&mut self) {}

    /// Append the other input the source took from the queue because it shared it (keys, a mouse) to `out`: handed over with the
    /// pointers.
    fn drain_other_into(&mut self, _out: &mut Vec<crate::input::InputEvent>) {}

    /// A touch the toolkit reported after all (the source took the rest of the queue, but a few events arrive in the moment between):
    /// the source makes it the pointer it already was, or a finger. Nothing for a source that swallows pen and touch before the
    /// toolkit sees them.
    fn late_touch(&mut self, _ev: PointerEvent) {}

    /// Stop receiving; called when the app exits.
    fn uninstall(&mut self);
}
