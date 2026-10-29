//! Android, through winit's NativeActivity backend. Pen and touch come from winit's basic input until the stylus
//! reader lands (`docs/PORTING.md`, step A2: `MotionEvent` with its historical samples, tool type and pressure).

use std::sync::Arc;

use winit::window::Window;

use std::path::PathBuf;

use super::PointerSource;
use super::generic::{existing_places, first_folder, home_dir};
use crate::browse::Place;
use crate::input::Mode;

pub use super::generic::{SystemClipboard, attach_parent_console, error_box, motion_prefs, report_fatal};

/// The shared storage as an app sees it before it has a `Context` to ask (`docs/PORTING.md`, step A4: the app's own
/// folder from `AndroidApp::external_data_path`, which needs no permission, is where saving will really go). Writing here
/// can be refused by scoped storage; the dialog shows the error when it is.
const SHARED: &str = "/storage/emulated/0";

/// Where a new document is first offered a place to be saved: Documents on the shared storage.
pub fn documents_dir() -> PathBuf {
    first_folder(&[PathBuf::from(SHARED).join("Documents"), PathBuf::from("/sdcard/Documents")], home_dir().as_deref())
}

/// The places the save dialog offers in one press.
pub fn places() -> Vec<Place> {
    let shared = PathBuf::from(SHARED);
    existing_places(vec![("Documents", shared.join("Documents")), ("Downloads", shared.join("Download")), ("Storage", shared)], |p| p.is_dir())
}

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
