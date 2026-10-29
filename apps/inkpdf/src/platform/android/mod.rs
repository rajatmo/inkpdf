//! Android, through winit's NativeActivity backend. Pen and touch come from winit's basic input until the stylus
//! reader lands (`docs/PORTING.md`, step A2: `MotionEvent` with its historical samples, tool type and pressure).

use std::path::PathBuf;
use std::sync::OnceLock;

use winit::platform::android::activity::AndroidApp;
use winit::raw_window_handle::HasWindowHandle;

mod reader;

use super::generic::{existing_places, first_folder, home_dir};
use super::{PointerSource, Waker};
use crate::browse::Place;
use crate::input::Mode;

pub use super::generic::{SystemClipboard, system_font, attach_parent_console, error_box, motion_prefs, report_fatal};

/// The activity, kept for what only it can say: where the app's own folders are, and the soft keyboard.
static APP: OnceLock<AndroidApp> = OnceLock::new();

/// The activity has started: `inkpdf::run_android` hands over its `AndroidApp` before anything else.
pub fn android_app_started(app: AndroidApp) {
    let _ = APP.set(app);
}

/// The app's own folder on the shared storage (`Android/data/<package>/files`): writable with no permission, and visible to a
/// file manager or `adb`. Where saving goes until the Storage Access Framework is used (`docs/PORTING.md`, step A4).
fn own_folder() -> Option<PathBuf> {
    let app = APP.get()?;
    app.external_data_path().or_else(|| app.internal_data_path())
}

/// Show or hide the soft keyboard, as the app starts or stops wanting typed text (a text box, the name of a file).
pub fn set_text_input(active: bool) {
    if let Some(app) = APP.get() {
        if active {
            app.show_soft_input(true);
        } else {
            app.hide_soft_input(true);
        }
    }
}

/// The first-start document: a short welcome with a page to write on, put in the app's Documents folder (once, so that what
/// is drawn on it and saved is not overwritten) and opened.
pub fn welcome_document() -> Option<PathBuf> {
    const WELCOME: &[u8] = include_bytes!("../../../assets/android/welcome.pdf");
    let path = documents_dir().join("Welcome to InkPDF.pdf");
    if !path.exists() {
        std::fs::write(&path, WELCOME).ok()?;
    }
    Some(path)
}

/// The shared storage as an app sees it before it has a `Context` to ask (`docs/PORTING.md`, step A4: the app's own
/// folder from `AndroidApp::external_data_path`, which needs no permission, is where saving will really go). Writing here
/// can be refused by scoped storage; the dialog shows the error when it is.
const SHARED: &str = "/storage/emulated/0";

/// Where a new document is first offered a place to be saved: a Documents folder in the app's own folder (made if need be),
/// else the one on the shared storage.
pub fn documents_dir() -> PathBuf {
    if let Some(own) = own_folder().map(|f| f.join("Documents")) {
        let _ = std::fs::create_dir_all(&own);
        if own.is_dir() {
            return own;
        }
    }
    first_folder(&[PathBuf::from(SHARED).join("Documents"), PathBuf::from("/sdcard/Documents")], home_dir().as_deref())
}

/// The places the save dialog offers in one press.
pub fn places() -> Vec<Place> {
    let shared = PathBuf::from(SHARED);
    existing_places(vec![("Documents", shared.join("Documents")), ("Downloads", shared.join("Download")), ("Storage", shared)], |p| p.is_dir())
}

/// What the app remembers is kept in its private folder (no other app, and no file manager, sees it).
pub fn config_dir() -> Option<PathBuf> {
    APP.get()?.internal_data_path()
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

/// The pen, touch and key reader: it shares the activity's input queue with the toolkit and reads it first (see `reader`).
pub fn install_pointer(_window: &dyn HasWindowHandle, _wake: Waker) -> Result<Box<dyn PointerSource>, String> {
    let app = APP.get().ok_or("the activity has not been handed over")?.clone();
    Ok(Box::new(reader::AndroidPointer::new(app)))
}
