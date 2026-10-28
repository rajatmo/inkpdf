//! Toolbar presses, the tools, and the tool panel under the toolbar.

use crate::input::Device;
use crate::toolbar::{self, Channel, Item, PanelState, Slider, Tool};

use super::{Act, Editor, Request};

impl Editor {
    /// If the pointer pressed a toolbar button and is still over it, press it.
    pub(super) fn release_toolbar(&mut self, dev: Device, id: u32, pos: Option<[f32; 2]>) {
        let Act::Toolbar { dev: d, id: a, item: Some(item) } = self.act else { return };
        if d != dev || a != id {
            return;
        }
        if pos.is_some_and(|p| self.toolbar().hit(p) == Some(item)) {
            self.activate(item);
        }
    }

    /// A press while the tool panel is open. On one of its sliders it sets that slider and starts a
    /// drag; on a shape or a colour it picks it; elsewhere on the panel it does nothing; anywhere
    /// else it puts the panel away. Returns whether the press was used up (a press on another toolbar
    /// button goes on to press that button).
    pub(super) fn press_panel(&mut self, dev: Device, id: u32, pos: [f32; 2]) -> bool {
        let Some(state) = self.panel else { return false };
        self.redraw = true;
        let (bar, viewport_width) = (self.toolbar(), self.view.viewport[0]);
        if let Some(panel) = bar.tool_panel(self.tool, viewport_width, state.editing.is_some()).filter(|p| p.contains(pos)) {
            if let Some(slider) = panel.slider_at(pos) {
                self.set_slider(slider, panel.value_at(slider, pos[0]));
                self.act = Act::Slider { dev, id, slider };
            } else {
                if let Some(shape) = panel.shape_at(pos) {
                    self.pick_shape(shape);
                } else if let Some(slot) = panel.swatch_at(pos) {
                    self.press_swatch(slot);
                }
                self.act = Act::Swallow { dev, id };
            }
            return true;
        }
        self.panel = None;
        // The tool's button is how the panel is put away, so that press must not reopen it; another
        // toolbar button is pressed as usual; a press elsewhere only closes the panel.
        if bar.contains(pos) && bar.hit(pos) != Some(Item::Tool(self.tool)) {
            return false;
        }
        self.act = Act::Swallow { dev, id };
        true
    }

    /// Move a slider to the pointer at `x`.
    pub(super) fn drag_slider(&mut self, slider: Slider, x: f32) {
        let Some(state) = self.panel else { return };
        let (bar, viewport_width) = (self.toolbar(), self.view.viewport[0]);
        if let Some(panel) = bar.tool_panel(self.tool, viewport_width, state.editing.is_some()) {
            self.set_slider(slider, panel.value_at(slider, x));
        }
    }

    pub(super) fn set_slider(&mut self, slider: Slider, value: f32) {
        if !value.is_finite() {
            return;
        }
        match slider {
            Slider::Width => {
                let (least, most) = toolbar::width_range(self.tool);
                self.style_mut().width = value.clamp(least, most);
            }
            Slider::Smoothing => self.smoothing = value.clamp(0.0, 1.0),
            Slider::Channel(channel) => self.set_channel(channel, value),
        }
        self.restyle_tool();
        self.redraw = true;
    }

    /// Press a colour in the panel: another colour is picked; the colour in use opens its editor,
    /// or puts it away if it is open.
    fn press_swatch(&mut self, slot: usize) {
        let editing = self.panel.and_then(|p| p.editing);
        if slot == self.style().colour {
            self.set_editing(if editing == Some(slot) { None } else { Some(slot) });
        } else if slot < self.palette.len() {
            self.style_mut().colour = slot;
            self.requests.push(Request::SettingsChanged);
            self.restyle_tool();
            if editing.is_some() {
                self.set_editing(Some(slot));
            }
        }
    }

    /// Open the colour editor for colour slot `slot` (starting from the colour it has now), or put
    /// it away (`None`).
    pub(super) fn set_editing(&mut self, slot: Option<usize>) {
        let colour = slot.and_then(|s| self.palette.get(s));
        if slot.is_some() && colour.is_none() {
            return;
        }
        if let Some(c) = colour {
            self.colour_hsv = crate::colour::rgb_to_hsv([c[0], c[1], c[2]]);
        }
        if let Some(panel) = &mut self.panel {
            panel.editing = slot;
        }
        self.redraw = true;
    }

    /// Set one part of the colour being edited, and use the result as the pen colour.
    pub fn set_channel(&mut self, channel: Channel, value: f32) {
        let Some(slot) = self.panel.and_then(|p| p.editing) else { return };
        let part = match channel {
            Channel::Hue => 0,
            Channel::Saturation => 1,
            Channel::Value => 2,
        };
        self.colour_hsv[part] = value;
        let rgb = crate::colour::hsv_to_rgb(self.colour_hsv);
        if let Some(cell) = self.palette.get_mut(slot) {
            *cell = [rgb[0], rgb[1], rgb[2], 255];
            self.style_mut().colour = slot;
        }
        self.redraw = true;
    }

    /// Switch the tool. Whatever was selected is let go (and any typing finished), the
    /// laser starts clean, and the panel (which belongs to the tool) goes away. The choice is
    /// remembered.
    pub(super) fn set_tool(&mut self, tool: Tool) {
        if self.tool != tool {
            self.settle();
            self.tool = tool;
            self.selection.clear();
            self.tools.leave_all(&mut self.ws);
            self.panel = None;
            self.requests.push(Request::SettingsChanged);
        }
        self.redraw = true;
    }

    /// Pressing a button. A tool's button selects the tool and opens its panel (the laser has
    /// none), and pressing it again puts the panel away.
    pub(super) fn activate(&mut self, item: Item) {
        if let Item::Tool(tool) = item {
            let was_open = self.tool == tool && self.panel.is_some();
            self.set_tool(tool);
            self.panel = (tool != Tool::Laser && !was_open).then(PanelState::default);
        } else {
            self.panel = None;
            // Undo and redo take back the typing first; anything else keeps it.
            if !matches!(item, Item::Undo | Item::Redo) {
                self.settle();
            }
            match item {
                Item::Undo => self.undo(),
                Item::Redo => self.redo(),
                Item::Save => self.requests.push(Request::Save),
                Item::Fullscreen => self.requests.push(Request::Fullscreen),
                Item::Close => self.requests.push(Request::Close),
                Item::AddPage => {
                    let page = self.view.current_page(&self.layout);
                    self.requests.push(Request::InsertPage(page));
                }
                // The last page there is stays (a document has at least one).
                Item::DeletePage if self.layout.len() > 1 => {
                    let page = self.view.current_page(&self.layout);
                    self.requests.push(Request::DeletePage(page));
                }
                Item::DeletePage => {}
                Item::Tool(_) => {}
            }
        }
        self.redraw = true;
    }
}
