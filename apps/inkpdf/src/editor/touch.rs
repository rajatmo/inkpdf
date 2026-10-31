//! Touch: pans and zooms. A palm is ignored. With the finger button down (see [`Editor::set_finger_draws`]) one finger on the page draws as the
//! stylus does, with whatever tool is in use; a second finger calls the stroke off and the two pinch and pan, so a person with no stylus has every tool
//! and still moves about the page.

use crate::input::{Device, GestureDelta, Phase, PointerEvent, Sample};

use super::{Act, Editor};

/// Seconds of its lift-off speed a flicked page keeps travelling for.
const FLING_SECONDS: f32 = 0.22;
/// The finger that is drawing as the stylus does. The stroke is not begun until the finger has moved a little (or lifts): a second finger that comes down
/// with it, as a pinch does, then finds nothing drawn that has to be taken back.
#[derive(Clone, Debug)]
pub(super) struct FingerPen {
    id: u32,
    /// Where and when it went down, and where it last was.
    start: Sample,
    last: Sample,
    /// The tool has been handed the press; until it has, these are the moves since.
    began: bool,
    waiting: Vec<Sample>,
}

/// A touch bigger than this (in logical pixels, about 28 mm) is a palm.
pub(super) const PALM_PX: f32 = 106.0;
/// How far a finger that stands in for the stylus moves before the stroke is begun, in logical pixels (a little less than a tap's slop).
const FINGER_BEGIN_PX: f32 = 8.0;
/// The pressure a finger has when it stands in for the stylus (a finger has none to speak of: it is a mouse's).
const FINGER_PRESSURE: f32 = 0.5;

impl Editor {
    pub(super) fn touch_event(&mut self, ev: &PointerEvent) {
        if !self.palm.admit_touch(ev, PALM_PX * self.ws.dpr) {
            return;
        }
        let (Some(first), Some(last)) = (ev.samples.first(), ev.samples.last()) else { return };
        match ev.phase {
            Phase::Down => {
                let first_finger = self.fingers.fingers() == 0 && matches!(self.chrome.act, Act::Idle) && self.finger.is_none();
                if first_finger {
                    self.dismiss_theme_menu(first.pos);
                    self.dismiss_pill(first.pos);
                }
                if first_finger && self.press_panel(Device::Touch, ev.id, first.pos) {
                    return;
                }
                let bar = self.toolbar();
                if first_finger && bar.contains(first.pos) {
                    self.chrome.act = Act::Toolbar { dev: Device::Touch, id: ev.id, item: bar.hit(first.pos) };
                    // (The button is shown pressed.)
                    self.ws.redraw = true;
                    return;
                }
                if first_finger && self.press_context(Device::Touch, ev.id, first.pos) {
                    return;
                }
                self.ws.view.stop();
                if first_finger && self.finger_draws && self.ws.has_doc {
                    self.finger_down(ev.id, first.pos, first.t_ms);
                    return;
                }
                self.fingers.set_scale(self.ws.dpr);
                // A second finger while one draws: the stroke is called off (if it was begun), and the two pinch and pan from where they are.
                if let Some(drawing) = self.finger.take() {
                    if drawing.began {
                        self.finger_route_samples(Phase::Cancel, &[drawing.last]);
                    }
                    self.fingers.down(drawing.id, drawing.last.pos, drawing.last.t_ms);
                }
                if self.fingers.fingers() == 0 {
                    self.end_pull();
                }
                self.fingers.down(ev.id, first.pos, first.t_ms);
            }
            Phase::Move => {
                if self.finger.as_ref().is_some_and(|f| f.id == ev.id) {
                    self.finger_moved(&ev.samples);
                    return;
                }
                // A finger on the toolbar or its slider is not panning the page.
                if self.act_belongs_to(Device::Touch, ev.id)
                    && matches!(self.chrome.act, Act::Toolbar { .. } | Act::Card { .. } | Act::Slider { .. } | Act::Swallow { .. })
                {
                    self.pointer_moved(Device::Touch, ev.id, &ev.samples);
                    return;
                }
                for s in &ev.samples {
                    self.fingers.set_scale(self.ws.dpr);
                    if let Some(d) = self.fingers.moved(ev.id, s.pos, s.t_ms) {
                        self.apply_touch(d);
                    }
                }
            }
            Phase::Up | Phase::Cancel => {
                // (A touch the system gave up on presses, taps and flings nothing.)
                let lifted = ev.phase == Phase::Up;
                if self.finger.as_ref().is_some_and(|f| f.id == ev.id) {
                    self.finger_lifted(&ev.samples, lifted);
                    return;
                }
                if matches!(self.chrome.act, Act::Toolbar { dev: Device::Touch, id, .. } if id == ev.id) {
                    self.release_toolbar(Device::Touch, ev.id, lifted.then_some(last.pos));
                    self.chrome.act = Act::Idle;
                    self.ws.redraw = true;
                    return;
                }
                if matches!(self.chrome.act, Act::Card { dev: Device::Touch, id, .. } if id == ev.id) {
                    self.release_context(Device::Touch, ev.id, lifted.then_some(last.pos));
                    self.chrome.act = Act::Idle;
                    self.ws.redraw = true;
                    return;
                }
                if matches!(self.chrome.act, Act::Slider { dev: Device::Touch, id, .. } | Act::Swallow { dev: Device::Touch, id } if id == ev.id) {
                    self.pointer_moved(Device::Touch, ev.id, &ev.samples);
                    self.end_act();
                    return;
                }
                self.fingers.set_scale(self.ws.dpr);
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
                        self.tap_on_page(pos, last.t_ms);
                    }
                }
            }
            Phase::Hover | Phase::Leave => {}
        }
        self.ws.redraw = true;
    }

    /// A finger went down on the page with the finger button down: it is the stylus, for the tool in use.
    fn finger_down(&mut self, id: u32, pos: [f32; 2], t_ms: f64) {
        self.tools.end_all(&mut self.ws);
        let at = Sample { pos, pressure: FINGER_PRESSURE, t_ms };
        self.finger = Some(FingerPen { id, start: at, last: at, began: false, waiting: Vec::new() });
        self.ws.redraw = true;
    }

    /// The finger drawing moved through `samples`: once it has gone far enough the tool is handed the press where it went down, and the moves since.
    fn finger_moved(&mut self, samples: &[Sample]) {
        let reach = FINGER_BEGIN_PX * self.ws.dpr;
        let Some(f) = &mut self.finger else { return };
        let moved: Vec<Sample> = samples.iter().map(|s| Sample { pressure: FINGER_PRESSURE, ..*s }).collect();
        if let Some(&last) = moved.last() {
            f.last = last;
        }
        if f.began {
            self.finger_route_samples(Phase::Move, &moved);
        } else {
            f.waiting.extend(moved);
            let far = f.waiting.last().is_some_and(|l| (l.pos[0] - f.start.pos[0]).hypot(l.pos[1] - f.start.pos[1]) > reach);
            if far {
                f.began = true;
                let (start, waiting) = (f.start, std::mem::take(&mut f.waiting));
                self.finger_route_samples(Phase::Down, &[start]);
                self.finger_route_samples(Phase::Move, &waiting);
            }
        }
        self.ws.redraw = true;
    }

    /// The finger drawing lifted (or was taken from us): a touch that never went far is a tap of the tip.
    fn finger_lifted(&mut self, samples: &[Sample], lifted: bool) {
        let Some(f) = self.finger.take() else { return };
        let last = samples.last().map_or(f.last, |s| Sample { pressure: FINGER_PRESSURE, ..*s });
        if !f.began && lifted {
            self.finger_route_samples(Phase::Down, &[f.start]);
        }
        if f.began || lifted {
            self.finger_route_samples(if lifted { Phase::Up } else { Phase::Cancel }, &[last]);
        }
        self.ws.redraw = true;
    }

    /// Whether a finger is drawing now.
    #[cfg(test)]
    pub fn finger_is_drawing(&self) -> bool {
        self.finger.is_some()
    }

    /// Every finger's business is called off, as when the pen comes near: no pan, pinch, toolbar press or fling goes on, and
    /// nothing is activated.
    pub(super) fn cancel_touches(&mut self) {
        self.fingers = crate::input::TouchGestures::default();
        self.end_pull();
        if let Some(drawing) = self.finger.take()
            && drawing.began
        {
            self.finger_route_samples(Phase::Cancel, &[drawing.last]);
        }
        if matches!(
            self.chrome.act,
            Act::Toolbar { dev: Device::Touch, .. }
                | Act::Card { dev: Device::Touch, .. }
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
