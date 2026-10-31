//! Android, through winit's NativeActivity backend. Pen and touch come from winit's basic input until the stylus
//! reader lands (`docs/PORTING.md`, step A2: `MotionEvent` with its historical samples, tool type and pressure).

use std::path::PathBuf;
use std::sync::OnceLock;

use winit::platform::android::activity::AndroidApp;
use winit::raw_window_handle::HasWindowHandle;

mod host;
mod reader;

use super::unix_dirs::{first_folder, home_dir};
use super::{PointerSource, Waker};
use crate::input::Mode;

pub use super::stubs::{SystemClipboard, attach_parent_console, error_box, motion_prefs, report_fatal, system_font};

/// What the system says its look is: the configuration's night mode (the activity keeps it current as the user switches the system between light and
/// dark), unless `INKPDF_THEME` says otherwise for a run. `None` when the system does not say.
pub fn color_scheme() -> Option<crate::palette::Scheme> {
    use winit::platform::android::activity::ndk::configuration::UiModeNight;
    if let Some(scheme) = super::stubs::env_scheme() {
        return Some(scheme);
    }
    match APP.get()?.config().ui_mode_night() {
        UiModeNight::Yes => Some(crate::palette::Scheme::Dark),
        UiModeNight::No => Some(crate::palette::Scheme::Light),
        _ => None,
    }
}

/// The Java side of the app (see `host`), made when the activity starts.
static HOST: OnceLock<std::sync::Arc<host::Host>> = OnceLock::new();

/// The activity, kept for what only it can say: where the app's own folders are, and the soft keyboard.
static APP: OnceLock<AndroidApp> = OnceLock::new();

/// The activity has started: `inkpdf::run_android` hands over its `AndroidApp` before anything else.
pub fn android_app_started(app: AndroidApp) {
    if let Some(host) = host::Host::new(app.vm_as_ptr(), app.activity_as_ptr()) {
        let _ = HOST.set(std::sync::Arc::new(host));
    } else {
        eprintln!("the Java side of the app could not be reached: no file picker, clipboard or links");
    }
    let _ = APP.set(app);
}

/// The Java side as the program's own bridge (files the system hands out, the clipboard, links), if it could be reached.
pub fn bridge() -> Option<std::sync::Arc<dyn super::saf::Bridge>> {
    HOST.get().map(|h| h.clone() as std::sync::Arc<dyn super::saf::Bridge>)
}

/// The app's own folder on the shared storage (`Android/data/<package>/files`): writable with no permission, and visible to a
/// file manager or `adb`. Where saving goes until the Storage Access Framework is used (`docs/PORTING.md`, step A4).
fn own_folder() -> Option<PathBuf> {
    let app = APP.get()?;
    app.external_data_path().or_else(|| app.internal_data_path())
}

/// Where the app is asked to write where its buttons are (the file --layout makes on a desktop), for the emulator test: only when a file
/// named layout.on has been put in the app's own folder (db shell touch), so that nothing is written in ordinary use.
pub fn layout_request() -> Option<PathBuf> {
    let own = own_folder()?;
    own.join("layout.on").exists().then(|| own.join("layout.json"))
}

/// The mode a test run says to keep (`desktop` or `tablet` in a file named mode.txt in the app's own folder, as `--mode` is on a desktop), only when
/// layout.on is there too.
pub fn test_mode() -> Option<Mode> {
    let own = own_folder()?;
    own.join("layout.on").exists().then_some(())?;
    Mode::parse(std::fs::read_to_string(own.join("mode.txt")).ok()?.trim())
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

/// What the app remembers is kept in its private folder (no other app, and no file manager, sees it); in a test run (layout.on is in the app's own
/// folder) it is that folder, where `adb` can put the settings the run starts with.
pub fn config_dir() -> Option<PathBuf> {
    if layout_request().is_some() {
        return own_folder();
    }
    APP.get()?.internal_data_path()
}

/// A phone or tablet in the hand is a tablet; `--mode desktop` (or the toolbar's mode button) says otherwise for a
/// Chromebook or a tablet with a keyboard.
pub fn detect_mode() -> Mode {
    Mode::Tablet
}

/// Opening a web address is an `Intent`: the Java side does it (see `Native::open_url`, which asks the bridge first).
pub fn open_url(url: &str) {
    eprintln!("cannot open {url}: the Java side of the app is not there");
}

/// The pen, touch and key reader: it shares the activity's input queue with the toolkit and reads it first (see `reader`).
pub fn install_pointer(_window: &dyn HasWindowHandle, _wake: Waker) -> Result<Box<dyn PointerSource>, String> {
    let app = APP.get().ok_or("the activity has not been handed over")?.clone();
    Ok(Box::new(reader::AndroidPointer::new(app)))
}
