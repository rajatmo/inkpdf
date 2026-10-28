//! Moving through the document by touch beyond plain panning: tapping a link, and pulling up past
//! the last page to add a blank one.

use crate::links::{self, Target};

use super::{Editor, Request};

/// How far a finger must pull up past the end of the document, in logical pixels, to add a page.
const PULL_PX: f32 = 140.0;

impl Editor {
    /// Follow the link at a tap, if there is one. Only touch calls this: a mouse click or a pen
    /// touch never follows a link.
    pub(super) fn follow_link_at(&mut self, pos: [f32; 2]) -> bool {
        let Some(link) = links::hit(&self.links, self.to_doc(pos)) else { return false };
        match link.target.clone() {
            Target::Page(page) => self.ws.scroll_to_page(page),
            Target::Url(url) => self.ws.requests.push(Request::OpenUrl(url)),
        }
        true
    }

    /// How far the pull has come towards a new page, 0 to 1.
    pub fn pull_progress(&self) -> f32 {
        (self.pull / (PULL_PX * self.ws.dpr)).clamp(0.0, 1.0)
    }

    pub(super) fn end_pull(&mut self) {
        self.pull = 0.0;
        self.pull_fired = false;
    }

    /// One finger panning. Pulling up when the view already shows the end of the document builds
    /// towards a new blank page; pulling back down gives it up again.
    pub(super) fn pan_and_pull(&mut self, pan: [f32; 2]) {
        let was_at_end = self.ws.view.at_end(&self.ws.layout);
        self.ws.pan_px(pan[0], pan[1]);
        if was_at_end && pan[1] < 0.0 {
            self.pull += -pan[1];
        } else if pan[1] > 0.0 {
            self.pull = (self.pull - pan[1]).max(0.0);
        }
        if !self.ws.view.at_end(&self.ws.layout) {
            self.pull = 0.0;
        }
        if self.pull >= PULL_PX * self.ws.dpr && !self.pull_fired {
            self.pull_fired = true;
            self.pull = 0.0;
            self.ws.requests.push(Request::AddPage);
        }
    }
}
