//! The toolbar's look in motion: plain numbers, 0 to 1, for how lit each button is, how enabled, how far a press has spread
//! and where the mark of the tool in use is. Whoever owns the time makes them (see `editor::motions`) and the drawing only
//! reads them, so the toolbar knows nothing of time. [`Look::resting`] is the look the state would settle to.

use super::{BUTTONS, Item, State, Toolbar};
use crate::tool::Tool;

/// The tools in the order of their buttons.
pub const TOOLS: [Tool; 4] = [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Laser];

/// Where the mark of the tool in use is, as a position along [`TOOLS`] (it slides between buttons).
fn tool_position(tool: Tool) -> f32 {
    TOOLS.iter().position(|&t| t == tool).unwrap_or(0) as f32
}

/// Whether the button can be pressed (the others are always enabled).
pub fn enabled(item: Item, state: &State) -> bool {
    match item {
        Item::Undo => state.can_undo,
        Item::Redo => state.can_redo,
        Item::DeletePage => state.can_delete,
        _ => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// How much the wash of the pointer being over a button shows.
    pub hover: [f32; BUTTONS],
    /// How far a press has spread from the middle of a button: 0 for none, nearly 1 when it has filled it.
    pub ripple: [f32; BUTTONS],
    /// 1 for a button that can be pressed, 0 for one that cannot (its icon is dim).
    pub enabled: [f32; BUTTONS],
    /// Where the mark of the tool in use is, along [`TOOLS`].
    pub tool: f32,
    /// 1 when the document has changes that are not saved (the save button is in the accent colour).
    pub unsaved: f32,
}

impl Look {
    /// What `state` looks like once nothing is moving.
    pub fn resting(state: &State, bar: &Toolbar) -> Look {
        let mut look = Look {
            hover: [0.0; BUTTONS],
            ripple: [0.0; BUTTONS],
            enabled: [1.0; BUTTONS],
            tool: tool_position(state.tool),
            unsaved: f32::from(state.unsaved),
        };
        for (slot, (item, _)) in bar.items.iter().enumerate() {
            let on = enabled(*item, state);
            look.enabled[slot] = f32::from(on);
            look.hover[slot] = f32::from(on && state.hover == Some(*item));
        }
        look
    }
}

/// `a` going to `b` as `t` goes from 0 to 1.
pub fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let channel = |i: usize| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8;
    [channel(0), channel(1), channel(2), channel(3)]
}

/// How near the mark of the tool in use is to the button at `index` along [`TOOLS`]: 1 on it, 0 a button away.
pub fn nearness(position: f32, index: usize) -> f32 {
    (1.0 - (position - index as f32).abs()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_mix_between_their_ends_and_the_mark_is_on_one_button_at_a_time_when_at_rest() {
        assert_eq!(mix([0, 100, 200, 255], [100, 200, 0, 255], 0.0), [0, 100, 200, 255]);
        assert_eq!(mix([0, 100, 200, 255], [100, 200, 0, 255], 1.0), [100, 200, 0, 255]);
        assert_eq!(mix([0, 100, 200, 255], [100, 200, 0, 255], 0.5), [50, 150, 100, 255]);
        assert_eq!(mix([0; 4], [255; 4], 9.0), [255; 4], "beyond its ends is its end");
        for (i, _) in TOOLS.iter().enumerate() {
            assert_eq!(nearness(i as f32, i), 1.0);
            assert_eq!(nearness(i as f32, (i + 1) % TOOLS.len()), 0.0);
        }
        assert_eq!(nearness(0.5, 0), 0.5);
        assert_eq!(nearness(0.5, 1), 0.5);
    }

    #[test]
    fn the_resting_look_follows_the_state() {
        let bar = Toolbar::layout([1650.0, 1350.0], 1.5);
        let state = State { can_undo: true, unsaved: true, tool: Tool::Text, hover: Some(Item::Undo), ..State::default() };
        let look = Look::resting(&state, &bar);
        let slot = |item| bar.slot(item).expect("a button");
        assert_eq!(look.tool, 2.0);
        assert_eq!(look.unsaved, 1.0);
        assert_eq!((look.hover[slot(Item::Undo)], look.enabled[slot(Item::Undo)]), (1.0, 1.0));
        assert_eq!((look.hover[slot(Item::Redo)], look.enabled[slot(Item::Redo)]), (0.0, 0.0));
        let dim = State { hover: Some(Item::Redo), ..State::default() };
        assert_eq!(Look::resting(&dim, &bar).hover[slot(Item::Redo)], 0.0, "a button that cannot be pressed does not light");
        assert!(look.ripple.iter().all(|&r| r == 0.0));
    }
}
