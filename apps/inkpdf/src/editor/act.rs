//! The interaction of the window's own chrome under way: a press on a toolbar button, a slider being
//! dragged, a press that was only closing the panel. At most one at a time. (What happens on the page is
//! the tools' own: see [`crate::tools`].)

use crate::input::Device;
use crate::toolbar::{ContextItem, Item, SearchItem, Slider};

#[derive(Clone, Debug, Default)]
pub(crate) enum Act {
    #[default]
    Idle,
    Toolbar {
        dev: Device,
        id: u32,
        item: Option<Item>,
    },
    /// A press on a button of the bar under the tools (see [`crate::toolbar::ContextBar`]).
    Context {
        dev: Device,
        id: u32,
        item: Option<ContextItem>,
    },
    /// A press on a part of the search card (see [`crate::toolbar::SearchLayout`]).
    Search {
        dev: Device,
        id: u32,
        item: Option<SearchItem>,
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
