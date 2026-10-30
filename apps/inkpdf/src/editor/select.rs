//! Finishing what is under way.

use super::{Act, Editor, Request};

impl Editor {
    /// How far the selection is being dragged by the tool in use: nothing until the pointer has moved far
    /// enough for it to be a drag, not a tap.
    pub fn drag_offset(&self) -> [f32; 2] {
        self.tools.get(self.ws.tool).drag_offset()
    }

    /// Finish whatever is under way, committing it.
    pub(super) fn end_act(&mut self) {
        self.tools.end_all(&mut self.ws);
        match std::mem::take(&mut self.chrome.act) {
            Act::Idle | Act::Toolbar { .. } | Act::Context { .. } | Act::Search { .. } | Act::Theme { .. } | Act::Swallow { .. } => {}
            // Letting go of a slider keeps what it set; the panel stays open for the next one.
            Act::Slider { .. } => self.ws.requests.push(Request::SettingsChanged),
        }
        self.ws.redraw = true;
    }
}
