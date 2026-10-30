//! The bar under the tools: where it is, what a press on it does, and what its buttons do. They work on the tool in
//! use and its selection, whichever device pressed them, so that a stylus alone (no eraser end, no barrel button) or
//! a finger can select, erase, delete, duplicate, copy, paste and restyle; the keyboard and the stylus's buttons do
//! the same things as shortcuts.

use crate::input::Device;
use crate::tool::{MAX_ACTIONS, Tool, ToolAction};
use crate::toolbar::{ContextBar, ContextItem, ContextState, PanelState};
use crate::tools::Key;

use super::act::CardHit;
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
            selection: !self.ws.selection.is_empty() || (self.ws.tool == Tool::Select && self.tools.select.has_selection(&self.ws)),
            paste: self.can_paste(),
            actions,
            style_open: self.chrome.panel.is_some(),
            palette: self.ws.styles.palette,
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

    /// What is under `pos` among the cards that float over the page, topmost first: the round button under the theme button, the search card (it
    /// is over the page, under the bars), the bar under the tools. `Some(None)` is on a card but not on a button of it.
    fn card_at(&self, pos: [f32; 2]) -> Option<Option<CardHit>> {
        if self.theme_layout().is_some_and(|c| c.contains(pos)) {
            return Some(Some(CardHit::Theme));
        }
        if let Some(card) = self.search_layout().filter(|c| c.contains(pos)) {
            return Some(card.hit(pos).map(CardHit::Search));
        }
        let bar = self.context_bar().filter(|b| b.contains(pos))?;
        Some(bar.hit(pos).map(CardHit::Context))
    }

    /// A press at `pos`: if it is on a card, it is the card's (and the button is pressed when it lets go).
    pub(super) fn press_context(&mut self, dev: Device, id: u32, pos: [f32; 2]) -> bool {
        let Some(hit) = self.card_at(pos) else { return false };
        self.chrome.act = Act::Card { dev, id, hit };
        self.ws.redraw = true;
        true
    }

    /// If the pointer pressed a button of a card and is still over it, press it.
    pub(super) fn release_context(&mut self, dev: Device, id: u32, pos: Option<[f32; 2]>) {
        let Act::Card { dev: d, id: a, hit: Some(hit) } = self.chrome.act else { return };
        if d == dev && a == id && pos.is_some_and(|p| self.card_at(p).flatten() == Some(hit)) {
            self.activate_card(hit);
        }
    }

    fn activate_card(&mut self, hit: CardHit) {
        match hit {
            CardHit::Theme => self.set_dark_pages(!self.dark_pages()),
            CardHit::Search(item) => self.press_search_item(item),
            CardHit::Context(item) => self.activate_context(item),
        }
    }
    fn activate_context(&mut self, item: ContextItem) {
        match item {
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

    /// Delete the selection (through the tool, which finishes what it was doing first). `false` if there was
    /// nothing to delete.
    pub fn delete_selected(&mut self) -> bool {
        let used = self.tools.get_mut(self.ws.tool).key(&mut self.ws, Key::Delete);
        self.ws.redraw = true;
        used
    }
}
