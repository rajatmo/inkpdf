//! Windows: pen and touch from `WM_POINTER*` messages (with the history Windows batches between messages), message
//! boxes, `ShellExecute`, the clipboard, animation settings and the convertible's posture. The only module with Win32.

mod folders;
mod native;
mod pointer;
mod posture;

use winit::raw_window_handle::HasWindowHandle;

use super::{PointerSource, Waker};
use crate::input::{Mode, PointerEvent};

pub use folders::{documents_dir, places};
pub use native::{SystemClipboard, attach_parent_console, error_box, motion_prefs, open_url, report_fatal};

/// An on-screen keyboard is the system's to show on Windows (a touch keyboard comes up by itself): nothing to do.
pub fn set_text_input(_active: bool) {}

/// The mode the machine's posture calls for (a convertible in slate posture is a tablet).
pub fn detect_mode() -> Mode {
    posture::detect()
}

/// The pointer layer is a subclass of the window procedure, with its queue held per thread (the window procedure
/// runs on the thread that made the window), so this handle only remembers the window to take the subclass off.
struct WindowsPointer {
    hwnd: isize,
}

pub fn install_pointer(window: &dyn HasWindowHandle, wake: Waker) -> Result<Box<dyn PointerSource>, String> {
    let hwnd = pointer::install(window, wake)?;
    Ok(Box::new(WindowsPointer { hwnd }))
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
        pointer::uninstall(self.hwnd);
    }
}
