//! The card an extension shows for a link that leaves the document (see `inkpdf_api::LinkHandler`): where the address goes, and buttons to open it in the
//! system's browser or copy it. With no extension that takes the link, the address goes straight to the system, as it always did. The card is the item
//! card's, drawn without a name or a delete button: at the right under the bars, not modal, every part a button for a stylus tip, a finger or the left
//! mouse button, and Esc puts it away.

use inkpdf_api::LinkAct;

use crate::toolbar::{CARD_ACTIONS, CARD_ROWS, CardItem, ItemCardLayout, ItemCardView};
use crate::tools::Key;

use super::{Editor, Request};

/// The card that is up: the address, which handler made it, and what it says.
pub struct LinkOpen {
    url: String,
    handler: usize,
    card: inkpdf_api::Card,
}

impl Editor {
    /// Follow an address that goes out to the web: an extension that has a card for it shows the card; if none does, the system opens it.
    pub(super) fn follow_web_link(&mut self, url: String) {
        let found = self.link_handlers.iter().enumerate().find_map(|(handler, h)| h.card(&url).map(|card| (handler, card)));
        match found {
            Some((handler, card)) => {
                self.close_item_card();
                self.close_search();
                self.link = Some(LinkOpen { url, handler, card });
                self.ws.redraw = true;
            }
            None => self.ws.requests.push(Request::OpenUrl(url)),
        }
    }

    #[cfg(test)]
    pub fn link_card_open(&self) -> bool {
        self.link.is_some()
    }

    pub fn close_link_card(&mut self) {
        if self.link.take().is_some() {
            self.ws.redraw = true;
        }
    }

    /// The card where it is (under the bars, and under the card of an item or the search card if one is up), and what it shows.
    pub(super) fn link_parts(&self) -> Option<(ItemCardLayout, ItemCardView)> {
        let open = self.link.as_ref()?;
        if self.chrome.prompt.is_some() {
            return None;
        }
        let mut top = self.card_top();
        for below in [self.search_layout().map(|c| c.card), self.item_card_layout().map(|c| c.card)].into_iter().flatten() {
            top = top.max(below[3] + 8.0 * self.ws.dpr);
        }
        let shown = open.card.rows.len().min(CARD_ROWS);
        let layout = ItemCardLayout::plain(&self.toolbar(), self.ws.view.viewport, top, shown, open.card.actions.len());
        let rows = open.card.rows[open.card.rows.len() - shown..].iter().map(|r| (r.head.clone(), r.body.clone())).collect();
        let view = ItemCardView {
            title: open.card.title.clone(),
            rows,
            older: open.card.rows.len() - shown,
            name: String::new(),
            name_caret: 0,
            hint: None,
            text: String::new(),
            caret: 0,
            actions: open.card.actions.iter().take(CARD_ACTIONS).map(|a| a.label.clone()).collect(),
            typing_name: false,
        };
        Some((layout, view))
    }

    pub fn link_layout(&self) -> Option<ItemCardLayout> {
        self.link_parts().map(|(layout, _)| layout)
    }

    pub fn link_view(&self) -> Option<ItemCardView> {
        self.link_parts().map(|(_, view)| view)
    }

    /// A part of the card pressed (and let go on).
    pub(super) fn press_link_item(&mut self, item: CardItem) {
        match item {
            CardItem::Close => self.close_link_card(),
            CardItem::Action(i) => {
                let Some(open) = self.link.as_ref() else { return };
                let Some(action) = open.card.actions.get(i) else { return };
                let (url, id, handler) = (open.url.clone(), action.id, open.handler);
                let asked = self.link_handlers.get(handler).map_or(LinkAct::Nothing, |h| h.act(&url, id));
                match asked {
                    LinkAct::Nothing => {}
                    LinkAct::Close => self.close_link_card(),
                    LinkAct::Open => {
                        self.close_link_card();
                        self.ws.requests.push(Request::OpenUrl(url));
                    }
                    LinkAct::Copy => {
                        self.ws.clip.text.set(&url);
                        self.close_link_card();
                    }
                }
            }
            // (A link card has no field, name or delete button: the layout does not offer them.)
            CardItem::Name | CardItem::Field | CardItem::Send | CardItem::Delete => {}
        }
        self.ws.redraw = true;
    }

    /// The keyboard while the card is up: Esc puts it away. (The rest is for the page: a link card does not hold the keyboard.)
    pub(super) fn link_key(&mut self, key: Key<'_>) -> bool {
        if key == Key::Escape {
            self.close_link_card();
            return true;
        }
        false
    }
}
