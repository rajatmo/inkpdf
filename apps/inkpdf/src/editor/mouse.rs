//! The mouse scrolls the page: it does not draw, point or pan, and has no menu. It does press the
//! buttons of the toolbars, the tool panel and the "save your changes?" prompt, which are part of the
//! window, not of the page. With the text tool it also places, selects and moves text boxes (see
//! `text`).

use winit::event::{ElementState, MouseButton, MouseScrollDelta};

use crate::input::{Device, Phase};

use super::{Act, Editor};

/// Pixels one mouse-wheel notch scrolls.
const WHEEL_NOTCH_PX: f32 = 110.0;
/// The toolbar sits within this many logical pixels of the top of the window.
const TOOLBAR_STRIP_PX: f32 = 90.0;

impl Editor {
    pub(crate) fn mouse_moved(&mut self, pos: [f32; 2]) {
        self.cursor = pos;
        if self.prompt.is_some() {
            self.prompt_pointer(Device::Mouse, 0, Phase::Move, pos);
            return;
        }
        self.update_hover(Some(pos));
        match self.act {
            Act::Slider { dev: Device::Mouse, slider, .. } => self.drag_slider(slider, pos[0]),
            // A text box being dragged with the text tool.
            _ if self.has_doc => {
                self.route_mouse(Phase::Move, pos);
            }
            _ => {}
        }
    }

    /// The mouse left the window.
    pub(crate) fn mouse_left(&mut self) {
        self.update_hover(None);
    }

    /// Track which toolbar button the mouse is over, drawing again only when that changes.
    fn update_hover(&mut self, pos: Option<[f32; 2]>) {
        // Only the strip the toolbar can be in needs the (small) layout work.
        let hover = pos.filter(|p| p[1] < TOOLBAR_STRIP_PX * self.dpr).and_then(|p| self.toolbar().hit(p));
        if hover != self.hover {
            self.hover = hover;
            self.redraw = true;
        }
    }

    /// The left button presses what is under it if that is part of the window (a toolbar button, the
    /// tool panel, the prompt); on the page it does nothing, and the other buttons never do anything.
    pub(crate) fn mouse_button(&mut self, state: ElementState, button: MouseButton) {
        if button != MouseButton::Left {
            return;
        }
        let pressed = state == ElementState::Pressed;
        if self.prompt.is_some() {
            self.prompt_pointer(Device::Mouse, 0, if pressed { Phase::Down } else { Phase::Up }, self.cursor);
            return;
        }
        if pressed {
            if !matches!(self.act, Act::Idle) {
                self.end_act();
            }
            self.tools.end_all(&mut self.ws);
            let bar = self.toolbar();
            if self.press_panel(Device::Mouse, 0, self.cursor) {
                // The tool panel took the press.
            } else if bar.contains(self.cursor) {
                self.act = Act::Toolbar { dev: Device::Mouse, id: 0, item: bar.hit(self.cursor) };
            } else if self.has_doc {
                // With the text tool the mouse places, selects and moves text boxes (the keyboard types);
                // the other tools' policies ignore it.
                self.route_mouse(Phase::Down, self.cursor);
            }
        } else if self.act_belongs_to(Device::Mouse, 0) {
            self.release_toolbar(Device::Mouse, 0, Some(self.cursor));
            self.end_act();
        } else {
            self.route_mouse(Phase::Up, self.cursor);
        }
        self.redraw = true;
    }

    /// The wheel scrolls, up and down and (for a wheel that tilts or a touchpad) sideways.
    pub fn wheel(&mut self, delta: MouseScrollDelta) {
        if self.prompt.is_some() {
            return;
        }
        match delta {
            MouseScrollDelta::LineDelta(x, y) => self.scroll_by_px(-x * WHEEL_NOTCH_PX, y * WHEEL_NOTCH_PX),
            // Precision touchpads already glide: follow them exactly.
            MouseScrollDelta::PixelDelta(p) => self.pan_px(p.x as f32, p.y as f32),
        }
        self.redraw = true;
    }
}
