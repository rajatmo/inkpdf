//! The "save your changes?" prompt, from the editor's side. While it is up every device talks to it
//! and to nothing else: the pen does not draw, a finger does not pan, the wheel does not scroll,
//! and no shortcut fires. (The prompt's buttons are the chrome's: see [`super::chrome::Chrome`].)

use crate::input::{Device, Phase, PointerEvent};

use super::Editor;

impl Editor {
    /// Put the prompt up. Whatever was under way stops.
    pub fn ask_unsaved(&mut self) {
        // An eraser stroke that was under way is called off: what it had touched is not erased.
        self.ws.erasing.clear();
        self.mouse_press = None;
        self.chrome.put_up_prompt();
        self.tools.cancel_all(&mut self.ws);
        self.fingers = crate::input::TouchGestures::default();
        self.end_pull();
        self.ws.requests.clear_answer();
        self.ws.redraw = true;
    }

    /// A pen or touch event while the prompt is up.
    pub(super) fn prompt_event(&mut self, ev: &PointerEvent) {
        let sample = if ev.phase == Phase::Down { ev.samples.first() } else { ev.samples.last() };
        if let Some(s) = sample {
            self.prompt_pointer(ev.device, ev.id, ev.phase, s.pos);
        }
    }

    /// One pointer's position and phase.
    pub(super) fn prompt_pointer(&mut self, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        self.chrome.prompt_pointer(&mut self.ws, dev, id, phase, pos);
    }
}
