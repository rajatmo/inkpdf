//! Where a document's bytes are kept, as a port: the program asks a [`FileStore`] to read and write, and names a file by a
//! [`DocId`], so a file need not be a path (Android hands out `content://` addresses, and a sandbox forbids paths). What stores
//! files by path is `path_store`; a platform whose files are something else supplies its own store (`Platform::file_store`).

use std::fmt;
use std::path::{Path, PathBuf};

/// Which file: a path, or an address the platform made (an Android `content://` URI).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DocId {
    Path(PathBuf),
    #[allow(dead_code, reason = "the Android store makes these (docs/PORTING.md, step A4)")]
    Uri(String),
}

impl DocId {
    /// The path, for a file kept by path.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            DocId::Path(path) => Some(path),
            DocId::Uri(_) => None,
        }
    }

    /// The name to show for it: a path's file name, or the last part of an address.
    pub fn name(&self) -> String {
        match self {
            DocId::Path(path) => path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
            DocId::Uri(uri) => {
                // (A content address keeps the folder inside one part, as `primary%3ADocuments%2Fnotes.pdf`.)
                let last = uri.rsplit(['/', ':']).next().unwrap_or(uri);
                last.rsplit("%2F").next().unwrap_or(last).to_string()
            }
        }
    }
}

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocId::Path(path) => write!(f, "{}", path.display()),
            DocId::Uri(uri) => f.write_str(uri),
        }
    }
}

/// Reading and writing files. Errors are already worded for the person. A store may be asked from any thread.
pub trait FileStore: Send + Sync {
    /// The file's bytes.
    fn read(&self, id: &DocId) -> Result<Vec<u8>, String>;
    /// Replace the file with `bytes`, or make it. All or nothing: a failure part-way leaves the file as it was, and a file the
    /// person has made read-only is refused.
    fn write_atomic(&self, id: &DocId, bytes: &[u8]) -> Result<(), String>;
    /// The name to show for the file: what the store knows it as (Android's picker has the file's real name, which its address does not
    /// say), else the one [`DocId::name`] reads from the address.
    fn display_name(&self, id: &DocId) -> String {
        id.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_the_last_part_of_a_path_or_an_address() {
        assert_eq!(DocId::Path("/home/me/Documents/notes.pdf".into()).name(), "notes.pdf");
        assert_eq!(DocId::Uri("content://com.android.externalstorage.documents/document/primary%3ADocuments%2Fnotes.pdf".into()).name(), "notes.pdf");
        assert_eq!(DocId::Uri("content://media/external/file/42".into()).name(), "42");
        assert_eq!(DocId::Path("/".into()).name(), "/");
    }

    #[test]
    fn only_a_path_is_a_path() {
        assert_eq!(DocId::Path("/a".into()).as_path(), Some(Path::new("/a")));
        assert_eq!(DocId::Uri("content://x".into()).as_path(), None);
    }
}
