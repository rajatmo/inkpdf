//! What the owner of an encrypted PDF allows to be done to it (the permission bits of its security handler), as plain data the editor reads. A file that is not
//! restricted, or was opened with the owner's password, allows everything.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    /// Adding and changing comments, and so the ink, the marks and the typed text.
    pub annotate: bool,
    /// Changing the contents: what the file holds apart from comments and form fields.
    pub modify: bool,
    /// Inserting, deleting and moving pages, and making bookmarks.
    pub assemble: bool,
    pub fill_forms: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self { annotate: true, modify: true, assemble: true, fill_forms: true }
    }
}

impl Permissions {
    /// Nothing may be added to the file: it is to be read (and searched, and shown).
    pub fn read_only(self) -> bool {
        !self.annotate
    }
}
