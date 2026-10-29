//! The services a platform may not have: no-ops, or the terminal where Windows has a dialog. `linux` and `android` re-export them,
//! and replace one when they get a real implementation.

use winit::raw_window_handle::HasWindowHandle;

use crate::clipboard::TextClipboard;

/// Show or hide an on-screen keyboard: nothing on a desktop (the keyboard is there). (Android has its own.)
#[cfg(not(target_os = "android"))]
pub fn set_text_input(_active: bool) {}

/// There is no terminal to join: a program started from one already has it.
pub fn attach_parent_console() {}

/// Report a problem before there is a window: on the terminal.
pub fn report_fatal(text: &str) {
    eprintln!("{text}");
}

/// What the user has asked of motion: `INKPDF_MOTION` (`off` or `on`) says it for this run. (Reading the desktop's
/// own "reduce animations" setting is a per-desktop job; until then motion is on.)
pub fn motion_prefs() -> crate::motion::Prefs {
    match std::env::var("INKPDF_MOTION").ok().as_deref() {
        Some("off") => crate::motion::Prefs::REDUCED,
        _ => crate::motion::Prefs::FULL,
    }
}

/// Text is set in the program's own Liberation Sans, which is as wide as Arial: a desktop's fonts differ, and saved text-box
/// geometry must not.
pub fn system_font() -> Option<Vec<u8>> {
    None
}

/// Tell the user something went wrong. Until each platform has a dialog of its own, on the terminal.
pub fn error_box(_window: &dyn HasWindowHandle, text: &str) {
    eprintln!("{text}");
}

/// The system clipboard for the text of text boxes. There is none yet on these platforms (it needs the display
/// server's own protocol, which differs between Wayland, X11 and Android), so this can never be made and the text
/// boxes keep their text in memory, as they do in tests.
pub enum SystemClipboard {}

impl SystemClipboard {
    pub fn new(_window: &dyn HasWindowHandle) -> Option<Self> {
        None
    }
}

impl TextClipboard for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        match *self {}
    }

    fn set(&mut self, _text: &str) {
        match *self {}
    }
}
