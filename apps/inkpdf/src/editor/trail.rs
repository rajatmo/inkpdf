//! Browsing through the document like the web. A jump that takes the reader away from where they were (a link followed, a contents entry or page
//! pressed in the overview, a search result pressed) is remembered with the place they left; the first one opens a small pill under the document's pill
//! with a button to go back and one to go forward again (Alt and an arrow, or a mouse's side buttons, do the same). Scrolling by hand is not a jump, and a
//! new jump after going back forgets what was ahead, as in a browser. Editing the pages forgets all of it: a place is a page and how far down it, and the
//! pages have moved.

use crate::toolbar::{TrailItem, TrailLayout, TrailView};

use super::Editor;
use super::act::{Act, CardHit};

/// The most places kept (the oldest go first).
const MAX_PLACES: usize = 100;

/// A place in the document: a page, and how far down it the top of the window is (it is above the page by a margin when a page has just been scrolled to).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    page: usize,
    dy: f32,
}

/// The places the reader has been through, and which of them they are at.
#[derive(Default)]
pub struct Trail {
    places: Vec<Place>,
    at: usize,
}

impl Trail {
    /// Whether there is somewhere to go (the pill is there from the first jump on).
    fn visible(&self) -> bool {
        self.places.len() >= 2
    }

    fn can_back(&self) -> bool {
        self.visible() && self.at > 0
    }

    fn can_forward(&self) -> bool {
        self.at + 1 < self.places.len()
    }

    pub fn clear(&mut self) {
        self.places.clear();
        self.at = 0;
    }

    /// The reader left `from` for `to`: what was ahead is forgotten, and the place left is kept as it was left.
    fn jumped(&mut self, from: Place, to: Place) {
        if self.places.is_empty() {
            self.places.push(from);
            self.at = 0;
        } else {
            self.places.truncate(self.at + 1);
            self.places[self.at] = from;
        }
        self.places.push(to);
        if self.places.len() > MAX_PLACES {
            self.places.remove(0);
        }
        self.at = self.places.len() - 1;
    }
}

impl Editor {
    /// Where the top of the window is heading, as a place.
    fn here(&self) -> Place {
        let y = self.ws.view.scroll_target_y();
        let page = self.ws.layout.page_at(y);
        Place { page, dy: y - self.ws.layout.page(page).map_or(0.0, |r| r.y) }
    }

    /// The document y of a place (a page that is no longer there is the last).
    fn y_of(&self, place: Place) -> f32 {
        let last = self.ws.layout.len().saturating_sub(1);
        self.ws.layout.page(place.page.min(last)).map_or(0.0, |r| r.y) + place.dy
    }

    /// Do `go` (something that scrolls the view somewhere), and remember it as a jump if it took the reader somewhere else.
    pub(super) fn jumping(&mut self, go: impl FnOnce(&mut Editor)) {
        let from = self.here();
        go(self);
        let to = self.here();
        if (self.y_of(from) - self.y_of(to)).abs() > 1.0 {
            self.trail.jumped(from, to);
            self.ws.redraw = true;
        }
    }

    /// Take the reader to the top of `page`, with the page's first line below the bars (the pills float over the page: on a phone they are tall, and a page
    /// scrolled to the very top would have its heading under them), and remember it as a jump.
    pub(super) fn jump_to_page(&mut self, page: usize) {
        self.jumping(|ed| {
            let Some(rect) = ed.ws.layout.page(page) else { return };
            let bars = ed.context_bar().map_or(ed.toolbar().bottom(), |c| c.rect[3]) + 8.0 * ed.ws.dpr;
            let y = rect.y - bars / ed.ws.view.zoom;
            ed.ws.view.glide_to_y(&ed.ws.layout, y);
        });
    }

    /// Back (`true`) or forward through the places, if there is one that way. `true` if it went.
    pub fn go_trail(&mut self, back: bool) -> bool {
        let t = &self.trail;
        if !(if back { t.can_back() } else { t.can_forward() }) {
            return false;
        }
        // (Where the reader is now is the place kept for this stop, so that coming back finds it as it was left.)
        let here = self.here();
        let at = if back { self.trail.at - 1 } else { self.trail.at + 1 };
        let at_now = self.trail.at;
        self.trail.places[at_now] = here;
        self.trail.at = at;
        let y = self.y_of(self.trail.places[at]);
        self.ws.view.glide_to_y(&self.ws.layout, y);
        self.ws.redraw = true;
        true
    }

    /// The pill, where it is, if there are places to go between and nothing takes the window.
    pub fn trail_layout(&self) -> Option<TrailLayout> {
        (self.ws.has_doc && self.trail.visible() && self.chrome.prompt.is_none() && !self.overview_open())
            .then(|| TrailLayout::new(&self.toolbar(), self.context_bar().map(|b| b.rect)))
    }

    /// What the pill is drawn from.
    pub fn trail_view(&self) -> Option<TrailView> {
        self.trail_layout()?;
        let pressed = match self.chrome.act {
            Act::Card { hit: Some(CardHit::Trail(item)), .. } => Some(item),
            _ => None,
        };
        Some(TrailView { can_back: self.trail.can_back(), can_forward: self.trail.can_forward(), pressed })
    }

    /// A button of the pill pressed (and let go on).
    pub(super) fn press_trail(&mut self, item: TrailItem) {
        self.go_trail(item == TrailItem::Back);
    }

    /// Forget the places (the pages were edited, or a document was opened).
    pub(super) fn forget_trail(&mut self) {
        if !self.trail.places.is_empty() {
            self.trail.clear();
            self.ws.redraw = true;
        }
    }
}
