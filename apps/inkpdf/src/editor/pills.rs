//! The pills that fold into round buttons when the window is too narrow for all three whole (see [`crate::toolbar::Pill`]). A press on a round
//! button unfolds its pill over the others; the pill folds again when something in it is pressed (undo, redo and the look's button
//! keep it open: they are pressed again and again), when a press is anywhere else, or on Esc. Every device does all of it with a press.

use crate::toolbar::{Item, Pill};

use super::Editor;

impl Editor {
    /// A press of a round button: its pill unfolds, or if it was the one open, folds again.
    pub(super) fn toggle_pill(&mut self, pill: Pill) {
        self.chrome.pill = if self.chrome.pill == Some(pill) { None } else { Some(pill) };
        self.ws.redraw = true;
    }

    /// The open pill folds again.
    pub(super) fn fold_pill(&mut self) {
        if self.chrome.pill.take().is_some() {
            self.ws.redraw = true;
        }
    }

    /// The window changed: a pill that no longer folds is not open (it would unfold again if the window were narrowed once more).
    pub(super) fn refold_pills(&mut self) {
        if self.chrome.pill.is_some_and(|pill| !self.toolbar().folds(pill)) {
            self.fold_pill();
        }
    }

    /// A press is beginning at `pos`: the open pill folds unless the press is on it, or on the round button of the look that hangs from it.
    pub(super) fn dismiss_pill(&mut self, pos: [f32; 2]) {
        let Some(open) = self.chrome.pill else { return };
        let on_pill = crate::ink::rect_contains(self.toolbar().pills[open.index()], pos);
        let on_button = self.theme_layout().is_some_and(|c| c.contains(pos));
        if !on_pill && !on_button {
            self.fold_pill();
        }
    }

    /// What pressing `item` does to the open pill: it folds, unless it is a button that is pressed again and again.
    pub(super) fn pill_after(&mut self, item: Item) {
        if !matches!(item, Item::Undo | Item::Redo | Item::Theme | Item::Fold(_)) {
            self.fold_pill();
        }
    }
}
