//! Android, through winit's NativeActivity backend. Pen and touch come from winit's basic input until the stylus
//! reader lands (`docs/PORTING.md`, step A2: `MotionEvent` with its historical samples, tool type and pressure).

use std::sync::Arc;

use winit::window::Window;

use super::PointerSource;
use crate::input::Mode;

pub use super::generic::{SystemClipboard, attach_parent_console, error_box, motion_prefs, report_fatal};

/// A phone or tablet in the hand is a tablet; `--mode desktop` (or the toolbar's mode button) says otherwise for a
/// Chromebook or a tablet with a keyboard.
pub fn detect_mode() -> Mode {
    Mode::Tablet
}

/// Opening a web address is an `Intent`, which needs the Java side; until then it is only said.
pub fn open_url(url: &str) {
    if let Some(url) = crate::links::safe_url(url) {
        eprintln!("cannot open {url} on Android yet");
    }
}

pub fn install_pointer(_window: &Arc<Window>) -> Result<Box<dyn PointerSource>, String> {
    Err("no native stylus reader on Android yet".into())
}
