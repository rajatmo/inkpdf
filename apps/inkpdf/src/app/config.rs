//! Where the settings live on disk, and reading and writing them. (What they are is `settings`, which does no I/O.)

use std::fs;
use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// The file whose presence beside the exe makes the app portable: its settings are kept there too.
const PORTABLE_MARKER: &str = "portable.txt";

/// Where the settings file lives: `settings.txt` beside the exe if `portable.txt` is there too (a copy on a stick leaves
/// nothing on the PC), else `InkPDF/settings.txt` in the platform's configuration folder (`%APPDATA%` on Windows,
/// `$XDG_CONFIG_HOME` or `~/.config` on Linux, the app's own folder on Android).
fn path(config_dir: Option<PathBuf>) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from));
    settings_path(exe_dir.as_deref(), config_dir)
}

fn settings_path(exe_dir: Option<&Path>, config_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = exe_dir.filter(|d| d.join(PORTABLE_MARKER).is_file()) {
        return Some(dir.join("settings.txt"));
    }
    Some(config_dir?.join("InkPDF").join("settings.txt"))
}

/// The saved settings, or the defaults when there are none (or they cannot be read).
pub fn load(config_dir: Option<PathBuf>) -> Settings {
    path(config_dir).and_then(|p| fs::read_to_string(p).ok()).map_or_else(Settings::default, |t| Settings::parse(&t))
}

/// Remember the settings. A failure is reported, not fatal: losing a colour must never stop work.
pub fn save(config_dir: Option<PathBuf>, settings: &Settings) -> Result<(), String> {
    let path = path(config_dir).ok_or("no application-data folder")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    // Written beside the file and renamed over it, so a crash half-way leaves the old settings.
    let temp = path.with_extension("tmp");
    fs::write(&temp, settings.to_text()).map_err(|e| format!("could not write {}: {e}", temp.display()))?;
    fs::rename(&temp, &path).map_err(|e| format!("could not replace {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portable_marker_beside_the_exe_keeps_the_settings_there_else_they_go_to_the_config_folder() {
        let dir = std::env::temp_dir().join(format!("inkpdf-portable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = PathBuf::from("C:\\appdata");
        assert_eq!(settings_path(Some(&dir), Some(config.clone())), Some(config.join("InkPDF").join("settings.txt")));
        std::fs::write(dir.join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(settings_path(Some(&dir), Some(config)), Some(dir.join("settings.txt")));
        assert_eq!(settings_path(Some(&dir), None), Some(dir.join("settings.txt")), "no config folder needed when portable");
        assert_eq!(settings_path(None, None), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_is_saved_is_read_back_by_the_next_session() {
        let dir = std::env::temp_dir().join(format!("inkpdf-settings-{}", std::process::id()));
        let s = Settings { smoothing: 0.8, ..Settings::default() };
        save(Some(dir.clone()), &s).expect("saved");
        // (A portable.txt beside the test binary would send it elsewhere; there is none.)
        assert_eq!(load(Some(dir.clone())), s);
        std::fs::remove_dir_all(&dir).ok();
    }
}
