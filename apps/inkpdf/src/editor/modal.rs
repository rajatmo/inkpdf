//! Which modal layer, if any, has the person's attention. The unsaved-changes prompt and the pages
//! overview each hold their own state in [`Chrome`](super::chrome::Chrome); this is the one place that says which of them
//! is on top, so that every route (mouse, pen and touch, wheel, keys) agrees, and a new layer is added here and in
//! [`Editor::modal`] alone.

use super::Editor;

/// A layer that takes the input meant for the page and the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Modal {
    /// The question asked when there are unsaved changes: over everything else.
    Prompt,
    /// The pages overview.
    Overview,
}

impl Editor {
    /// The layer on top, if there is one. A prompt is above the overview.
    pub(super) fn modal(&self) -> Option<Modal> {
        if self.chrome.prompt.is_some() {
            Some(Modal::Prompt)
        } else if self.chrome.overview.is_some() {
            Some(Modal::Overview)
        } else {
            None
        }
    }
}
