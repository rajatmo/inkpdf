//! Routing a pointer to the interaction it belongs to.

use crate::input::{Device, PointerEvent, Sample};

use super::{Act, Editor};

impl Editor {
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
