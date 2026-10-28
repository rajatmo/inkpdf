//! The "save your changes?" prompt, from the editor's side. While it is up every device talks to it
//! and to nothing else: the pen does not draw, a finger does not pan, the wheel does not scroll,
//! and no shortcut fires. A press on a button only counts if it is released on the same button.

use crate::input::{Device, Phase, PointerEvent};
use crate::toolbar::{Choice, PromptLayout, PromptView};

use super::{Editor, Request};

/// The pointer that pressed a button, and which button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Grab {
    dev: Device,
    id: u32,
    choice: Choice,
}

impl Editor {
    /// Put the prompt up. Whatever was under way stops.
    pub fn ask_unsaved(&mut self) {
        // An eraser stroke that was under way is called off: what it had touched is not erased.
        self.erasing.clear();
        self.act = super::Act::Idle;
        self.mouse_press = None;
        self.panel = None;
        self.hover = None;
        self.tools.cancel_all(&mut self.ws);
        self.fingers = crate::input::TouchGestures::default();
        self.end_pull();
        self.prompt = Some(PromptView::default());
        self.prompt_grab = None;
        self.requests.clear_answer();
        self.redraw = true;
    }

    fn prompt_layout(&self) -> PromptLayout {
        PromptLayout::new(self.view.viewport, self.dpr)
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
        self.redraw = true;
        let over = self.prompt_layout().hit(pos);
        let mine = |grab: &Option<Grab>| grab.is_some_and(|g| g.dev == dev && g.id == id);
        match phase {
            Phase::Down => self.prompt_grab = over.map(|choice| Grab { dev, id, choice }),
            Phase::Up => {
                if mine(&self.prompt_grab)
                    && let Some(grab) = self.prompt_grab.take()
                    && over == Some(grab.choice)
                {
                    self.answer_with(grab.choice);
                    return;
                }
            }
            Phase::Cancel => {
                if mine(&self.prompt_grab) {
                    self.prompt_grab = None;
                }
            }
            Phase::Move | Phase::Hover | Phase::Leave => {}
        }
        let pressed = self.prompt_grab.filter(|g| over == Some(g.choice)).map(|g| g.choice);
        self.prompt = Some(PromptView { hover: if phase == Phase::Leave { None } else { over }, pressed });
    }

    fn answer_with(&mut self, choice: Choice) {
        self.requests.push(Request::Answer(choice));
        self.prompt = None;
        self.prompt_grab = None;
        self.redraw = true;
    }
}
