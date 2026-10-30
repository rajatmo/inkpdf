//! The text of the pages, handed to the editor by the shell as it is read (see `indexer`), and what the editor does when it changes.

use std::sync::Arc;

use crate::pagetext::PageTextView;

use super::Editor;

impl Editor {
    /// Another document: no text is known.
    pub fn forget_text(&mut self) {
        self.ws.text.clear();
        self.ws.redraw = true;
    }

    /// Which page of the file draws each page of the document. `true` if that changed anything.
    pub fn set_text_files(&mut self, files: Vec<Option<u32>>) -> bool {
        let changed = self.ws.text.set_files(files);
        if changed {
            self.ws.redraw = true;
        }
        changed
    }

    /// A page of the file has been read.
    pub fn add_text(&mut self, file: u32, text: Option<Arc<dyn PageTextView>>) {
        self.ws.text.insert(file, text);
        self.ws.redraw = true;
    }
}
