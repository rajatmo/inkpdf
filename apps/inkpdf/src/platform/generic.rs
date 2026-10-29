//! What every non-Windows platform does the same way, so that `linux` and `android` hold only what differs.

use std::sync::Arc;

use winit::window::Window;

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

/// Tell the user something went wrong. Until each platform has a dialog of its own, on the terminal.
pub fn error_box(_window: &Arc<Window>, text: &str) {
    eprintln!("{text}");
}

/// The system clipboard for the text of text boxes. There is none yet on these platforms (it needs the display
/// server's own protocol, which differs between Wayland, X11 and Android), so this can never be made and the text
/// boxes keep their text in memory, as they do in tests.
pub enum SystemClipboard {}

impl SystemClipboard {
    pub fn new(_window: &Window) -> Option<Self> {
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

// ── known folders (Linux and Android) ────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::browse::Place;

/// The folders a desktop names in `user-dirs.dirs` (`XDG_DOCUMENTS_DIR="$HOME/Documents"`), by their keys. Anything
/// that is not a quoted absolute path, or `$HOME` followed by a path, is left out: the file is the user's, and not trusted
/// to be well made.
#[cfg_attr(target_os = "android", allow(dead_code, reason = "Android has no user-dirs file; the parser is tested wherever there is one"))]
pub fn parse_user_dirs(text: &str, home: &Path) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else { continue };
        if line.starts_with('#') || !key.starts_with("XDG_") || !key.ends_with("_DIR") {
            continue;
        }
        let value = value.trim().trim_matches('"');
        let path = match value.strip_prefix("$HOME") {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
            _ if value.starts_with('/') => PathBuf::from(value),
            _ => continue,
        };
        found.insert(key.to_string(), path);
    }
    found
}

/// The home folder.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// The first of `candidates` that is a folder, else `home` if it is, else wherever the program is, else the top.
pub fn first_folder(candidates: &[PathBuf], home: Option<&Path>) -> PathBuf {
    candidates
        .iter()
        .map(PathBuf::as_path)
        .chain(home)
        .find(|p| p.is_dir())
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `places` that exist as folders, each once (the first label a folder is given is the one it keeps).
pub fn existing_places(places: Vec<(&str, PathBuf)>, exists: impl Fn(&Path) -> bool) -> Vec<Place> {
    let mut out: Vec<Place> = Vec::new();
    for (label, path) in places {
        if exists(&path) && !out.iter().any(|p| p.path == path) {
            out.push(Place { label: label.to_string(), path });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folders_a_desktop_names_are_read_from_its_file_and_nothing_untrusted_is() {
        let home = Path::new("/home/me");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOCUMENTS_DIR=\"$HOME/Dokumente\"\nXDG_DOWNLOAD_DIR=\"/data/dl\"\nXDG_MUSIC_DIR=\"$HOME\"\nXDG_VIDEOS_DIR=\"$HOMEBASE/x\"\nXDG_PICTURES_DIR=\"relative/path\"\nNOT_XDG=\"/x\"\n#XDG_TEMPLATES_DIR=\"/t\"\nXDG_PUBLICSHARE_DIR\n";
        let dirs = parse_user_dirs(text, home);
        assert_eq!(dirs["XDG_DESKTOP_DIR"], Path::new("/home/me/Desktop"));
        assert_eq!(dirs["XDG_DOCUMENTS_DIR"], Path::new("/home/me/Dokumente"), "a translated name is used, not Documents");
        assert_eq!(dirs["XDG_DOWNLOAD_DIR"], Path::new("/data/dl"));
        assert_eq!(dirs["XDG_MUSIC_DIR"], Path::new("/home/me"), "$HOME alone is home");
        for left_out in ["XDG_VIDEOS_DIR", "XDG_PICTURES_DIR", "NOT_XDG", "XDG_TEMPLATES_DIR", "XDG_PUBLICSHARE_DIR"] {
            assert!(!dirs.contains_key(left_out), "{left_out}");
        }
    }

    #[test]
    fn the_start_folder_is_the_first_that_exists_then_home_then_somewhere() {
        let dir = std::env::temp_dir().join(format!("inkpdf-folders-{}", std::process::id()));
        let docs = dir.join("Documents");
        std::fs::create_dir_all(&docs).unwrap();
        let missing = dir.join("missing");
        assert_eq!(first_folder(&[missing.clone(), docs.clone()], Some(&dir)), docs);
        assert_eq!(first_folder(std::slice::from_ref(&missing), Some(&dir)), dir, "no Documents folder: home");
        let somewhere = first_folder(std::slice::from_ref(&missing), Some(&missing));
        assert!(somewhere.is_dir(), "never a folder that is not there: {somewhere:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn places_that_are_not_there_or_are_the_same_folder_twice_are_left_out() {
        let there = |p: &Path| p != Path::new("/gone");
        let places = existing_places(
            vec![
                ("Documents", "/home/me/Documents".into()),
                ("Desktop", "/gone".into()),
                ("Home", "/home/me".into()),
                ("Also home", "/home/me".into()),
            ],
            there,
        );
        let labels: Vec<&str> = places.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["Documents", "Home"]);
    }
}
