//! Ink strokes: the model, geometry (simplify, hit tests, selection) and the undo history, in four parts: the strokes ([drawing]), the history\n//! of steps ([history]), what can be asked of the strokes ([query]) and the store that holds them together ([store]).
//! Pure logic with no window or GPU types, so it is unit-tested.
//!
//! Strokes live in document space (points, pages stacked as in [`crate::view::Layout`]) with the
//! page each belongs to. Width is fixed along a stroke.

mod drawing;
mod history;
mod query;
mod store;
mod stroke;

// The plane helpers are the base's; ink has always been the way the rest of the program reaches them.
pub use crate::plane::{P, dist2, rect_contains, simplify, thin};
pub use store::{InkStore, Replacement};
pub use stroke::{MarkupKind, Restyle, Stroke, Style, TextBlock};

/// A stroke as stored in a file: its page, style and points, the figure (anchor points and handles)
/// it was made from if it was a shape, the text if it is a text box, and the kind of markup if it is one (its points are then the
/// corners of the boxes it marks).
pub type PageStroke = (u32, Style, Vec<P>, Option<crate::figure::Figure>, Option<TextBlock>, Option<MarkupKind>);
