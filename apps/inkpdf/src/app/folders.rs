//! Listing a folder for the Save As dialog: its subfolders and PDFs, read off the main thread (a network drive can take
//! seconds to answer, and a frame must not wait for it) and handed back through a channel the shell empties as it serves the
//! editor's requests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;

use winit::window::Window;

use crate::browse::{self, Entry};

/// The most directory entries looked at: the rest of a folder of a million files is not read.
const MAX_READ: usize = 20_000;

/// What was found in `dir`, or why it could not be read.
pub(super) struct Listed {
    pub dir: PathBuf,
    pub result: Result<Vec<Entry>, String>,
}

/// Whether a name or attribute says the entry is hidden from the person (a dot file, or what Windows hides).
fn hidden(name: &str, meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;
        if meta.file_attributes() & HIDDEN_OR_SYSTEM != 0 {
            return true;
        }
    }
    #[cfg(not(windows))]
    let _ = meta;
    name.starts_with('.')
}

/// The subfolders and PDFs of `dir`, not hidden, in no particular order. A link is followed, so a link to a folder is a folder.
pub(super) fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = fs::read_dir(dir).map_err(|e| format!("Cannot open this folder: {e}"))?;
    let mut entries = Vec::new();
    for item in read.take(MAX_READ).flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        let Ok(meta) = fs::metadata(item.path()) else { continue };
        if hidden(&name, &meta) {
            continue;
        }
        if meta.is_dir() {
            entries.push(Entry { name, folder: true });
        } else if meta.is_file() && browse::has_extension(&name) {
            entries.push(Entry { name, folder: false });
        }
    }
    Ok(entries)
}

/// Read `dir` on a thread of its own; the answer goes through `tx` and the window is woken to take it.
pub(super) fn spawn(dir: PathBuf, window: Arc<Window>, tx: Sender<Listed>) {
    let spawned = std::thread::Builder::new().name("inkpdf-list-folder".into()).spawn(move || {
        let result = list(&dir);
        if tx.send(Listed { dir, result }).is_ok() {
            window.request_redraw();
        }
    });
    if let Err(e) = spawned {
        eprintln!("could not start a thread to read a folder: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkpdf-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_listing_has_the_folders_and_the_pdfs_and_nothing_hidden_or_of_another_kind() {
        let dir = temp("list");
        fs::create_dir(dir.join("Invoices")).unwrap();
        fs::create_dir(dir.join(".cache")).unwrap();
        for file in ["a.pdf", "B.PDF", "notes.txt", ".hidden.pdf", "pdf"] {
            fs::write(dir.join(file), b"x").unwrap();
        }
        let mut found = list(&dir).expect("listing");
        browse::sort(&mut found);
        let names: Vec<(&str, bool)> = found.iter().map(|e| (e.name.as_str(), e.folder)).collect();
        assert_eq!(names, [("Invoices", true), ("a.pdf", false), ("B.PDF", false)]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_cannot_be_read_says_why_and_a_file_is_not_a_folder() {
        let dir = temp("missing");
        let gone = dir.join("nothing here");
        let why = list(&gone).expect_err("no such folder");
        assert!(why.starts_with("Cannot open this folder"), "{why}");
        fs::write(dir.join("f.pdf"), b"x").unwrap();
        assert!(list(&dir.join("f.pdf")).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_folder_is_a_folder_and_a_broken_link_is_left_out() {
        let dir = temp("links");
        fs::create_dir(dir.join("real")).unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("alias")).unwrap();
        std::os::unix::fs::symlink(dir.join("gone"), dir.join("broken")).unwrap();
        let mut found = list(&dir).expect("listing");
        browse::sort(&mut found);
        assert_eq!(found, [Entry { name: "alias".into(), folder: true }, Entry { name: "real".into(), folder: true }]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
