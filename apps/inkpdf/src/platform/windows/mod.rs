//! Windows: pen and touch from `WM_POINTER*` messages (with the history Windows batches between messages), message
//! boxes, `ShellExecute`, the clipboard, animation settings and the convertible's posture. The only module with Win32.

mod folders;
mod native;
mod pointer;
mod posture;

use std::sync::Arc;

use winit::window::Window;

use super::PointerSource;
use crate::input::{Mode, PointerEvent};

pub use folders::{documents_dir, places};
pub use native::{SystemClipboard, attach_parent_console, error_box, motion_prefs, open_url, report_fatal};

/// The mode the machine's posture calls for (a convertible in slate posture is a tablet).
pub fn detect_mode() -> Mode {
    posture::detect()
}

/// The pointer layer is a subclass of the window procedure, with its queue held per thread (the window procedure
/// runs on the thread that made the window), so this handle only remembers the window to take the subclass off.
struct WindowsPointer {
    window: Arc<Window>,
}

pub fn install_pointer(window: &Arc<Window>) -> Result<Box<dyn PointerSource>, String> {
    pointer::install(window)?;
    Ok(Box::new(WindowsPointer { window: window.clone() }))
}

impl PointerSource for WindowsPointer {
    fn pump(&mut self) {
        pointer::pump();
    }

    fn pending(&self) -> bool {
        pointer::pending()
    }

    fn drain(&mut self) -> Vec<PointerEvent> {
        pointer::drain()
    }

    fn take_posture_change(&mut self) -> bool {
        pointer::take_posture_change()
    }

    fn uninstall(&mut self) {
        pointer::uninstall(&self.window);
    }
}
