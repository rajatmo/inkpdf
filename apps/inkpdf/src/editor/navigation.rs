//! Moving through the document by touch beyond plain panning: tapping a link, and pulling up past
//! the last page to add a blank one.

use crate::input::{Device, FINGER, POINTER, Phase, Sample};
use crate::links::{self, Target};
use crate::tool::Tool;

use super::{Editor, Request};

/// How far a finger must pull up past the end of the document, in logical pixels, to add a page.
const PULL_PX: f32 = 140.0;
/// The most the document can be lifted by a pull (it is never quite that far: see `lift_for`), in logical pixels.
const LIFT_LIMIT_PX: f32 = 110.0;

/// A press of the stylus, or the mouse as one, on a link, while it might still be a click.
#[derive(Clone, Copy, Debug)]
pub(super) struct LinkTap {
    dev: Device,
    id: u32,
    from: [f32; 2],
    t_ms: f64,
}

impl Editor {
    /// With the Select tool (the one to read with, and the one the program starts on) a click that stays where it was on a link follows it,
    /// with the stylus's tip and with the left mouse button: the same as a finger's tap, which follows a link with every tool. (With the tools
    /// that draw, a press is a mark, so only a finger follows.) A drag from a link is a selection, not a click.
    pub(super) fn watch_link_click(&mut self, dev: Device, id: u32, phase: Phase, samples: &[Sample]) {
        if self.ws.tool != Tool::Select {
            self.link_tap = None;
            return;
        }
        let (Some(first), Some(last)) = (samples.first(), samples.last()) else { return };
        match phase {
            Phase::Down => {
                self.leave_field(first.pos);
                let on_link = links::hit(&self.links, self.to_doc(first.pos)).is_some();
                self.link_tap = on_link.then_some(LinkTap { dev, id, from: first.pos, t_ms: first.t_ms });
            }
            Phase::Move => {
                if let Some(tap) = self.link_tap.filter(|t| t.dev == dev && t.id == id)
                    && (last.pos[0] - tap.from[0]).hypot(last.pos[1] - tap.from[1]) > POINTER.slop_px * self.ws.dpr
                {
                    self.link_tap = None;
                }
            }
            Phase::Up => {
                if let Some(tap) = self.link_tap.take().filter(|t| t.dev == dev && t.id == id)
                    && POINTER.is_click((last.pos[0] - tap.from[0]).hypot(last.pos[1] - tap.from[1]) / self.ws.dpr, last.t_ms - tap.t_ms)
                {
                    self.follow_link_at(last.pos);
                }
            }
            Phase::Cancel | Phase::Leave => self.link_tap = None,
            Phase::Hover => {}
        }
    }

    /// Follow the link at a tap, if there is one. Only touch calls this: a mouse click or a pen
    /// touch never follows a link.
    pub(super) fn follow_link_at(&mut self, pos: [f32; 2]) -> bool {
        self.leave_field(pos);
        let Some(link) = links::hit(&self.links, self.to_doc(pos)) else { return false };
        match link.target.clone() {
            Target::Page(page) => self.ws.scroll_to_page(page),
            Target::Url(url) => self.ws.requests.push(Request::OpenUrl(url)),
            Target::Field { .. } => return self.fill_field_at(pos),
        }
        true
    }

    /// A finger tapped empty page. One tap does nothing more (it let go of the selection, at once: nothing waits to see whether another comes);
    /// a second soon and near the first is a double tap, which fits the page to the width, and a second double tap goes back to where it was.
    pub(super) fn tap_on_page(&mut self, pos: [f32; 2], t_ms: f64) {
        let dpr = self.ws.dpr;
        let double = self.last_tap.is_some_and(|(t, p)| FINGER.is_double(t_ms - t, (pos[0] - p[0]).hypot(pos[1] - p[1]) / dpr));
        if !double {
            self.last_tap = Some((t_ms, pos));
            return;
        }
        self.last_tap = None;
        // Back, if it was this double tap that fitted the page and nothing has changed the zoom since (a pinch forgets where it was).
        match self.fit_before.take().filter(|_| self.ws.fit_mode) {
            Some((zoom, scroll)) => {
                self.ws.fit_mode = false;
                self.ws.view.zoom = zoom;
                self.ws.view.scroll = scroll;
                self.ws.view.stop();
                self.ws.view.clamp(&self.ws.layout);
            }
            None => {
                self.fit_before = Some((self.ws.view.zoom, self.ws.view.scroll));
                self.ws.fit_mode = true;
                self.ws.fit_width();
            }
        }
        self.ws.redraw = true;
    }

    /// How far the pull has come towards a new page, 0 to 1.
    pub fn pull_progress(&self) -> f32 {
        (self.pull / (PULL_PX * self.ws.dpr)).clamp(0.0, 1.0)
    }

    /// The pull is over (the finger lifted, a second one came, something else took the window): what it had lifted comes back down.
    pub(super) fn end_pull(&mut self) {
        self.pull = 0.0;
        self.pull_fired = false;
        self.ws.view.let_go();
    }

    /// How far the document is lifted for a pull of `pull` pixels: the finger is followed at first, then less and less (as a stretched
    /// band is), so the pull has no end to hit and the page it adds is felt coming.
    fn lift_for(&self, pull: f32) -> f32 {
        let limit = LIFT_LIMIT_PX * self.ws.dpr;
        limit * (1.0 - (-pull / limit).exp())
    }

    /// One finger panning. Pulling up when the view already shows the end of the document lifts it off its end, with a plus (see
    /// `pull_overlay`) in the room that is left, and builds towards a new blank page; pulling back down brings it back. When the pull
    /// is long enough the page is added and the finger has done its part: the view glides on to the new page by itself.
    pub(super) fn pan_and_pull(&mut self, pan: [f32; 2]) {
        if self.pull_fired {
            return;
        }
        // A pull under way takes the finger coming back first: the document comes down before it scrolls.
        let mut dy = pan[1];
        if self.pull > 0.0 && dy > 0.0 {
            let back = dy.min(self.pull);
            self.pull -= back;
            dy -= back;
        }
        let was_at_end = self.ws.view.at_end(&self.ws.layout);
        self.ws.pan_px(pan[0], dy);
        if was_at_end && dy < 0.0 {
            self.pull += -dy;
        }
        if !self.ws.view.at_end(&self.ws.layout) {
            self.pull = 0.0;
        }
        let threshold = PULL_PX * self.ws.dpr;
        if self.pull >= threshold {
            self.pull_fired = true;
            let lift = self.lift_for(self.pull);
            self.pull = 0.0;
            // The page comes in over the room the pull made: the document is let go (it comes down as the view glides on to the page).
            self.ws.view.pull(lift, 1.0);
            self.ws.view.let_go();
            if self.add_page() {
                self.ws.view.glide_slowly();
            }
        } else {
            let lift = self.lift_for(self.pull);
            if lift > 0.0 {
                self.ws.view.pull(lift, self.pull_progress());
            } else {
                self.ws.view.let_go();
            }
        }
    }
}
