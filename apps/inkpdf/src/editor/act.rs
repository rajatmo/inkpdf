//! The interaction of the window's own chrome under way: a press on a toolbar button, a slider being
//! dragged, a press that was only closing the panel. At most one at a time. (What happens on the page is
//! the tools' own: see [`crate::tools`].)

use crate::input::Device;
use crate::toolbar::{ContextItem, Item, SearchItem, Slider};

/// A button of one of the cards that float over the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardHit {
    /// A button of the bar under the tools (see [`crate::toolbar::ContextBar`]).
    Context(ContextItem),
    /// A part of the search card (see [`crate::toolbar::SearchLayout`]).
    Search(SearchItem),
    /// The round button under the theme button: whether the pages follow the look.
    Theme,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum Act {
    #[default]
    Idle,
    Toolbar {
        dev: Device,
        id: u32,
        item: Option<Item>,
    },
    /// A press on one of the cards that float over the page (the bar under the tools, the search card, the round button under the theme
    /// button); `hit` is the button of it that was pressed, if it was on one.
    Card {
        dev: Device,
        id: u32,
        hit: Option<CardHit>,
    },
    /// Dragging one of the sliders in the pen panel.
    Slider {
        dev: Device,
        id: u32,
        slider: Slider,
    },
    /// A press that did its work at once (it closed the panel, picked a colour); it does nothing else.
    Swallow {
        dev: Device,
        id: u32,
    },
}
