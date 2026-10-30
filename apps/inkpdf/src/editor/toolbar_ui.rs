//! Toolbar presses, the tools, and the tool panel under the toolbar.

use crate::ink::Restyle;
use crate::input::{Device, Mode};
use crate::scene::Xform;
use crate::tool::Tool;
use crate::toolbar::{Channel, Item, PromptLayout, Slider, Ui};

use super::{Act, Editor, Request};

impl Editor {
    /// Everything the interface over the page is made of, now: the toolbars with the bar under the tools and the tool's panel unfolded
    /// from it (or the prompt, which takes the window and puts the toolbars out of the way), each as far in as its motion has come. (The
    /// pages overview is drawn by [`Editor::draw_overview`].)
    pub fn draw_interface(&self, ui: &mut Ui) {
        let (state, look) = (self.toolbar_state(), self.toolbar_look());
        // The prompt takes over the window: the toolbars step out of the way.
        if self.chrome.prompt.is_none() {
            let bar = self.toolbar();
            bar.draw_with(&state, &look, ui);
            let coming = ui.mark();
            let unfolded = self
                .panel_look()
                .and_then(|(view, amount)| Some((view, amount, bar.tool_panel(view.tool, self.ws.view.viewport[0], view.editing.is_some())?)));
            // The bar under the tools; with the panel unfolded they are one card, drawn before the panel's contents. The card
            // grows to hold the panel as the panel comes in, and the panel's contents appear when there is room for them.
            // (A bar that is going out is drawn as it was, from what was kept.)
            let held = self.held_bar.filter(|_| self.motions.bar_amount() > 0.0);
            if let Some((context, view)) = self.context_bar().zip(self.context_view()).or(held) {
                let end = unfolded.as_ref().filter(|(_, amount, _)| *amount > 0.02).map(|(_, amount, panel)| {
                    let full = context.card(panel.rect[3] + 0.2 * panel.unit())[3].max(panel.rect[3]);
                    context.rect[3] + (full - context.rect[3]) * amount
                });
                context.draw(&view, end, ui);
            }
            if let Some((view, amount, panel)) = &unfolded {
                let contents = ui.mark();
                panel.draw(view, ui);
                ui.apply(contents, Xform::fade(((amount - 0.5) * 2.0).clamp(0.0, 1.0)));
            }
            // The bar comes down into place as it comes in.
            let came = self.motions.bar_amount();
            ui.apply(coming, Xform::slide([0.0, -(1.0 - came) * 8.0 * self.ws.dpr], came));
            if let (Some(card), Some(view)) = (self.theme_layout(), self.theme_view()) {
                card.draw(&view, ui);
            }
            if let (Some(card), Some(view)) = (self.search_layout(), self.search_view()) {
                card.draw(&view, ui);
            }
        }
        if let Some((view, amount)) = self.prompt_look() {
            PromptLayout::new(self.ws.view.viewport, self.ws.dpr).draw_with(view, amount, ui);
        }
    }

    /// If the pointer pressed a toolbar button and is still over it, press it.
    pub(super) fn release_toolbar(&mut self, dev: Device, id: u32, pos: Option<[f32; 2]>) {
        let Act::Toolbar { dev: d, id: a, item: Some(item) } = self.chrome.act else { return };
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
        let Some(state) = self.chrome.panel else { return false };
        self.ws.redraw = true;
        let (bar, viewport_width) = (self.toolbar(), self.ws.view.viewport[0]);
        if let Some(panel) = bar.tool_panel(self.ws.tool, viewport_width, state.editing.is_some()).filter(|p| p.contains(pos)) {
            if let Some(slider) = panel.slider_at(pos) {
                self.set_slider(slider, panel.value_at(slider, pos[0]));
                self.chrome.act = Act::Slider { dev, id, slider };
            } else {
                if let Some(shape) = panel.shape_at(pos) {
                    self.pick_shape(shape);
                } else if let Some(slot) = panel.swatch_at(pos) {
                    self.press_swatch(slot);
                }
                self.chrome.act = Act::Swallow { dev, id };
            }
            return true;
        }
        // The rest of the card the panel unfolds in belongs to it: a press there does nothing.
        if self.drawer().is_some_and(|d| crate::ink::rect_contains(d, pos)) && !self.context_bar().is_some_and(|b| b.contains(pos)) {
            self.chrome.act = Act::Swallow { dev, id };
            return true;
        }
        // The bar under the tools works on what the panel is changing: pressing it leaves the panel as it is.
        if self.context_bar().is_some_and(|b| b.contains(pos)) {
            return false;
        }
        self.chrome.panel = None;
        // The tool's button is how the panel is put away, so that press must not reopen it; another
        // toolbar button is pressed as usual; a press elsewhere only closes the panel.
        if bar.contains(pos) && bar.hit(pos) != Some(Item::Tool(self.ws.tool)) {
            return false;
        }
        self.chrome.act = Act::Swallow { dev, id };
        true
    }

    /// Move a slider to the pointer at `x`.
    pub(super) fn drag_slider(&mut self, slider: Slider, x: f32) {
        let Some(state) = self.chrome.panel else { return };
        let (bar, viewport_width) = (self.toolbar(), self.ws.view.viewport[0]);
        if let Some(panel) = bar.tool_panel(self.ws.tool, viewport_width, state.editing.is_some()) {
            // (Moving on from the press that began the drag: what it changes in the selection is one undo step.)
            self.set_slider_run(slider, panel.value_at(slider, x), true);
        }
    }

    pub(super) fn set_slider(&mut self, slider: Slider, value: f32) {
        self.set_slider_run(slider, value, false);
    }

    /// Move a slider. What the tool writes with changes, and so does what is selected (its colour or thickness);
    /// `continuing` says this is a later move of the same drag, which goes into the same undo step.
    fn set_slider_run(&mut self, slider: Slider, value: f32, continuing: bool) {
        if !value.is_finite() {
            return;
        }
        let change = match slider {
            Slider::Width => {
                let (least, most) = crate::tool::width_range(self.ws.tool);
                let width = value.clamp(least, most);
                self.ws.style_mut().width = width;
                Some(Restyle { width: Some(width), rgba: None })
            }
            Slider::Smoothing => {
                self.ws.smoothing = value.clamp(0.0, 1.0);
                None
            }
            Slider::Channel(channel) => {
                self.set_channel(channel, value);
                Some(Restyle { rgba: Some(self.colour_of(self.style())), width: None })
            }
        };
        self.restyle_tool();
        if let Some(change) = change {
            self.restyle_selected(change, continuing);
        }
        self.ws.redraw = true;
    }

    /// Give what is selected the colour or thickness just chosen. (A text box being typed into takes it through the
    /// tool instead; the pointer tool has nothing to restyle.)
    fn restyle_selected(&mut self, change: Restyle, continuing: bool) {
        if self.ws.tool.selects_items() && !self.is_typing() {
            self.ws.restyle_selection(change, continuing);
        }
    }

    /// Press a colour in the panel: another colour is picked; the colour in use opens its editor,
    /// or puts it away if it is open.
    pub(super) fn press_swatch(&mut self, slot: usize) {
        let editing = self.chrome.panel.and_then(|p| p.editing);
        if slot == self.style().colour {
            self.set_editing(if editing == Some(slot) { None } else { Some(slot) });
        } else if slot < self.ws.palette.len() {
            self.ws.style_mut().colour = slot;
            self.ws.requests.push(Request::SettingsChanged);
            self.restyle_tool();
            self.restyle_selected(Restyle { rgba: Some(self.colour_of(self.style())), width: None }, false);
            if editing.is_some() {
                self.set_editing(Some(slot));
            }
        }
    }

    /// Open the colour editor for colour slot `slot`, or put it away (`None`).
    pub(super) fn set_editing(&mut self, slot: Option<usize>) {
        self.chrome.set_editing(&mut self.ws, slot);
    }

    /// Set one part of the colour being edited, and use the result as the tool's colour.
    pub fn set_channel(&mut self, channel: Channel, value: f32) {
        self.chrome.set_channel(&mut self.ws, channel, value);
    }
    /// Switch the tool. Whatever was selected is let go (and any typing finished), the
    /// laser starts clean, and the panel (which belongs to the tool) goes away. The choice is
    /// remembered.
    pub(super) fn set_tool(&mut self, tool: Tool) {
        if self.ws.tool != tool {
            self.settle();
            self.ws.tool = tool;
            self.ws.selection.clear();
            self.tools.leave_all(&mut self.ws);
            self.chrome.panel = None;
            self.ws.requests.push(Request::SettingsChanged);
        }
        self.ws.redraw = true;
    }

    /// Pressing a button. A tool's button selects the tool and opens its panel (the laser has
    /// none), and pressing it again puts the panel away.
    pub(super) fn activate(&mut self, item: Item) {
        if item != Item::Theme {
            self.close_theme_menu();
        }
        if let Item::Fold(pill) = item {
            // (A round button only unfolds or folds its pill: the panel and what is being typed are left as they are.)
            self.toggle_pill(pill);
            return;
        }
        if let Item::Tool(tool) = item {
            // (The panel with the settings is the bar's: the tool's button only chooses the tool.)
            self.set_tool(tool);
            self.chrome.panel = None;
        } else {
            self.chrome.panel = None;
            // Undo and redo take back the typing first; anything else keeps it.
            if !matches!(item, Item::Undo | Item::Redo) {
                self.settle();
            }
            match item {
                Item::Undo => self.undo(),
                Item::Redo => self.redo(),
                Item::Save => self.ws.requests.push(Request::Save),
                Item::Fullscreen => self.ws.requests.push(Request::Fullscreen),
                // The other mode (the posture decides again when the machine is turned into a tablet or back).
                Item::ModeSwitch => self.set_mode(if self.mode == Mode::Desktop { Mode::Tablet } else { Mode::Desktop }),
                // (The look goes round, and its card drops down: whether the pages follow it.)
                Item::Theme => {
                    self.cycle_appearance();
                    self.open_theme_menu();
                }
                Item::Close => self.ws.requests.push(Request::Close),
                Item::AddPage => {
                    let page = self.ws.view.current_page(&self.ws.layout);
                    self.insert_page_after(page);
                }
                Item::Pages => self.open_overview(),
                Item::Search => self.toggle_search(),
                Item::Open => self.ws.requests.push(Request::Open),
                // The last page there is stays (a document has at least one: the edit is refused).
                Item::DeletePage => {
                    self.delete_current_page();
                }
                Item::Tool(_) | Item::Fold(_) => {}
            }
        }
        self.pill_after(item);
        self.ws.redraw = true;
    }
}
