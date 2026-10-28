//! Touch: pans and zooms. A palm is ignored.

use crate::input::{Device, GestureDelta, Phase, PointerEvent};

use super::{Act, Editor};

/// Seconds of its lift-off speed a flicked page keeps travelling for.
const FLING_SECONDS: f32 = 0.22;
/// A touch bigger than this (in logical pixels, about 28 mm) is a palm.
const PALM_PX: f32 = 106.0;

impl Editor {
    pub(super) fn touch_event(&mut self, ev: &PointerEvent) {
        let now = ev.samples.last().map_or_else(self.clock, |s| s.t_ms);
        if !self.palm.admit_touch(ev, now, PALM_PX * self.ws.dpr) {
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
                    && matches!(self.chrome.act, Act::Toolbar { .. } | Act::Context { .. } | Act::Slider { .. } | Act::Swallow { .. })
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
                if matches!(self.chrome.act, Act::Toolbar { dev: Device::Touch, id, .. } if id == ev.id) {
                    self.release_toolbar(Device::Touch, ev.id, Some(last.pos));
                    self.chrome.act = Act::Idle;
                    return;
                }
                if matches!(self.chrome.act, Act::Context { dev: Device::Touch, id, .. } if id == ev.id) {
                    self.release_context(Device::Touch, ev.id, Some(last.pos));
                    self.chrome.act = Act::Idle;
                    return;
                }
                if matches!(self.chrome.act, Act::Slider { dev: Device::Touch, id, .. } | Act::Swallow { dev: Device::Touch, id } if id == ev.id) {
                    self.pointer_moved(Device::Touch, ev.id, &ev.samples);
                    self.end_act();
                    return;
                }
                let released = self.fingers.up(ev.id, last.pos, last.t_ms);
                if let Some(v) = released.fling {
                    self.ws.scroll_by_px(v[0] * FLING_SECONDS, v[1] * FLING_SECONDS);
                }
                if self.fingers.fingers() == 0 {
                    self.end_pull();
                }
                if let Some(pos) = released.tap {
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

    /// Touch through winit's own events, used only if the Windows pointer layer could not be
    /// installed. Pans and zooms; it cannot tell a pen from a finger.
    pub fn basic_touch(&mut self, phase: winit::event::TouchPhase, id: u64, pos: [f32; 2]) {
        use winit::event::TouchPhase;
        if self.chrome.prompt.is_some() {
            return;
        }
        match phase {
            TouchPhase::Started => {
                self.ws.view.stop();
                self.basic_touches.insert(id, pos);
                self.basic_pinch = self.basic_pinch_state();
            }
            TouchPhase::Moved => {
                let Some(old) = self.basic_touches.insert(id, pos) else { return };
                if self.basic_touches.len() == 1 {
                    self.ws.pan_px(pos[0] - old[0], pos[1] - old[1]);
                } else if let (Some(prev), Some(now)) = (self.basic_pinch.take(), self.basic_pinch_state()) {
                    if prev.0 > 1.0 {
                        self.ws.fit_mode = false;
                        let zoom = self.ws.view.zoom * now.0 / prev.0;
                        self.ws.zoom_about(zoom, now.1);
                    }
                    self.ws.pan_px(now.1[0] - prev.1[0], now.1[1] - prev.1[1]);
                    self.basic_pinch = Some(now);
                }
                self.ws.redraw = true;
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.basic_touches.remove(&id);
                self.basic_pinch = self.basic_pinch_state();
            }
        }
    }

    fn basic_pinch_state(&self) -> Option<(f32, [f32; 2])> {
        let mut ids: Vec<&u64> = self.basic_touches.keys().collect();
        ids.sort();
        let (a, b) = (self.basic_touches.get(ids.first()?)?, self.basic_touches.get(ids.get(1)?)?);
        Some((((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt(), [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]))
    }

    /// Whether a second finger or the pinch fallback means the zoom is in motion.
    pub fn pinching(&self) -> bool {
        self.basic_pinch.is_some() || self.fingers.fingers() >= 2
    }
}
