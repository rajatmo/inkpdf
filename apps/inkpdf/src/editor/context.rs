//! The bar under the tools: where it is, what a press on it does, and what its buttons do. They work on the tool in
//! use and its selection, whichever device pressed them, so that a stylus alone (no eraser end, no barrel button) or
//! a finger can select, erase, delete, duplicate, copy, paste and restyle; the keyboard and the stylus's buttons do
//! the same things as shortcuts.

use crate::input::{Device, Intent};
use crate::tool::{MAX_ACTIONS, Tool, ToolAction};
use crate::toolbar::{ContextBar, ContextItem, ContextState, PanelState};
use crate::tools::Key;

use super::{Act, Editor};

impl Editor {
    /// The bar under the tools, if there is one: not without a document, and not with the pointer tool, which
    /// makes nothing to select (the text selection's has its marks).
    pub fn context_bar(&self) -> Option<ContextBar> {
        (self.ws.has_doc && self.ws.tool.has_bar()).then(|| ContextBar::below(&self.toolbar(), self.ws.view.viewport, self.tool_actions()))
    }

    /// The actions of its own the tool in use offers now (what it is editing decides which).
    fn tool_actions(&self) -> &'static [ToolAction] {
        self.tools.get(self.ws.tool).actions(&self.ws)
    }

    /// Everything the bar's look depends on.
    pub fn context_view(&self) -> Option<ContextState> {
        self.context_bar()?;
        let tool = self.tools.get(self.ws.tool);
        let mut actions = [None; MAX_ACTIONS];
        for (slot, a) in actions.iter_mut().zip(self.tool_actions()) {
            *slot = Some((*a, tool.action_enabled(&self.ws, *a)));
        }
        let state = ContextState {
            tool: self.ws.tool,
            intent: self.ws.intent,
            selection: !self.ws.selection.is_empty() || (self.ws.tool == Tool::Select && self.tools.select.has_selection(&self.ws)),
            paste: self.can_paste(),
            actions,
            style_open: self.chrome.panel.is_some(),
            palette: self.ws.palette,
            colour: self.style().colour,
        };
        Some(state)
    }

    /// The card the bar and its unfolded panel make together, while the panel is open: what a press on it that is
    /// not on a control of the panel does nothing to (and does not close the panel).
    pub fn drawer(&self) -> Option<[f32; 4]> {
        let state = self.chrome.panel?;
        let bar = self.context_bar()?;
        let panel = self.toolbar().tool_panel(self.ws.tool, self.ws.view.viewport[0], state.editing.is_some())?;
        // (A little below the panel's last row, for the card's rounded corners.)
        Some(bar.card(panel.rect[3] + 0.2 * panel.unit()))
    }

    /// A press at `pos`: if it is on the bar, it is the bar's (and the button is pressed when it lets go).
    pub(super) fn press_context(&mut self, dev: Device, id: u32, pos: [f32; 2]) -> bool {
        if let Some(card) = self.theme_layout().filter(|c| c.contains(pos)) {
            self.chrome.act = Act::Theme { dev, id, hit: card.hit(pos) };
            self.ws.redraw = true;
            return true;
        }
        // (The search card floats over the page, under the bars: it takes the press first.)
        if let Some(card) = self.search_layout().filter(|c| c.contains(pos)) {
            self.chrome.act = Act::Search { dev, id, item: card.hit(pos) };
            self.ws.redraw = true;
            return true;
        }
        let Some(bar) = self.context_bar().filter(|b| b.contains(pos)) else { return false };
        self.chrome.act = Act::Context { dev, id, item: bar.hit(pos) };
        self.ws.redraw = true;
        true
    }

    /// If the pointer pressed a button of the bar and is still over it, press it.
    pub(super) fn release_context(&mut self, dev: Device, id: u32, pos: Option<[f32; 2]>) {
        if let Act::Theme { dev: d, id: a, hit: true } = self.chrome.act {
            if d == dev && a == id && pos.is_some_and(|p| self.theme_layout().is_some_and(|c| c.hit(p))) {
                self.press_theme_row();
            }
            return;
        }
        if let Act::Search { dev: d, id: a, item: Some(item) } = self.chrome.act {
            if d == dev && a == id && pos.is_some_and(|p| self.search_layout().and_then(|c| c.hit(p)) == Some(item)) {
                self.press_search_item(item);
            }
            return;
        }
        let Act::Context { dev: d, id: a, item: Some(item) } = self.chrome.act else { return };
        if d != dev || a != id {
            return;
        }
        if pos.is_some_and(|p| self.context_bar().and_then(|b| b.hit(p)) == Some(item)) {
            self.activate_context(item);
        }
    }

    fn activate_context(&mut self, item: ContextItem) {
        match item {
            ContextItem::Intent(intent) => self.choose_intent(intent),
            ContextItem::Style => {
                self.settle();
                self.chrome.panel = if self.chrome.panel.is_some() { None } else { Some(PanelState::default()) };
            }
            ContextItem::Swatch(slot) => {
                self.settle();
                // Another colour is taken; the one in use opens its editor, with the panel unfolded to hold it.
                if slot == self.style().colour && self.chrome.panel.is_none() {
                    self.chrome.panel = Some(PanelState::default());
                }
                self.press_swatch(slot);
            }
            ContextItem::Duplicate => {
                self.settle();
                self.ws.duplicate_selection();
            }
            ContextItem::Copy => {
                self.settle();
                if self.ws.tool == Tool::Select {
                    self.tools.select.copy_text(&mut self.ws);
                } else {
                    self.ws.copy_selection();
                }
            }
            ContextItem::Paste => {
                self.settle();
                self.ws.paste();
            }
            ContextItem::Delete => {
                self.settle();
                self.delete_selected();
            }
            ContextItem::Action(action) => {
                self.settle();
                self.tools.get_mut(self.ws.tool).act(&mut self.ws, action);
            }
        }
        self.ws.redraw = true;
    }

    /// Choose what the plain tip of the stylus (or the left mouse button) does: draw, select or erase.
    pub fn choose_intent(&mut self, intent: Intent) {
        self.settle();
        self.tools.end_all(&mut self.ws);
        self.ws.intent = intent;
        self.ws.redraw = true;
    }

    /// Delete the selection (through the tool, which finishes what it was doing first). `false` if there was
    /// nothing to delete.
    pub fn delete_selected(&mut self) -> bool {
        let used = self.tools.get_mut(self.ws.tool).key(&mut self.ws, Key::Delete);
        self.ws.redraw = true;
        used
    }
}
