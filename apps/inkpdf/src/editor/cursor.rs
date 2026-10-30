//! Which cursor the mouse should show. It is what the rules (`input::decide`) say a press there would do, so the cursor never promises what a
//! press does not do: a hand over a link in the reader, an I-beam over text, a closed hand while the page is dragged, the drawing tools' own.
//! It is asked after the mouse moves or a button goes down or up, never on the pen's path (a stylus has no cursor to show), and the shell
//! changes the window's cursor only when the answer changes.

use crate::input::{Action, CursorKind, Device, Role, Target, decide};
use crate::links;
use crate::tool::Tool;

use super::Editor;

/// How near, in logical pixels, the pointer must be to a letter to be on it (as the select tool has it).
const LETTER_SLACK_PX: f32 = 5.0;
/// How near, in logical pixels, to a stroke to be on it.
const ITEM_SLACK_PX: f32 = 4.0;

impl Editor {
    /// What is under a point of the window: a link, a letter of the PDF's text, an item of ours, or paper.
    fn target_at(&self, pos: [f32; 2]) -> Target {
        let at = self.ws.to_doc(pos);
        if self.ws.tool == Tool::Select && self.tools.select.over_handle(&self.ws, pos) {
            return Target::Handle;
        }
        if links::hit(&self.links, at).is_some() {
            return Target::Link;
        }
        // (As the tools find what is under a press: the strokes' own boxes first, so that a hover costs next to nothing.)
        let slack = ITEM_SLACK_PX * self.ws.dpr / self.ws.view.zoom;
        if let Some(top) = self.ws.ink.strokes().iter().rev().find(|s| s.touches_circle(at, slack)) {
            return if top.markup.is_some() { Target::Markup } else { Target::Item };
        }
        let page = self.ws.layout.page_at(at[1]);
        let (Some(r), Some(text)) = (self.ws.layout.page(page), self.ws.text.page(page)) else { return Target::Empty };
        let (x, y) = (at[0] - r.x, at[1] - r.y);
        let slack = LETTER_SLACK_PX * self.ws.dpr / self.ws.view.zoom;
        let on_a_letter = text
            .nearest(x, y)
            .and_then(|glyph| text.glyph_rects(&[glyph]).first().copied())
            .is_some_and(|b| x >= b[0] - slack && x <= b[2] + slack && y >= b[1] - slack && y <= b[3] + slack);
        if on_a_letter { Target::TextRun } else { Target::Empty }
    }

    /// The cursor of the mouse now: the arrow over the window's own controls and while a layer above the page has the pointer, else what the
    /// rules say for the tool in use, the mode and what is under it.
    pub fn cursor_kind(&self) -> CursorKind {
        if self.modal().is_some() || !self.ws.has_doc || self.over_interface(self.cursor) {
            return CursorKind::Arrow;
        }
        // A press held on the page was decided when it went down, by the button that did it.
        let held = self.mouse_press.map(|p| p.pen.role());
        let answer = decide(self.mode, self.ws.tool, Device::Mouse, held.unwrap_or(Role::Draw), self.target_at(self.cursor));
        match (held, answer.action) {
            (Some(_), Action::Pan) => CursorKind::Grabbing,
            _ => answer.cursor,
        }
    }
}
