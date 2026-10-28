//! Handing the page's input to the tool in use. The session does not know what any tool does with it: it
//! asks the tool's [`InputPolicy`](crate::tools::InputPolicy) which devices it listens to, and passes on
//! the pointer and the keyboard. The accessors here are how the shell and the tests look at what a tool
//! holds.

use crate::figure::Figure;
use crate::geometry::Shape;
use crate::input::{Device, PenButtons, Phase, Sample};
use crate::toolbar::Tool;
use crate::tools::{Key, PageEvent, TextEdit};

use super::{Editor, Request};

impl Editor {
    /// The text box being typed into, for drawing it.
    pub fn text_edit(&self) -> Option<&TextEdit> {
        self.tools.text.edit()
    }

    /// The pointer tool's dot and trail.
    #[cfg(test)]
    pub fn laser(&self) -> &crate::laser::Laser {
        &self.tools.pointer.laser
    }

    /// The polyline or spline being drawn by taps, if any.
    #[cfg(test)]
    pub fn draft(&self) -> Option<&crate::tools::Draft> {
        self.tools.shape.draft.as_ref()
    }

    /// What to draw for the stroke being drawn by taps: its outline and its nodes.
    #[cfg(test)]
    pub fn draft_preview(&self) -> Option<(Vec<crate::ink::P>, Vec<crate::ink::P>)> {
        self.tools.shape.draft_preview()
    }

    /// End the stroke being drawn by taps as it is (open), so that it is part of the document.
    pub fn finish_draft(&mut self) {
        self.tools.shape.finish_draft(&mut self.ws);
    }

    /// The single selected shape, and the figure to show for it, while the shape tool is in use.
    pub fn editing(&self) -> Option<(u64, &Figure)> {
        if self.tool != Tool::Geometry {
            return None;
        }
        self.tools.shape.editing(&self.ws)
    }

    /// The anchor of the shape being edited that was pressed last.
    #[cfg(test)]
    pub fn selected_anchor(&self) -> Option<usize> {
        self.tools.shape.selected_anchor(&self.ws)
    }

    /// Where a drag has snapped to an anchor of another shape.
    #[cfg(test)]
    pub fn snap(&self) -> Option<crate::ink::P> {
        self.tools.shape.snap()
    }

    /// Whether nothing is under way: no press on the window, no gesture of any tool.
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        matches!(self.act, super::Act::Idle) && !self.tools.busy()
    }

    /// Whether the keyboard is heard: a text box is being typed into.
    pub fn is_typing(&self) -> bool {
        self.tool == Tool::Text && self.tools.text.is_typing() && self.prompt.is_none()
    }

    /// Type `typed` at the end of the text (see [`crate::text::clean`]). Heard only while typing.
    pub fn type_text(&mut self, typed: &str) {
        self.key(Key::Text(typed));
    }

    /// Take the last character away (a new line goes whole).
    pub fn backspace(&mut self) {
        self.key(Key::Backspace);
    }

    /// Escape ends the edit of the text box: the typing becomes an undo step of its own.
    pub fn escape(&mut self) {
        self.key(Key::Escape);
    }

    /// End the edit of the text box, as an undo step of its own, whatever tool is in use.
    #[cfg(test)]
    pub fn finish_text(&mut self) {
        self.tools.text.finish_text(&mut self.ws);
    }

    /// The keyboard, if the tool in use listens to it.
    fn key(&mut self, key: Key<'_>) -> bool {
        if self.prompt.is_some() {
            return false;
        }
        let tool = self.tools.get_mut(self.ws.tool);
        tool.policy().keyboard && tool.key(&mut self.ws, key)
    }

    /// The colour or size in the panel changed.
    pub(super) fn restyle_tool(&mut self) {
        self.tools.get_mut(self.ws.tool).restyled(&mut self.ws);
    }

    /// Choose the shape the shape tool makes.
    pub(super) fn pick_shape(&mut self, shape: Shape) {
        if shape != self.shape {
            self.finish_draft();
            self.shape = shape;
            self.requests.push(Request::SettingsChanged);
            self.redraw = true;
        }
    }

    /// Hand a stylus or mouse message on the page to the tool in use, if it listens to that device (or
    /// already has a gesture of it under way).
    pub(super) fn route_to_tool(&mut self, dev: Device, id: u32, phase: Phase, samples: &[Sample], pen: PenButtons) {
        let tool = self.tools.get_mut(self.ws.tool);
        let policy = tool.policy();
        let listens = match dev {
            Device::Pen => policy.stylus,
            Device::Mouse => policy.mouse,
            Device::Touch => false,
        };
        if listens || tool.owns(dev, id) {
            tool.pointer(&mut self.ws, &PageEvent { dev, id, phase, samples, pen });
        }
    }

    /// A mouse message at `pos` (screen) on the page.
    pub(super) fn route_mouse(&mut self, phase: Phase, pos: [f32; 2]) {
        let samples = [Sample { pos, pressure: 0.5, t_ms: crate::win_pointer::now_ms() }];
        self.route_to_tool(Device::Mouse, 0, phase, &samples, PenButtons::default());
    }

    /// Whether a point is on a toolbar or the tool panel.
    pub(super) fn over_interface(&self, p: [f32; 2]) -> bool {
        let bar = self.toolbar();
        bar.contains(p)
            || self
                .panel
                .is_some_and(|s| bar.tool_panel(self.tool, self.view.viewport[0], s.editing.is_some()).is_some_and(|panel| panel.contains(p)))
    }

    /// The pen hovers at `pos` (or has gone): the tool's dot or draft follows it, except over the toolbars.
    pub(super) fn route_hover(&mut self, pos: Option<[f32; 2]>) {
        let free = pos.filter(|&p| self.has_doc && !self.over_interface(p));
        let here;
        let (phase, samples): (Phase, &[Sample]) = match free {
            Some(pos) => {
                here = [Sample { pos, pressure: 0.0, t_ms: crate::win_pointer::now_ms() }];
                (Phase::Hover, &here)
            }
            None => (Phase::Leave, &[]),
        };
        self.route_to_tool(Device::Pen, 0, phase, samples, PenButtons::default());
    }
}
