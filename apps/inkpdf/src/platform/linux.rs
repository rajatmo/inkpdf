//! Linux, on Wayland or X11, both through winit. Pen and touch come from winit's basic input until the native pen
//! reader lands (`docs/PORTING.md`, step L2: the Wayland `zwp_tablet_v2` protocol, read on winit's own connection
//! right after the display wait, as `platform/windows` reads its message queue).

use std::path::PathBuf;

use winit::raw_window_handle::HasWindowHandle;

pub use super::unix_dirs::xdg_config_dir as config_dir;
use super::unix_dirs::{first_folder, home_dir, parse_user_dirs};
use super::{PointerSource, Waker};
use crate::input::Mode;

pub use super::stubs::{SystemClipboard, attach_parent_console, error_box, motion_prefs, report_fatal, set_text_input, system_font};

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

/// The look the desktop has: `INKPDF_THEME` if set, else what GNOME's settings say (`color-scheme`, or a theme named `-dark`), which most
/// desktops keep in step; `None` where that cannot be read (taken as light). X11 does not tell a window, so it is asked of the settings.
pub fn color_scheme() -> Option<crate::palette::Scheme> {
    use crate::palette::Scheme;
    if let Some(scheme) = super::stubs::env_scheme() {
        return Some(scheme);
    }
    let ask = |key: &str| -> Option<String> {
        let out = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", key])
            .stdin(std::process::Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_lowercase())
    };
    if let Some(value) = ask("color-scheme") {
        if value.contains("prefer-dark") {
            return Some(Scheme::Dark);
        }
        if value.contains("prefer-light") {
            return Some(Scheme::Light);
        }
    }
    ask("gtk-theme").map(|theme| if theme.contains("dark") { Scheme::Dark } else { Scheme::Light })
}

/// A desktop is a desktop: no Linux desktop reports a slate posture in a way that can be told from a keyboard being
/// folded away, and the toolbar's mode button (or `--mode tablet`) flips it by hand.
pub fn detect_mode() -> Mode {
    Mode::Desktop
}

/// Open a web address in the default program for it, through `xdg-open`.
pub fn open_url(url: &str) {
    match std::process::Command::new("xdg-open").arg(url).spawn() {
        // (Reaped on its own thread so that it does not stay a zombie.)
        Ok(mut child) => drop(std::thread::spawn(move || drop(child.wait()))),
        Err(e) => eprintln!("could not open {url}: {e}"),
    }
}

pub fn install_pointer(_window: &dyn HasWindowHandle, _wake: Waker) -> Result<Box<dyn PointerSource>, String> {
    Err("no native pen reader on Linux yet".into())
}
