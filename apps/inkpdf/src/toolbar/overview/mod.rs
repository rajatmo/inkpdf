//! The pages overview: a card over the dimmed document with a picture of every page and its number, a table of contents
//! beside them when the document has one, and buttons to add pages, to edit them, to undo and redo, and to leave. This
//! is how it looks and where everything is; what it does is the editor's (see `editor::overview`).
//!
//! The pictures are the pages as the renderer draws them, with the ink over them; the numbers and titles are text drawn
//! from the app's font (a [`Label`](super::theme::Label)), so the layout and the drawing here are plain data.

mod draw;
mod layout;

pub use draw::{DragView, Hover, OverviewView, PageMotion, draw};
pub use layout::{Action, Control, MenuItem, OverviewLayout, TocHit, TocRow};
