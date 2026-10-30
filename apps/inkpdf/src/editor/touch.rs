//! Touch: pans and zooms. A palm is ignored.

use crate::input::{Device, GestureDelta, Phase, PointerEvent};

use super::{Act, Editor};

/// Seconds of its lift-off speed a flicked page keeps travelling for.
const FLING_SECONDS: f32 = 0.22;
/// A touch bigger than this (in logical pixels, about 28 mm) is a palm.
pub(super) const PALM_PX: f32 = 106.0;

impl Editor {
    pub(super) fn touch_event(&mut self, ev: &PointerEvent) {
        if !self.palm.admit_touch(ev, PALM_PX * self.ws.dpr) {
            return;
        }
        let (Some(first), Some(last)) = (ev.samples.first(), ev.samples.last()) else { return };
        match ev.phase {
            Phase::Down => {
                let first_finger = self.fingers.fingers() == 0 && matches!(self.chrome.act, Act::Idle);
                if first_finger && self.press_panel(Device::Touch, ev.id, first.pos) {
                    return;
                }
                let bar = self.toolbar();
                if first_finger && bar.contains(first.pos) {
                    self.chrome.act = Act::Toolbar { dev: Device::Touch, id: ev.id, item: bar.hit(first.pos) };
                    return;
                }
                if first_finger && self.press_context(Device::Touch, ev.id, first.pos) {
                    return;
                }
                self.ws.view.stop();
                if self.fingers.fingers() == 0 {
                    self.end_pull();
                }
                self.fingers.down(ev.id, first.pos, first.t_ms);
            }
            Phase::Move => {
                // A finger on the toolbar or its slider is not panning the page.
                if self.act_belongs_to(Device::Touch, ev.id)
                    && matches!(
                        self.chrome.act,
                        Act::Toolbar { .. } | Act::Context { .. } | Act::Search { .. } | Act::Slider { .. } | Act::Swallow { .. }
                    )
                {
                    self.pointer_moved(Device::Touch, ev.id, &ev.samples);
                    return;
                }
                for s in &ev.samples {
                    if let Some(d) = self.fingers.moved(ev.id, s.pos, s.t_ms) {
                        self.apply_touch(d);
                    }
                }
            }
            Phase::Up | Phase::Cancel => {
                // (A touch the system gave up on presses, taps and flings nothing.)
                let lifted = ev.phase == Phase::Up;
                if matches!(self.chrome.act, Act::Toolbar { dev: Device::Touch, id, .. } if id == ev.id) {
                    self.release_toolbar(Device::Touch, ev.id, lifted.then_some(last.pos));
                    self.chrome.act = Act::Idle;
                    return;
                }
                if matches!(self.chrome.act, Act::Context { dev: Device::Touch, id, .. } | Act::Search { dev: Device::Touch, id, .. } if id == ev.id)
                {
                    self.release_context(Device::Touch, ev.id, lifted.then_some(last.pos));
                    self.chrome.act = Act::Idle;
                    return;
                }
                if matches!(self.chrome.act, Act::Slider { dev: Device::Touch, id, .. } | Act::Swallow { dev: Device::Touch, id } if id == ev.id) {
                    self.pointer_moved(Device::Touch, ev.id, &ev.samples);
                    self.end_act();
                    return;
                }
                let released = self.fingers.up(ev.id, last.pos, last.t_ms);
                if let Some(v) = released.fling.filter(|_| lifted) {
                    self.ws.scroll_by_px(v[0] * FLING_SECONDS, v[1] * FLING_SECONDS);
                }
                if self.fingers.fingers() == 0 {
                    self.end_pull();
                }
                if let Some(pos) = released.tap.filter(|_| lifted) {
                    // A tap follows a link under the finger; failing that, on the page it lets go of
                    // a selection.
                    if !self.follow_link_at(pos) {
                        self.ws.selection.clear();
                    }
                }
            }
            Phase::Hover | Phase::Leave => {}
        }
        self.ws.redraw = true;
    }

    /// Every finger's business is called off, as when the pen comes near: no pan, pinch, toolbar press or fling goes on, and
    /// nothing is activated.
    pub(super) fn cancel_touches(&mut self) {
        self.fingers = crate::input::TouchGestures::default();
        self.end_pull();
        if matches!(
            self.chrome.act,
            Act::Toolbar { dev: Device::Touch, .. }
                | Act::Context { dev: Device::Touch, .. }
                | Act::Search { dev: Device::Touch, .. }
                | Act::Slider { dev: Device::Touch, .. }
                | Act::Swallow { dev: Device::Touch, .. }
        ) {
            self.end_act();
        }
        self.overview_gestures_lost();
        self.ws.redraw = true;
    }

    /// Fingers moved: two or more pinch and pan, one pans (and may pull a new page in).
    fn apply_touch(&mut self, d: GestureDelta) {
        if d.fingers >= 2 {
            self.end_pull();
            if (d.zoom - 1.0).abs() > 1e-4 {
                self.ws.fit_mode = false;
                // Zoom about where the fingers were, then follow them.
                let before = [d.centre[0] - d.pan[0], d.centre[1] - d.pan[1]];
                let zoom = self.ws.view.zoom * d.zoom;
                self.ws.zoom_about(zoom, before);
            }
            self.ws.pan_px(d.pan[0], d.pan[1]);
        } else {
            self.pan_and_pull(d.pan);
        }
    }

    /// Whether a second finger means the zoom is in motion.
    pub fn pinching(&self) -> bool {
        self.fingers.fingers() >= 2
    }
}
