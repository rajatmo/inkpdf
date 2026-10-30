//! Moving through the document by touch beyond plain panning: tapping a link, and pulling up past
//! the last page to add a blank one.

use crate::input::{Device, Phase, Sample};
use crate::links::{self, Target};
use crate::tool::Tool;

use super::{Editor, Request};

/// How far a finger must pull up past the end of the document, in logical pixels, to add a page.
const PULL_PX: f32 = 140.0;

/// A press of the stylus, or the mouse as one, on a link, while it might still be a click.
#[derive(Clone, Copy, Debug)]
pub(super) struct LinkTap {
    dev: Device,
    id: u32,
    from: [f32; 2],
    t_ms: f64,
}

/// How long a click may take and still be a click, and how far the pointer may move in it (logical pixels).
const CLICK_MS: f64 = 800.0;
const CLICK_SLOP_PX: f32 = 6.0;

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
                let on_link = links::hit(&self.links, self.to_doc(first.pos)).is_some();
                self.link_tap = on_link.then_some(LinkTap { dev, id, from: first.pos, t_ms: first.t_ms });
            }
            Phase::Move => {
                if let Some(tap) = self.link_tap.filter(|t| t.dev == dev && t.id == id)
                    && (last.pos[0] - tap.from[0]).hypot(last.pos[1] - tap.from[1]) > CLICK_SLOP_PX * self.ws.dpr
                {
                    self.link_tap = None;
                }
            }
            Phase::Up => {
                if let Some(tap) = self.link_tap.take().filter(|t| t.dev == dev && t.id == id)
                    && last.t_ms - tap.t_ms <= CLICK_MS
                    && (last.pos[0] - tap.from[0]).hypot(last.pos[1] - tap.from[1]) <= CLICK_SLOP_PX * self.ws.dpr
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
            self.add_page();
        }
    }
}
