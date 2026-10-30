//! What the keyboard does to a selection, for every tool alike: Delete takes it away, Escape lets go of it, the
//! arrows nudge it (Shift for a bigger step). Each tool that takes the keyboard calls this first, then handles
//! what is its own (the text tool types, the shape tool takes a node away).

use crate::selection::{NUDGE, NUDGE_BIG};
use crate::workspace::Workspace;

use super::{Dir, Key};

/// Do what `key` says to the selection. `true` if it did something.
pub fn selection_key(ws: &mut Workspace, key: Key<'_>) -> bool {
    match key {
        Key::Delete | Key::Backspace => ws.delete_selection(),
        Key::Escape => {
            let held = !ws.selection.is_empty();
            ws.selection.clear();
            ws.redraw = true;
            held
        }
        Key::Arrow(dir, big) => {
            let step = if big { NUDGE_BIG } else { NUDGE };
            let (dx, dy) = match dir {
                Dir::Left => (-step, 0.0),
                Dir::Right => (step, 0.0),
                Dir::Up => (0.0, -step),
                Dir::Down => (0.0, step),
            };
            ws.nudge_selection(dx, dy)
        }
        Key::Text(_) | Key::Home(_) | Key::End(_) | Key::Chord(_) | Key::Page(_) => false,
    }
}
