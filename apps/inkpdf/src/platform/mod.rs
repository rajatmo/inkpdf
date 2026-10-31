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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::clipboard::TextClipboard;
use crate::input::{Mode, PointerEvent};
use crate::motion::Prefs;
use crate::path_store::PathStore;
use crate::store::FileStore;

/// Asks the event loop for a frame from any thread (a native input callback uses it to wake a sleeping loop).
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// A window as the system's dialogs want it: its handle and its display's.
pub trait WindowHandles: HasWindowHandle + HasDisplayHandle {}
impl<T: HasWindowHandle + HasDisplayHandle + ?Sized> WindowHandles for T {}

/// What the system's file picker was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A PDF to open.
    Open,
    /// A place and a name to save a document under.
    Save,
}

/// What the person picked, or `None` if they gave up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    pub kind: Pick,
    pub id: Option<crate::store::DocId>,
}

/// What the shell asks of the operating system, as one object built once at start (see [`native`]). The window comes in as a
/// raw handle, so nothing here names the windowing toolkit; a test builds its own `Platform`, and a new OS or backend is one
/// `impl`. (Only [`PointerSource`] is on the pen's path, and it was already a trait object, so this costs the frame nothing.)
pub trait Platform {
    /// Where a new document is first offered a place to be saved.
    fn documents_dir(&self) -> PathBuf;
    /// The folder the app keeps what it remembers in (the user's configuration folder), if there is one.
    fn config_dir(&self) -> Option<PathBuf>;
    /// The mode the machine's posture calls for.
    fn detect_mode(&self) -> Mode;
    /// What the user has asked of motion.
    fn motion_prefs(&self) -> Prefs;
    /// What the system says its look is (light or dark), for where the windowing toolkit cannot say: it can on Windows (the shell asks
    /// the window), not on X11, nor Android. `INKPDF_THEME=light|dark` says it for a run. `None` when it is not known (taken as light).
    fn color_scheme(&self) -> Option<crate::palette::Scheme>;
    /// Whether the system's look can change while the program is in front of the person, with nothing the program is told of (Android: the
    /// quick settings switch it): the shell then looks again each time its loop wakes.
    fn watches_color_scheme(&self) -> bool {
        false
    }
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
    /// Whether the system's bars (Android's status and navigation bars) are the platform's to show and hide, which is what the full-screen button
    /// does there; where the window can fill the screen itself (a desktop) it is not, and the window is asked.
    fn controls_system_bars(&self) -> bool {
        false
    }
    /// Hide the system's bars, or show them again (see [`Platform::controls_system_bars`]).
    fn set_system_bars_hidden(&self, _hidden: bool) {}
    /// Show the pointer the rules ask for, where the platform has a pointer of its own to set (Android's, for a mouse or a hovering stylus;
    /// a desktop's is the window's, set by the shell).
    fn set_pointer_icon(&self, _kind: crate::input::CursorKind) {}
    /// Whether the system has a picker of its own for files (Android's: the app cannot see the folders, it is handed the file): then it
    /// opens and saves where the program's own dialog would.
    fn has_picker(&self) -> bool {
        false
    }
    /// Ask the system's picker for a file (see [`Platform::has_picker`]), over `window`, starting in the folder `dir` and offering `name` when
    /// saving; the answer is taken with [`Platform::take_picked`].
    fn pick(&self, _window: &dyn WindowHandles, _kind: Pick, _name: &str, _dir: &Path) {}
    /// How to wake the event loop when the picker has answered (it may do so from another thread).
    fn set_waker(&self, _wake: Waker) {}
    /// What the picker answered since this was last asked, if it has.
    fn take_picked(&self) -> Option<Picked> {
        None
    }
    /// A document another app handed this one ("Open with", a share) that has not been taken yet.
    fn take_launched(&self) -> Option<crate::store::DocId> {
        None
    }
    /// Start the OS's own pen and touch reader for `window`. An `Err` is not fatal: the shell falls back to the basic mouse
    /// and touch the toolkit reports (no pressure, eraser or buttons for a pen), and says why on the terminal. `wake` asks
    /// the event loop for a frame.
    fn install_pointer(&self, window: &dyn HasWindowHandle, wake: Waker) -> Result<Box<dyn PointerSource>, String>;
}

#[cfg(test)]
mod contract;
#[cfg(test)]
pub(crate) mod fake;

#[cfg(not(target_os = "android"))]
mod dialog;

#[cfg(target_os = "android")]
mod android;
#[cfg_attr(not(target_os = "android"), allow(dead_code, reason = "Android's; tested everywhere"))]
pub(crate) mod saf;
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
pub use os::{android_app_started, layout_request, test_mode};

/// The bridge to the Java side of the app, on the platform that has one (Android): files the system hands out, the clipboard, links.
#[cfg(target_os = "android")]
fn bridge() -> Option<Arc<dyn saf::Bridge>> {
    os::bridge()
}

#[cfg(not(target_os = "android"))]
fn bridge() -> Option<Arc<dyn saf::Bridge>> {
    None
}
pub use os::{attach_parent_console, report_fatal};

/// This build's operating system as a [`Platform`]: each fact is the one the OS's own module names.
#[derive(Default)]
struct Native {
    #[cfg(not(target_os = "android"))]
    dialogs: dialog::Dialogs,
}

impl Platform for Native {
    fn documents_dir(&self) -> PathBuf {
        os::documents_dir()
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

    fn watches_color_scheme(&self) -> bool {
        cfg!(target_os = "android")
    }

    fn file_store(&self) -> Arc<dyn FileStore> {
        match bridge() {
            Some(bridge) => Arc::new(saf::SafStore::new(bridge)),
            None => Arc::new(PathStore),
        }
    }

    fn color_scheme(&self) -> Option<crate::palette::Scheme> {
        os::color_scheme()
    }

    fn system_font(&self) -> Option<Vec<u8>> {
        os::system_font()
    }

    fn open_url(&self, url: &str) {
        match bridge() {
            Some(bridge) => bridge.open_url(url),
            None => os::open_url(url),
        }
    }

    fn error_box(&self, window: &dyn HasWindowHandle, text: &str) {
        os::error_box(window, text);
    }

    fn set_text_input(&self, active: bool) {
        os::set_text_input(active);
    }

    fn clipboard(&self, window: &dyn HasWindowHandle) -> Option<Box<dyn TextClipboard>> {
        match bridge() {
            Some(bridge) => Some(Box::new(saf::BridgeClipboard(bridge))),
            None => os::SystemClipboard::new(window).map(|c| Box::new(c) as Box<dyn TextClipboard>),
        }
    }

    fn controls_system_bars(&self) -> bool {
        bridge().is_some()
    }

    fn set_system_bars_hidden(&self, hidden: bool) {
        if let Some(bridge) = bridge() {
            bridge.set_bars_hidden(hidden);
        }
    }

    fn set_pointer_icon(&self, kind: crate::input::CursorKind) {
        use crate::input::CursorKind;
        // (Android's PointerIcon types: arrow, hand, text, crosshair, all-scroll, horizontal double arrow, grab, grabbing.)
        let code = match kind {
            CursorKind::Arrow => 1000,
            CursorKind::Hand => 1002,
            CursorKind::Text => 1008,
            CursorKind::Crosshair | CursorKind::Dot | CursorKind::Eraser => 1007,
            CursorKind::Move => 1013,
            CursorKind::Resize => 1014,
            CursorKind::Grab => 1020,
            CursorKind::Grabbing => 1021,
        };
        if let Some(bridge) = bridge() {
            bridge.set_pointer_icon(code);
        }
    }

    fn has_picker(&self) -> bool {
        true
    }

    fn pick(&self, window: &dyn WindowHandles, kind: Pick, name: &str, dir: &Path) {
        if let Some(bridge) = bridge() {
            bridge.pick(kind, name);
            return;
        }
        #[cfg(not(target_os = "android"))]
        self.dialogs.pick(window, kind, name, dir);
        #[cfg(target_os = "android")]
        let _ = (window, dir);
    }

    fn set_waker(&self, wake: Waker) {
        #[cfg(not(target_os = "android"))]
        self.dialogs.set_waker(wake);
        #[cfg(target_os = "android")]
        let _ = wake;
    }

    fn take_picked(&self) -> Option<Picked> {
        if let Some(bridge) = bridge() {
            return bridge.take_picked().as_deref().and_then(saf::parse_picked);
        }
        #[cfg(not(target_os = "android"))]
        return self.dialogs.take();
        #[cfg(target_os = "android")]
        None
    }

    fn take_launched(&self) -> Option<crate::store::DocId> {
        bridge()?.take_launch().filter(|u| !u.is_empty()).map(crate::store::DocId::Uri)
    }

    fn install_pointer(&self, window: &dyn HasWindowHandle, wake: Waker) -> Result<Box<dyn PointerSource>, String> {
        os::install_pointer(window, wake)
    }
}

/// The platform this program was built for.
pub fn native() -> Box<dyn Platform> {
    Box::new(Native::default())
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
