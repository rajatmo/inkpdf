//! Editing commands, shared by the toolbar and the other parts of the editor.

use super::Editor;

impl Editor {
    /// Take back the tool's own pending work (typing, the last node of a stroke) if there is some, else
    /// the last step.
    pub(crate) fn undo(&mut self) {
        if !self.tools.get_mut(self.ws.tool).undo(&mut self.ws) && self.ink.undo() {
            self.prune_selection();
        }
    }

    pub(crate) fn redo(&mut self) {
        if !self.tools.get_mut(self.ws.tool).redo(&mut self.ws) && self.ink.redo() {
            self.prune_selection();
        }
    }

    /// Select all the ink on the page being looked at (for screenshots: `--select-all`).
    pub(crate) fn select_all_on_page(&mut self) {
        let page = self.view.current_page(&self.layout) as u32;
        self.selection = self.ink.strokes().iter().filter(|s| s.page == page).map(|s| s.id).collect();
    }
}
