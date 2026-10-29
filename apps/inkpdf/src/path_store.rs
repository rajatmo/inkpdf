//! Files by path, on the machine's own file system: [`FileStore`] for [`DocId::Path`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::browse::{self, Entry};
use crate::store::{DocId, FileStore};

/// The most directory entries looked at: the rest of a folder of a million files is not read.
const MAX_READ: usize = 20_000;

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

/// The file system, with the care a save needs: the new file is written beside the old one and swapped in.
pub struct PathStore;

fn path_of(id: &DocId) -> Result<&Path, String> {
    id.as_path().ok_or_else(|| format!("{id} is not a file on this machine's disk"))
}

impl FileStore for PathStore {
    fn read(&self, id: &DocId) -> Result<Vec<u8>, String> {
        let path = path_of(id)?;
        fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))
    }

    fn write_atomic(&self, id: &DocId, bytes: &[u8]) -> Result<(), String> {
        let path = path_of(id)?;
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let file = path.file_name().map_or_else(|| "document.pdf".into(), |n| n.to_string_lossy().into_owned());
        let tmp = dir.join(format!(".{file}.inkpdf-saving"));
        // Windows will not replace a read-only file, but a rename on Linux does, so a file made read-only is refused here,
        // and the file that replaces it keeps its permissions.
        #[cfg(not(windows))]
        let kept = fs::metadata(path).ok().map(|m| m.permissions());
        #[cfg(not(windows))]
        if kept.as_ref().is_some_and(fs::Permissions::readonly) {
            return Err(format!("could not replace {}: it is read-only", path.display()));
        }
        fs::write(&tmp, bytes).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        #[cfg(not(windows))]
        if let Some(permissions) = kept {
            let _ = fs::set_permissions(&tmp, permissions);
        }
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("could not replace {}: {e}", path.display()));
        }
        Ok(())
    }

    fn list(&self, folder: &DocId) -> Result<Vec<Entry>, String> {
        list(path_of(folder)?)
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
