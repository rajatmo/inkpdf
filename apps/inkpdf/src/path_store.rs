//! Files by path, on the machine's own file system: [`FileStore`] for [`DocId::Path`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::store::{DocId, FileStore};

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
}
