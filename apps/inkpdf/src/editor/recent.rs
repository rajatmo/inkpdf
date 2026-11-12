//! The PDFs opened last (see [`crate::recent`]): the card that lists them, which the clock button of the document's pill shows and puts away, and
//! where the document in view is, to be shown so again the next time it is opened. The shell keeps the list and its file; the editor is told the rows
//! to show ([`Editor::set_recent`]) and asks for one to be opened ([`Request::OpenRecent`]). A press anywhere else, Esc, or the button again puts the
//! card away; every device does all of it with a press.

use crate::recent::Place;
use crate::toolbar::{Item, RecentLayout, RecentRow, RecentView};

use super::act::{Act, CardHit};
use super::{Editor, Request};

impl Editor {
    /// The documents the card lists, newest first (the shell says, whenever its list changes).
    pub fn set_recent(&mut self, rows: Vec<RecentRow>) {
        if rows != self.recent {
            self.recent = rows;
            if self.recent.is_empty() {
                self.chrome.recent = false;
            }
            self.ws.redraw = true;
        }
    }

    #[cfg(test)]
    pub fn recent_open(&self) -> bool {
        self.chrome.recent
    }

    /// A press of the clock button: the card is shown, or put away if it was. (One card at a time: the cards at the right are put away, as the
    /// window may be no wider than one.)
    pub(super) fn toggle_recent(&mut self) {
        if self.chrome.recent {
            self.close_recent();
        } else if !self.recent.is_empty() {
            self.close_search();
            self.close_item_card();
            self.close_link_card();
            self.chrome.recent = true;
            self.ws.redraw = true;
        }
    }

    pub fn close_recent(&mut self) {
        if self.chrome.recent {
            self.chrome.recent = false;
            self.ws.redraw = true;
        }
    }

    /// The card, where it is, if it is shown (not while a prompt, the overview or the slides have the window).
    pub fn recent_layout(&self) -> Option<RecentLayout> {
        if !self.chrome.recent || self.recent.is_empty() || self.modal().is_some() {
            return None;
        }
        let avoid = [self.trail_layout().map(|t| t.card), self.context_bar().map(|b| b.rect)].map(Option::unwrap_or_default);
        Some(RecentLayout::new(&self.toolbar(), self.ws.view.viewport, &avoid, self.recent.len()))
    }

    /// What the card is drawn from.
    pub fn recent_view(&self) -> Option<RecentView> {
        self.recent_layout()?;
        let pressed = match self.chrome.act {
            Act::Card { hit: Some(CardHit::Recent(row)), .. } => Some(row),
            _ => None,
        };
        Some(RecentView { rows: self.recent.clone(), pressed })
    }

    /// A press is beginning at `pos`: the card goes unless the press is on it or on the button that shows it (which puts it away itself).
    pub(super) fn dismiss_recent(&mut self, pos: [f32; 2]) {
        if !self.chrome.recent {
            return;
        }
        let on_card = self.recent_layout().is_some_and(|c| c.contains(pos));
        if !on_card && self.toolbar().hit(pos) != Some(Item::Recent) {
            self.close_recent();
        }
    }

    /// A row of the card pressed (and let go on): the shell is asked to open that document.
    pub(super) fn press_recent_row(&mut self, row: usize) {
        self.close_recent();
        if row < self.recent.len() {
            self.ws.requests.push(Request::OpenRecent(row));
        }
        self.ws.redraw = true;
    }

    /// Where the document is being looked at: the page at the top of the window and how far down it, the zoom, and whether the zoom follows the
    /// window's width. (While the slides show, the page that is shown, as it will be when they are left.)
    pub fn place(&self) -> Place {
        let view = &self.ws.view;
        if let Some((page, fit)) = self.slide_place() {
            return Place { page, dy: -crate::view::DOC_MARGIN, x: 0.0, zoom: view.zoom, fit };
        }
        let y = view.scroll_target_y();
        let page = self.ws.layout.page_at(y);
        Place { page, dy: y - self.ws.layout.page(page).map_or(0.0, |r| r.y), x: view.scroll[0], zoom: view.zoom, fit: self.ws.fit_mode }
    }

    /// Look at the document as it was left at `place`, at once. A page that is no longer there is the last one, at its top; the view is kept
    /// within the document, and a zoom that followed the window's width follows the width the window has now.
    pub fn show_place(&mut self, place: Place) {
        let last = self.ws.layout.len().saturating_sub(1);
        let Some(rect) = self.ws.layout.page(place.page.min(last)) else { return };
        if self.presenting() || !place.is_sound() {
            return;
        }
        let dy = if place.page > last { -crate::view::DOC_MARGIN } else { place.dy.min(rect.h) };
        let (zoom, x) = if place.fit { (self.ws.view.fit_width_zoom(&self.ws.layout), 0.0) } else { (place.zoom, place.x) };
        self.ws.fit_mode = place.fit;
        self.ws.view.jump_to(&self.ws.layout, zoom, [x, rect.y + dy]);
        self.fit_before = None;
        self.ws.redraw = true;
    }
}
