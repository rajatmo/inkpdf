//! The mouse. Its wheel always scrolls. Its buttons press the window's own controls (the toolbars, the tool
//! panel, the "save your changes?" prompt), and on the page they do what the tool in use and the mode say (see
//! [`crate::tools::MouseUse`]): in the desktop mode the pen, shape and pointer tools take the mouse for a stylus
//! (left button the tip, right the eraser end, middle the barrel button) and the text tool takes it as itself
//! (click to place, edit and move text); in the tablet mode it only scrolls.

use winit::event::{ElementState, MouseButton, MouseScrollDelta};

use crate::input::{Device, PenButtons, Phase};

use super::{Act, Editor};

/// Pixels one mouse-wheel notch scrolls.
const WHEEL_NOTCH_PX: f32 = 110.0;
/// The toolbar sits within this many logical pixels of the top of the window.
const TOOLBAR_STRIP_PX: f32 = 90.0;

/// A button held down on the page, and what it stands for (so its moves and its release say the same).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MousePress {
    button: MouseButton,
    pen: PenButtons,
}

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
            _ if !self.has_doc => {}
            // A button held on the page: the gesture follows.
            _ if self.mouse_press.is_some() => {
                let pen = self.mouse_press.map(|p| p.pen).unwrap_or_default();
                self.route_mouse(Phase::Move, pos, pen);
            }
            // Over the page with nothing held: a mouse that draws shows what a hovering stylus does (the pointer's
            // dot, the next segment of a polyline).
            _ if self.mouse_draws() => self.route_hover(Some(pos)),
            _ => {
                self.route_mouse(Phase::Move, pos, PenButtons::default());
            }
        }
    }

    /// The mouse left the window.
    pub(crate) fn mouse_left(&mut self) {
        self.update_hover(None);
        if self.mouse_draws() && self.mouse_press.is_none() {
            self.route_hover(None);
        }
    }

    /// The window lost the keyboard focus: a button released elsewhere never reaches us, so what the mouse was
    /// in the middle of is ended now.
    pub(crate) fn focus_lost(&mut self) {
        if self.mouse_press.take().is_some() {
            self.tools.end_all(&mut self.ws);
            self.redraw = true;
        }
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

    /// What the buttons of the stylus the mouse stands in for are, for `button`.
    fn stylus_for(button: MouseButton) -> PenButtons {
        PenButtons { in_range: true, contact: true, eraser: button == MouseButton::Right, barrel: button == MouseButton::Middle }
    }

    /// A mouse button went down or up. The left button presses what is under it if that is part of the window
    /// (a toolbar button, the tool panel, the prompt); anywhere on the page, any of the three starts what the tool
    /// and the mode make of a mouse.
    pub(crate) fn mouse_button(&mut self, state: ElementState, button: MouseButton) {
        if !matches!(button, MouseButton::Left | MouseButton::Right | MouseButton::Middle) {
            return;
        }
        let pressed = state == ElementState::Pressed;
        if self.prompt.is_some() {
            if button == MouseButton::Left {
                self.prompt_pointer(Device::Mouse, 0, if pressed { Phase::Down } else { Phase::Up }, self.cursor);
            }
            return;
        }
        if pressed {
            self.mouse_pressed(button);
        } else {
            self.mouse_released(button);
        }
        self.redraw = true;
    }

    fn mouse_pressed(&mut self, button: MouseButton) {
        let on_page = !self.over_interface(self.cursor);
        // The other buttons only mean something on the page, to a mouse that draws, with nothing else going on.
        if button != MouseButton::Left && !(self.mouse_draws() && self.has_doc && self.panel.is_none() && on_page) {
            return;
        }
        // Another button pressed while one is held on the page changes nothing. (The same button pressed again can
        // only mean its release was lost: that gesture is over, and this is a new press.)
        if let Some(held) = self.mouse_press {
            if held.button != button {
                return;
            }
            self.mouse_press = None;
        }
        if !matches!(self.act, Act::Idle) {
            self.end_act();
        }
        self.tools.end_all(&mut self.ws);
        let bar = self.toolbar();
        if button == MouseButton::Left && self.press_panel(Device::Mouse, 0, self.cursor) {
            // The tool panel took the press.
        } else if button == MouseButton::Left && bar.contains(self.cursor) {
            self.act = Act::Toolbar { dev: Device::Mouse, id: 0, item: bar.hit(self.cursor) };
        } else if self.has_doc && on_page {
            let pen = Self::stylus_for(button);
            self.mouse_press = Some(MousePress { button, pen });
            self.route_mouse(Phase::Down, self.cursor, pen);
        }
    }

    fn mouse_released(&mut self, button: MouseButton) {
        if button == MouseButton::Left && self.act_belongs_to(Device::Mouse, 0) {
            self.release_toolbar(Device::Mouse, 0, Some(self.cursor));
            self.end_act();
        } else if let Some(held) = self.mouse_press.filter(|p| p.button == button) {
            self.mouse_press = None;
            self.route_mouse(Phase::Up, self.cursor, held.pen);
        }
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
