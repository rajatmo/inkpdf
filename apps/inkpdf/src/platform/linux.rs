//! Linux, on Wayland or X11, both through winit. Pen and touch come from winit's basic input until the native pen
//! reader lands (`docs/PORTING.md`, step L2: the Wayland `zwp_tablet_v2` protocol, read on winit's own connection
//! right after the display wait, as `platform/windows` reads its message queue).

use std::sync::Arc;

use winit::window::Window;

use super::PointerSource;
use crate::input::Mode;

pub use super::generic::{SystemClipboard, attach_parent_console, error_box, motion_prefs, report_fatal};

/// A desktop is a desktop: no Linux desktop reports a slate posture in a way that can be told from a keyboard being
/// folded away, and the toolbar's mode button (or `--mode tablet`) flips it by hand.
pub fn detect_mode() -> Mode {
    Mode::Desktop
}

/// Open a web address in the default program for it, through `xdg-open`. The address is checked again here: only a
/// plain http, https or mailto address is ever handed over.
pub fn open_url(url: &str) {
    let Some(url) = crate::links::safe_url(url) else { return };
    match std::process::Command::new("xdg-open").arg(&url).spawn() {
        // (Reaped on its own thread so that it does not stay a zombie.)
        Ok(mut child) => drop(std::thread::spawn(move || drop(child.wait()))),
        Err(e) => eprintln!("could not open {url}: {e}"),
    }
}

pub fn install_pointer(_window: &Arc<Window>) -> Result<Box<dyn PointerSource>, String> {
    Err("no native pen reader on Linux yet".into())
}
