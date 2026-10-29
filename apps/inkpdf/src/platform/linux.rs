//! Linux, on Wayland or X11, both through winit. Pen and touch come from winit's basic input until the native pen
//! reader lands (`docs/PORTING.md`, step L2: the Wayland `zwp_tablet_v2` protocol, read on winit's own connection
//! right after the display wait, as `platform/windows` reads its message queue).

use std::sync::Arc;

use winit::window::Window;

use std::path::PathBuf;

use super::PointerSource;
use super::generic::{existing_places, first_folder, home_dir, parse_user_dirs};
use crate::browse::Place;
use crate::input::Mode;

pub use super::generic::{SystemClipboard, attach_parent_console, error_box, motion_prefs, report_fatal};

/// The folders the desktop names (`$XDG_*_DIR`, else `~/.config/user-dirs.dirs`), by key, with the name they have when
/// nothing says otherwise.
fn named_folder(key: &str, default: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    if let Some(dir) = std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute()) {
        return Some(dir);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map_or_else(|| home.join(".config"), PathBuf::from);
    let listed = std::fs::read_to_string(config.join("user-dirs.dirs")).ok().map(|text| parse_user_dirs(&text, &home));
    Some(listed.and_then(|mut dirs| dirs.remove(key)).unwrap_or_else(|| home.join(default)))
}

/// Where a new document is first offered a place to be saved: the Documents folder, wherever this desktop keeps it.
pub fn documents_dir() -> PathBuf {
    first_folder(&named_folder("XDG_DOCUMENTS_DIR", "Documents").into_iter().collect::<Vec<_>>(), home_dir().as_deref())
}

/// The places the save dialog offers in one press.
pub fn places() -> Vec<Place> {
    let home = home_dir();
    let mut all = vec![
        ("Documents", named_folder("XDG_DOCUMENTS_DIR", "Documents")),
        ("Desktop", named_folder("XDG_DESKTOP_DIR", "Desktop")),
        ("Downloads", named_folder("XDG_DOWNLOAD_DIR", "Downloads")),
        ("Home", home),
    ]
    .into_iter()
    .filter_map(|(label, path)| Some((label, path?)))
    .collect::<Vec<_>>();
    all.push(("Computer", PathBuf::from("/")));
    existing_places(all, |p| p.is_dir())
}

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
