//! The card that says something went wrong, for the platforms with no message box of their own (see `toolbar/error_card.rs`): what the shell is told to
//! say (`show_error`: a heading on the first line, the reason after it), where it is, and putting it away (its close button, or Esc). It does not time out.

use crate::toolbar::{ErrorLayout, ErrorView, error_lines, error_wrap};

use super::Editor;

impl Editor {
    /// Say that something went wrong: `text`'s first line is the heading, the rest the reason. (One thing at a time: a new one replaces the old.)
    pub fn show_error(&mut self, text: &str) {
        let mut lines = text.lines();
        let heading = lines.next().unwrap_or_default().trim().to_string();
        let reason = lines.collect::<Vec<_>>().join("\n");
        self.error = Some((heading, reason));
        self.ws.redraw = true;
    }

    /// Put the card away.
    pub fn close_error(&mut self) {
        if self.error.take().is_some() {
            self.ws.redraw = true;
        }
    }

    #[cfg(test)]
    pub fn error_open(&self) -> bool {
        self.error.is_some()
    }

    /// The card where it is, and what it says, if there is something to say.
    pub(super) fn error_parts(&self) -> Option<(ErrorLayout, ErrorView)> {
        let (heading, reason) = self.error.as_ref()?;
        let (viewport, dpr) = (self.ws.view.viewport, self.ws.dpr);
        // (The room for a line depends on the card, and the card's height on the lines: the first is asked of a card with room for the most.)
        let room = ErrorLayout::new(viewport, dpr, error_lines()).room;
        let lines = error_wrap(reason, ErrorLayout::text_size(dpr), room);
        Some((ErrorLayout::new(viewport, dpr, lines.len()), ErrorView { heading: heading.clone(), lines }))
    }

    pub fn error_layout(&self) -> Option<ErrorLayout> {
        self.error_parts().map(|(layout, _)| layout)
    }

    pub fn error_view(&self) -> Option<ErrorView> {
        self.error_parts().map(|(_, view)| view)
    }
}
