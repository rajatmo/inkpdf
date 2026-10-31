//! Handing the page's input to the tool in use. The session does not know what any tool does with it: it
//! asks the tool's [`InputPolicy`](crate::tools::InputPolicy) which devices it listens to, and passes on
//! the pointer and the keyboard. The accessors here are how the shell and the tests look at what a tool
//! holds.

use crate::figure::Figure;
use crate::geometry::Shape;
use crate::input::{Device, Mode, PenButtons, Phase, Sample};
use crate::tool::Tool;
use crate::tools::{Key, MouseUse, PageEvent, TextEdit};

use super::modal::Modal;
use super::{Editor, Request};

use crate::input::MOUSE_STYLUS_ID as MOUSE_AS_STYLUS_ID;

impl Editor {
    /// The text box being typed into, for drawing it.
    pub fn text_edit(&self) -> Option<&TextEdit> {
        self.tools.text.edit()
    }

    /// The polyline or spline being drawn by taps, if any.
    #[cfg(test)]
    pub fn draft(&self) -> Option<&crate::tools::Draft> {
        self.tools.shape.draft.as_ref()
    }

    /// What to draw for the stroke being drawn by taps: its outline and its nodes.
    #[cfg(test)]
    pub fn draft_preview(&self) -> Option<(Vec<crate::ink::P>, Vec<crate::ink::P>)> {
        self.tools.shape.draft_preview()
    }

    /// End the stroke being drawn by taps as it is (open), so that it is part of the document.
    pub fn finish_draft(&mut self) {
        self.tools.shape.finish_draft(&mut self.ws);
    }

    /// The single selected shape, and the figure to show for it, while the shape tool is in use.
    pub fn editing(&self) -> Option<(u64, &Figure)> {
        if self.ws.tool != Tool::Geometry {
            return None;
        }
        self.tools.shape.editing(&self.ws)
    }

    /// The anchor of the shape being edited that was pressed last.
    #[cfg(test)]
    pub fn selected_anchor(&self) -> Option<usize> {
        self.tools.shape.selected_anchor(&self.ws)
    }

    /// Where a drag has snapped to an anchor of another shape.
    #[cfg(test)]
    pub fn snap(&self) -> Option<crate::ink::P> {
        self.tools.shape.snap()
    }

    /// Whether nothing is under way: no press on the window, no gesture of any tool.
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        matches!(self.chrome.act, super::Act::Idle) && !self.tools.busy()
    }

    /// Whether the keyboard is heard: a text box is being typed into.
    pub fn is_typing(&self) -> bool {
        match self.modal() {
            Some(Modal::Prompt) => false,
            Some(Modal::Overview) => false,
            None => self.search.open || self.form_typing.is_some() || (self.ws.tool == Tool::Text && self.tools.text.is_typing()),
        }
    }

    /// Type `typed` at the end of the text (see [`crate::text::clean`]). Heard only while typing.
    #[cfg(test)]
    pub fn type_text(&mut self, typed: &str) {
        self.key(Key::Text(typed));
    }

    /// Take the last character away (a new line goes whole).
    #[cfg(test)]
    pub fn backspace(&mut self) {
        self.key(Key::Backspace);
    }

    /// Escape ends the edit of the text box: the typing becomes an undo step of its own.
    #[cfg(test)]
    pub fn escape(&mut self) {
        self.key(Key::Escape);
    }

    /// End the edit of the text box, as an undo step of its own, whatever tool is in use.
    #[cfg(test)]
    pub fn finish_text(&mut self) {
        self.tools.text.finish_text(&mut self.ws);
    }

    /// A key that is not a letter with Ctrl, if the tool in use listens to the keyboard (see [`Key`]). `true` if it was
    /// used. The window's own presses of the bar under the tools go through the tool alike, so a key and a button
    /// that mean the same thing do the same thing.
    pub fn press(&mut self, key: Key<'_>) -> bool {
        self.key(key)
    }

    /// The keyboard, if the tool in use listens to it.
    fn key(&mut self, key: Key<'_>) -> bool {
        match self.modal() {
            Some(Modal::Prompt) => return false,
            Some(Modal::Overview) => return self.overview_key(key),
            None => {}
        }

        // Esc puts away the round button under the theme button.
        if self.chrome.theme_menu && key == Key::Escape {
            self.close_theme_menu();
            return true;
        }
        // And folds an open pill.
        if self.chrome.pill.is_some() && key == Key::Escape {
            self.fold_pill();
            return true;
        }
        // The card of the search has the keyboard while it is open.
        if self.search.open && self.search_key(key) {
            return true;
        }
        // And so does a field of a form that is being typed into.
        if self.form_typing.is_some() && self.form_key(key) {
            self.ws.redraw = true;
            return true;
        }
        let mode = self.mode;
        let tool = self.tools.get_mut(self.ws.tool);
        let used = tool.policy(mode).keyboard && tool.key(&mut self.ws, key);
        // What no tool took scrolls, as in any reader.
        let used = used || self.reader_key(key);
        self.ws.redraw |= used;
        used
    }

    /// How the machine is being used now (see [`Mode`]).
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Change the mode (the machine was turned into a tablet, or back). Whatever the devices were in the middle
    /// of is ended first, so a stroke or a drag is never left half done under rules that no longer apply.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        self.tools.end_all(&mut self.ws);
        self.mouse_press = None;
        self.mode = mode;
        self.ws.redraw = true;
    }

    /// Whether the mouse draws, as a stylus does, with the tool in use in the mode in use.
    pub(super) fn mouse_draws(&self) -> bool {
        self.tools.get(self.ws.tool).policy(self.mode).mouse == MouseUse::AsStylus
    }

    /// A keyboard shortcut: `key` is the letter pressed with Ctrl. While a text box is being typed into, A, C, X and
    /// V are the text's (select all, copy, cut, paste), in both modes: a keyboard on a tablet types. Otherwise they
    /// are the desktop mode's only (a tablet has no keyboard to rely on): Z undoes (with Shift, redoes), Y redoes,
    /// S saves; A selects all of what the tool made, C copies, X cuts, V pastes, D duplicates. Returns whether it
    /// was one.
    pub fn shortcut(&mut self, key: &str, shift: bool) -> bool {
        let letter = key.to_ascii_lowercase();
        match self.modal() {
            Some(Modal::Prompt) => return false,
            // The overview takes the document's own shortcuts (undo, redo, save) and leaves the rest of them alone.
            Some(Modal::Overview) if !matches!((letter.as_str(), self.mode), ("z" | "y" | "s", Mode::Desktop)) => {
                return self.mode == Mode::Desktop && self.overview_shortcut(&letter);
            }
            _ => {}
        }
        if self.is_typing()
            && let Some(c) = letter.chars().next().filter(|c| letter.len() == 1 && matches!(c, 'a' | 'c' | 'x' | 'v'))
        {
            self.key(Key::Chord(c));
            self.ws.redraw = true;
            return true;
        }
        if self.mode != Mode::Desktop {
            return false;
        }
        // With the text selection, Ctrl+A takes the page's text and Ctrl+C copies it (the keyboard of that tool, not the items').
        if self.ws.tool == Tool::Select && matches!(letter.as_str(), "a" | "c") {
            let used = letter.chars().next().is_some_and(|c| self.key(Key::Chord(c)));
            self.ws.redraw = true;
            return used;
        }
        if self.zoom_shortcut(&letter) {
            return true;
        }
        match (letter.as_str(), shift) {
            ("z", false) => self.undo(),
            ("z", true) | ("y", _) => self.redo(),
            ("f", _) => self.toggle_search(),
            ("s", true) => {
                self.settle();
                self.ws.requests.push(Request::SaveAs);
            }
            ("s", false) => {
                self.settle();
                self.ws.requests.push(Request::Save);
            }
            ("a", _) => {
                self.settle();
                self.ws.select_all();
            }
            ("c", _) => {
                self.ws.copy_selection();
            }
            ("x", _) => {
                self.settle();
                self.ws.cut_selection();
            }
            ("v", _) => {
                self.settle();
                self.ws.paste();
            }
            ("d", _) => {
                self.settle();
                self.ws.duplicate_selection();
            }
            _ => return false,
        }
        self.ws.redraw = true;
        true
    }

    /// The colour or size in the panel changed.
    pub(super) fn restyle_tool(&mut self) {
        self.tools.get_mut(self.ws.tool).restyled(&mut self.ws);
    }

    /// Choose the shape the shape tool makes.
    pub(super) fn pick_shape(&mut self, shape: Shape) {
        if shape != self.ws.styles.shape {
            self.finish_draft();
            self.ws.styles.shape = shape;
            self.ws.requests.push(Request::SettingsChanged);
            self.ws.redraw = true;
        }
    }

    /// Hand a stylus or mouse message on the page to the tool in use, if it listens to that device (or
    /// already has a gesture of it under way).
    pub(super) fn route_to_tool(&mut self, dev: Device, id: u32, phase: Phase, samples: &[Sample], pen: PenButtons) {
        let mode = self.mode;
        let tool = self.tools.get_mut(self.ws.tool);
        let policy = tool.policy(mode);
        // A mouse standing in for a stylus is handed over as one, with an id of its own: the tool is written for
        // the stylus and cannot tell.
        let (dev, id) = match (dev, policy.mouse) {
            (Device::Mouse, MouseUse::AsStylus) => (Device::Pen, MOUSE_AS_STYLUS_ID),
            _ => (dev, id),
        };
        let listens = match dev {
            Device::Pen => policy.stylus,
            Device::Mouse => policy.mouse == MouseUse::Itself,
            Device::Touch => false,
        };
        if listens || tool.owns(dev, id) {
            tool.pointer(&mut self.ws, &PageEvent { dev, id, phase, samples, pen });
            self.watch_link_click(dev, id, phase, samples);
        }
    }

    /// A mouse message at `pos` (screen) on the page, with the buttons that stand for the stylus's.
    pub(super) fn route_mouse(&mut self, phase: Phase, pos: [f32; 2], pen: PenButtons) {
        let samples = [Sample { pos, pressure: 0.5, t_ms: (self.clock)() }];
        self.route_to_tool(Device::Mouse, 0, phase, &samples, pen);
    }

    /// Whether a point is on a toolbar or the tool panel.
    pub(super) fn over_interface(&self, p: [f32; 2]) -> bool {
        let bar = self.toolbar();
        bar.contains(p)
            || self.context_bar().is_some_and(|c| c.contains(p))
            || self.search_layout().is_some_and(|c| c.contains(p))
            || self.theme_layout().is_some_and(|c| c.contains(p))
            || self.drawer().is_some_and(|d| crate::ink::rect_contains(d, p))
            || self
                .chrome
                .panel
                .is_some_and(|s| bar.tool_panel(self.ws.tool, self.ws.view.viewport[0], s.editing.is_some()).is_some_and(|panel| panel.contains(p)))
    }

    /// The pen hovers at `pos` (or has gone): the tool's dot or draft follows it, except over the toolbars.
    pub(super) fn route_hover(&mut self, pos: Option<[f32; 2]>) {
        let free = pos.filter(|&p| self.ws.has_doc && !self.over_interface(p));
        let here;
        let (phase, samples): (Phase, &[Sample]) = match free {
            Some(pos) => {
                here = [Sample { pos, pressure: 0.0, t_ms: (self.clock)() }];
                (Phase::Hover, &here)
            }
            None => (Phase::Leave, &[]),
        };
        self.route_to_tool(Device::Pen, 0, phase, samples, PenButtons::default());
    }
}
