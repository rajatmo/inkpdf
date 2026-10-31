//! What the owner of a PDF allows to be done to it. A file whose security handler says that it may not be commented on is opened to be read: its tools that add
//! to the document are not available (the reader is, to select, copy and search; so is the pointer), nor are the edits of its pages, and the card that says so
//! is up. A file that may not have its form filled in leaves its fields alone. Nothing here is a lock: a person who has the owner's password opens the file with
//! every permission, and these are only what the file asks of a program that honours them.

use crate::permissions::Permissions;
use crate::tool::Tool;

use super::Editor;

impl Editor {
    /// Take what the owner of the document just opened allows. With nothing allowed to be added, the tool in use is the reader, and the card says why.
    pub fn set_permissions(&mut self, permissions: Permissions) {
        self.ws.permissions = permissions;
        if permissions.read_only() {
            if self.tools.get(self.ws.tool).edits() {
                self.settle();
                self.ws.tool = Tool::Select;
                self.ws.selection.clear();
                self.tools.leave_all(&mut self.ws);
                self.chrome.panel = None;
            }
            self.show_error(READ_ONLY);
        }
        self.ws.redraw = true;
    }

    /// Whether the document may not be added to.
    pub fn read_only(&self) -> bool {
        self.ws.permissions.read_only()
    }

    /// Whether `tool` can be chosen: not one that adds to the document, where that is not allowed.
    pub(super) fn tool_allowed(&self, tool: Tool) -> bool {
        !self.read_only() || !self.tools.get(tool).edits()
    }

    /// Whether the pages may be added to, taken away and moved (and bookmarked).
    pub(super) fn pages_allowed(&self) -> bool {
        !self.read_only() && self.ws.permissions.assemble
    }

    /// Whether the fields of the form may be filled in.
    pub(super) fn fields_allowed(&self) -> bool {
        self.ws.permissions.fill_forms
    }

    /// Something was asked that the document does not allow: the card says so.
    pub(super) fn refuse(&mut self) {
        self.show_error(READ_ONLY);
    }

    pub(super) fn refuse_fields(&mut self) {
        self.show_error("This form is read-only\nIts owner has not allowed filling it in.");
    }
}

const READ_ONLY: &str = "This PDF is read-only\nIts owner has not allowed adding to it. You can read, search and present it.";
