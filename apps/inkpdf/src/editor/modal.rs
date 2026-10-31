//! Which modal layer, if any, has the person's attention. The unsaved-changes prompt and the pages
//! overview each hold their own state in [`Chrome`](super::chrome::Chrome); this is the one place that says which of them
//! is on top, so that every route (mouse, pen and touch, wheel, keys) agrees, and a new layer is added here and in
//! [`Editor::modal`] alone.

use crate::input::{Device, Phase};

use super::Editor;

/// A layer that takes the input meant for the page and the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Modal {
    /// The question asked when there are unsaved changes: over everything else.
    Prompt,
    /// The pages overview.
    Overview,
    /// The document shown as slides (see [`super::presentation`]).
    Presentation,
}

impl Editor {
    /// A pointer's press, move or release at pos, for the layer modal.
    pub(super) fn modal_pointer(&mut self, modal: Modal, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        match modal {
            Modal::Prompt => self.prompt_pointer(dev, id, phase, pos),
            Modal::Overview => self.overview_pointer(dev, id, phase, pos),
            Modal::Presentation => self.presentation_pointer(dev, id, phase, pos),
        }
    }

    /// The layer on top, if there is one. A prompt is above the overview.
    pub(super) fn modal(&self) -> Option<Modal> {
        if self.chrome.prompt.is_some() {
            Some(Modal::Prompt)
        } else if self.presentation.is_some() {
            Some(Modal::Presentation)
        } else if self.chrome.overview.is_some() {
            Some(Modal::Overview)
        } else {
            None
        }
    }
}
