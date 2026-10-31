//! The card of an item an extension made (see `inkpdf_api::ItemKind::card`): opened by the tool that pressed the item, it shows what the kind says, takes
//! what is written in its field and the name written on it, and has the kind's buttons and the program's own delete. It floats at the right under
//! the bars, is not modal (the page stays in view and the pen still works on it) and has the keyboard while it is open. Everything on it is a
//! button for a stylus tip or the left mouse button, and the keyboard does the same things.

use crate::field::{Edited, Field, Rules};
use crate::toolbar::{CARD_ACTIONS, CARD_ROWS, CardItem, ItemCardLayout, ItemCardView};
use crate::tools::Key;
use crate::tools::items;

use super::{Editor, Request};

/// What a reply takes: a line of at most so many characters.
const REPLY: Rules = Rules { max_chars: 2000, multiline: false };
/// What the name takes.
const NAME: Rules = Rules { max_chars: 60, multiline: false };

/// What the open card is about: the item, its kind, what the kind says it shows, and what the item holds.
struct Open {
    id: u64,
    kind: &'static dyn inkpdf_api::ItemKind,
    card: inkpdf_api::Card,
    data: String,
}

impl Editor {
    /// The open card's item, if a card is open and its item is still there (an undo or an eraser can take it away).
    fn open_card(&self) -> Option<Open> {
        let id = self.ws.card.open?;
        let item = self.ws.ink.get(id)?.item.as_ref()?;
        let kind = items::kind(&item.kind)?;
        let card = kind.card(&item.data)?;
        Some(Open { id, kind, card, data: item.data.clone() })
    }

    /// Whether the card of an item is open.
    pub fn item_card_open(&self) -> bool {
        self.open_card().is_some()
    }

    /// Put the card away.
    pub fn close_item_card(&mut self) {
        if self.ws.card.open.take().is_some() {
            self.ws.card.field = Field::default();
            self.ws.card.typing_name = false;
            self.ws.redraw = true;
        }
    }

    /// The card, where it is (at the right, under the bars and under the search card if that is open), if it is open.
    pub fn item_card_layout(&self) -> Option<ItemCardLayout> {
        let open = self.open_card()?;
        if self.chrome.prompt.is_some() {
            return None;
        }
        let bar = self.toolbar();
        let mut top = self.context_bar().map_or(bar.bottom(), |c| c.rect[3]) + 8.0 * self.ws.dpr;
        if let Some(button) = self.theme_layout() {
            top = top.max(button.rect[3] + 8.0 * self.ws.dpr);
        }
        if let Some(search) = self.search_layout() {
            top = top.max(search.card[3] + 8.0 * self.ws.dpr);
        }
        Some(ItemCardLayout::new(&bar, self.ws.view.viewport, top, open.card.rows.len(), open.card.actions.len(), open.card.field.is_some()))
    }

    /// What the card is drawn from.
    pub fn item_card_view(&self) -> Option<ItemCardView> {
        let open = self.open_card()?;
        let state = &self.ws.card;
        let shown = open.card.rows.len().min(CARD_ROWS);
        let rows = open.card.rows[open.card.rows.len() - shown..].iter().map(|r| (r.head.clone(), r.body.clone())).collect();
        Some(ItemCardView {
            title: open.card.title.clone(),
            rows,
            older: open.card.rows.len() - shown,
            name: state.name.text.clone(),
            name_caret: state.name.byte(state.name.caret),
            hint: open.card.field.clone(),
            text: state.field.text.clone(),
            caret: state.field.byte(state.field.caret),
            actions: open.card.actions.iter().take(CARD_ACTIONS).map(|a| a.label.clone()).collect(),
            typing_name: state.typing_name,
        })
    }

    /// Do `action` of the kind (or, with none, send `input` from the field) to the item that is open: the change is one undo step. `true` if it
    /// changed anything.
    fn item_card_act(&mut self, action: &str, input: &str) -> bool {
        let Some(open) = self.open_card() else { return false };
        let date = items::now_date();
        let who = inkpdf_api::Who { author: &self.ws.card.name.text, date: &date };
        let Some(data) = open.kind.act(&open.data, action, input, &who) else { return false };
        let changed = self.ws.ink.edit_item(open.id, data);
        self.ws.redraw |= changed;
        changed
    }

    /// Send what is in the field (nothing, if it holds only spaces).
    fn send_from_card(&mut self) {
        let text = self.ws.card.field.text.trim().to_string();
        if !text.is_empty() && self.item_card_act("", &text) {
            self.ws.card.field = Field::default();
        }
    }

    /// Take the item away, as one undo step, and put the card away.
    pub fn delete_card_item(&mut self) {
        let Some(id) = self.ws.card.open else { return };
        self.ws.ink.remove(&[id]);
        self.ws.prune_selection();
        self.close_item_card();
    }

    /// A part of the card pressed (and let go on): what it does.
    pub(super) fn press_card_item(&mut self, item: CardItem) {
        let Some(open) = self.open_card() else { return };
        match item {
            CardItem::Name => {
                self.ws.card.typing_name = true;
                self.ws.card.name.move_to(usize::MAX, false);
            }
            CardItem::Field => {
                self.ws.card.typing_name = false;
                self.ws.card.field.move_to(usize::MAX, false);
            }
            CardItem::Send => self.send_from_card(),
            CardItem::Close => self.close_item_card(),
            CardItem::Delete => self.delete_card_item(),
            CardItem::Action(i) => {
                if let Some(action) = open.card.actions.get(i) {
                    self.item_card_act(action.id, "");
                }
            }
        }
        self.ws.redraw = true;
    }

    /// The keyboard while the card is open: typing in the field (or the name, once it has been pressed), Enter to send (or to leave the name), Esc
    /// to put the card away, and Delete in an empty field to take the item away. `true` if the key was used.
    pub(super) fn item_card_key(&mut self, key: Key<'_>) -> bool {
        let name = self.ws.card.typing_name;
        let used = match key {
            Key::Escape => {
                self.close_item_card();
                true
            }
            Key::Text(typed) if typed.contains(['\n', '\r']) => {
                // (An Enter sends; what came with it is typed first, without its line breaks.)
                if name {
                    self.ws.card.name.type_text(typed, NAME);
                    self.ws.card.typing_name = false;
                    self.ws.requests.push(Request::SettingsChanged);
                } else {
                    self.ws.card.field.type_text(typed, REPLY);
                    self.send_from_card();
                }
                true
            }
            Key::Delete if !name && self.ws.card.field.text.is_empty() => {
                self.delete_card_item();
                true
            }
            // (A page key goes on to scroll the document, as the card has no use for it.)
            Key::Page(_) => false,
            _ => {
                let card = &mut self.ws.card;
                let (field, rules) = if name { (&mut card.name, NAME) } else { (&mut card.field, REPLY) };
                let edited = field.key(key, &mut *self.ws.clip.text, rules);
                if name && edited == Edited::Changed {
                    self.ws.requests.push(Request::SettingsChanged);
                }
                edited != Edited::Unused
            }
        };
        self.ws.redraw = true;
        used
    }
}
