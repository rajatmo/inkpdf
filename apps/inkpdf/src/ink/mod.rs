//! Ink strokes: the model, geometry (simplify, hit tests, selection) and the undo history.
//! Pure logic with no window or GPU types, so it is unit-tested.
//!
//! Strokes live in document space (points, pages stacked as in [`crate::view::Layout`]) with the
//! page each belongs to. Width is fixed along a stroke.

mod store;
mod stroke;

// The plane helpers are the base's; ink has always been the way the rest of the program reaches them.
pub use crate::plane::{P, dist2, rect_contains, simplify, thin};
pub use store::{InkStore, Replacement};
pub use stroke::{Restyle, Stroke, Style, TextBlock};

/// A stroke as stored in a file: its page, style and points, the figure (anchor points and handles)
/// it was made from if it was a shape, and the text if it is a text box.
pub type PageStroke = (u32, Style, Vec<P>, Option<crate::figure::Figure>, Option<TextBlock>);
