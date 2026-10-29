//! Routing a pointer to the interaction it belongs to.

use crate::input::{Device, InputEvent, PointerEvent, Sample};

use super::{Act, Editor};

impl Editor {
    /// The one way in for what the person does: every device, and the window's own changes. The shell translates what its
    /// toolkit says into an [`InputEvent`]; a replay of a recorded session hands over the same events, so the editor behaves
    /// alike whoever is driving it. (The methods this calls are also public, as the tests use them.)
    pub fn handle(&mut self, event: InputEvent) {
        match event {
            InputEvent::Pointer(ev) => self.pointer_event(ev),
            InputEvent::MouseMoved(pos) => self.mouse_moved(pos),
            InputEvent::MouseButton { state, button } => self.mouse_button(state, button),
            InputEvent::MouseLeft => self.mouse_left(),
            InputEvent::Wheel(delta) => self.wheel(delta),
            InputEvent::Key(key) => {
                self.press(key.as_key());
            }
            InputEvent::Shortcut { letter, shift } => {
                self.shortcut(&letter, shift);
            }
            InputEvent::FocusLost => self.focus_lost(),
            InputEvent::Resized(size) => self.set_viewport(size),
            InputEvent::ScaleChanged(dpr) => self.set_dpr(dpr),
        }
    }

    /// Whether the chrome interaction under way belongs to this pointer.
    pub(super) fn act_belongs_to(&self, dev: Device, id: u32) -> bool {
        match &self.chrome.act {
            Act::Toolbar { dev: d, id: a, .. }
            | Act::Context { dev: d, id: a, .. }
            | Act::Slider { dev: d, id: a, .. }
            | Act::Swallow { dev: d, id: a } => *d == dev && *a == id,
            Act::Idle => false,
        }
    }

    /// A pen or touch message from the Windows pointer layer.
    pub(crate) fn pointer_event(&mut self, ev: PointerEvent) {
        if self.chrome.prompt.is_some() {
            self.prompt_event(&ev);
            return;
        }
        if self.chrome.save_as.is_some() {
            if ev.device != Device::Mouse {
                self.save_as_event(&ev);
            }
            return;
        }
        if self.chrome.overview.is_some() {
            if ev.device == Device::Pen {
                self.palm.pen_event(&ev, (self.clock)());
            }
            if ev.device != Device::Mouse {
                self.overview_event(&ev);
            }
            return;
        }
        match ev.device {
            Device::Pen => {
                self.palm.pen_event(&ev, (self.clock)());
                self.pen_event(&ev);
            }
            Device::Touch => self.touch_event(&ev),
            Device::Mouse => {}
        }
    }

    /// The pointer `(dev, id)` moved through `samples` (while a chrome interaction is under way).
    pub(super) fn pointer_moved(&mut self, dev: Device, id: u32, samples: &[Sample]) {
        if !self.act_belongs_to(dev, id) {
            return;
        }
        if let Act::Slider { slider, .. } = self.chrome.act
            && let Some(s) = samples.last()
        {
            self.drag_slider(slider, s.pos[0]);
        }
    }
}
