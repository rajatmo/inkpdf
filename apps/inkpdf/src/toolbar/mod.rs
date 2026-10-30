//! The floating toolbars: layout, hit testing and drawing. Everything is rounded panels and
//! round-ended line segments, so it needs no text or image rendering; the icons are TikZ drawings
//! (see [`icons`]).
//!
//! There are three pills along the top, as in Notewise: the document's buttons on the left (close,
//! undo, redo, save), the tools in the middle (pen, laser) and full screen on the right. The pen's
//! colour, thickness and smoothing are in a panel that opens under the pen button
//! (see [`PenPanel`]).

mod context_bar;
mod icons;
mod look;
mod overview;
mod prompt;
mod save_as;
mod theme;
mod tool_panel;

#[cfg(test)]
mod tests;

use crate::ink::rect_contains;
use crate::input::Mode;
use crate::settings::Appearance;
#[cfg(test)]
use crate::tool::{MAX_WIDTH, MIN_WIDTH};
use crate::tool::{PALETTE, Tool, width_range};

pub use context_bar::{ContextBar, ContextItem, ContextState};
use icons::icons;
pub use look::Look;
pub use overview::{
    Action as OverviewAction, Control as OverviewControl, DragView, Hover as OverviewHover, MenuItem as OverviewMenuItem, OverviewLayout,
    OverviewView, PageMotion, TocHit, TocRow, draw as draw_overview,
};
pub use prompt::{PromptLayout, PromptView};
pub use save_as::{ListingView, SaveAsHit, SaveAsLayout, SaveAsView};
pub use theme::{Align, Image, Label, Ui};
#[cfg(test)]
pub use tool_panel::ToolPanel;
pub use tool_panel::{Channel, PanelState, PanelView, Slider};

/// A width in `range` for a position `t` (0 to 1) along the slider. Widths span a decade or two, so
/// the slider is logarithmic: the same drag makes the same proportional change at any thickness.
pub fn width_in(range: (f32, f32), t: f32) -> f32 {
    range.0 * (range.1 / range.0).powf(t.clamp(0.0, 1.0))
}

/// Where along the slider a width sits, 0 to 1; the inverse of [`width_in`].
pub fn position_in(range: (f32, f32), width: f32) -> f32 {
    ((width.max(range.0) / range.0).ln() / (range.1 / range.0).ln()).clamp(0.0, 1.0)
}

/// [`width_in`] for a pen.
#[cfg(test)]
pub fn width_at(t: f32) -> f32 {
    width_in((MIN_WIDTH, MAX_WIDTH), t)
}

/// [`position_in`] for a pen.
#[cfg(test)]
pub fn slider_position(width: f32) -> f32 {
    position_in((MIN_WIDTH, MAX_WIDTH), width)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// Close the document and start a blank one.
    Close,
    Undo,
    Redo,
    Save,
    /// Delete the page being looked at (without asking). Not on the last page there is.
    DeletePage,
    /// Add a blank page after the one being looked at.
    AddPage,
    /// Open the pages overview.
    Pages,
    Tool(Tool),
    /// Switch between the desktop and the tablet mode (and show which it is).
    ModeSwitch,
    /// Fill the whole screen, or give it back.
    Fullscreen,
    /// The look of the interface: the system's, light, dark (each press goes to the next).
    Theme,
    /// Whether the pages go dark when the look is dark.
    DarkPages,
}

/// Everything the toolbars' look depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub can_undo: bool,
    pub can_redo: bool,
    /// There is more than one page, so one can go.
    pub can_delete: bool,
    pub unsaved: bool,
    pub fullscreen: bool,
    /// The mode the devices are used in, shown on its button.
    pub mode: Mode,
    /// The look the user chose, whether the interface is dark now, and whether the pages follow it.
    pub appearance: Appearance,
    pub dark: bool,
    pub dark_pages: bool,
    pub tool: Tool,
    /// The colours the pen and the shape tool write in, shown on their buttons.
    pub pen_colour: [u8; 4],
    pub shape_colour: [u8; 4],
    pub text_colour: [u8; 4],
    /// The button the mouse is over.
    pub hover: Option<Item>,
    /// The button being pressed.
    pub pressed: Option<Item>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            can_undo: false,
            can_redo: false,
            can_delete: false,
            unsaved: false,
            fullscreen: false,
            mode: Mode::default(),
            appearance: Appearance::default(),
            dark: false,
            dark_pages: false,
            tool: Tool::Pen,
            pen_colour: PALETTE[0],
            shape_colour: PALETTE[1],
            text_colour: PALETTE[0],
            hover: None,
            pressed: None,
        }
    }
}

/// What goes in each pill, left to right; `None` is a gap with a hairline in it.
const LEFT: [Option<Item>; 9] = [
    Some(Item::Close),
    None,
    Some(Item::Undo),
    Some(Item::Redo),
    Some(Item::Save),
    None,
    Some(Item::DeletePage),
    Some(Item::AddPage),
    Some(Item::Pages),
];
const MIDDLE: [Option<Item>; 5] = [
    Some(Item::Tool(Tool::Pen)),
    Some(Item::Tool(Tool::Geometry)),
    Some(Item::Tool(Tool::Text)),
    Some(Item::Tool(Tool::Select)),
    Some(Item::Tool(Tool::Laser)),
];
const RIGHT: [Option<Item>; 4] = [Some(Item::Theme), Some(Item::DarkPages), Some(Item::ModeSwitch), Some(Item::Fullscreen)];

/// Buttons in all.
pub const BUTTONS: usize = 16;

pub struct Toolbar {
    /// The three pills, `[x0, y0, x1, y1]` in pixels: left, middle, right.
    pub pills: [[f32; 4]; 3],
    /// Every button and where it is; a fixed array, so laying the toolbar out (which happens on
    /// every pen and mouse event near it) allocates nothing.
    items: [(Item, [f32; 4]); BUTTONS],
    unit: f32,
    dpr: f32,
}

impl Toolbar {
    /// Along the top of a viewport of `viewport` pixels, sized for `dpr`.
    pub fn layout(viewport: [f32; 2], dpr: f32) -> Toolbar {
        let (gap, pad, margin) = (10.0 * dpr, 8.0 * dpr, 12.0 * dpr);
        // Shrink to fit narrow windows, never below something still tappable: the left pill (seven
        // buttons and two gaps) and half the middle one must fit in half the window.
        let unit = (38.0 * dpr).min(((viewport[0] / 2.0 - margin - 3.0 * pad - 3.0 * gap) / 9.5).max(17.0 * dpr));
        let height = unit + 2.0 * pad;
        let width_of = |slots: &[Option<Item>]| slots.iter().map(|s| if s.is_some() { unit } else { gap }).sum::<f32>() + 2.0 * pad;
        let (left_w, middle_w, right_w) = (width_of(&LEFT), width_of(&MIDDLE), width_of(&RIGHT));
        let lefts = [margin, (viewport[0] - middle_w) / 2.0, (viewport[0] - margin - right_w).max(0.0)];
        let widths = [left_w, middle_w, right_w];
        let mut items = [(Item::Close, [0.0; 4]); BUTTONS];
        let mut placed = 0;
        let mut pills = [[0.0; 4]; 3];
        for (i, slots) in [&LEFT[..], &MIDDLE[..], &RIGHT[..]].into_iter().enumerate() {
            let mut x = lefts[i] + pad;
            for slot in slots {
                match slot {
                    Some(item) => {
                        items[placed] = (*item, [x, margin + pad, x + unit, margin + pad + unit]);
                        placed += 1;
                        x += unit;
                    }
                    None => x += gap,
                }
            }
            pills[i] = [lefts[i], margin, lefts[i] + widths[i], margin + height];
        }
        debug_assert_eq!(placed, BUTTONS, "every button is placed");
        Toolbar { pills, items, unit, dpr }
    }

    /// The bottom edge of the pills.
    pub fn bottom(&self) -> f32 {
        self.pills[0][3]
    }

    /// How much room the bar under the tools takes (its gap and its height), which a panel unfolds below.
    fn context_drop(&self) -> f32 {
        (context_bar::CONTEXT_GAP + 2.0 * context_bar::CONTEXT_PAD) * self.dpr + self.unit
    }

    /// Where `item` is among the buttons (what a [`Look`] is indexed by).
    pub fn slot(&self, item: Item) -> Option<usize> {
        self.items.iter().position(|(i, _)| *i == item)
    }

    #[cfg(test)]
    fn button(&self, item: Item) -> Option<[f32; 4]> {
        self.items.iter().find(|(i, _)| *i == item).map(|(_, r)| *r)
    }

    /// The middle of a button, for tests that press it.
    #[cfg(test)]
    pub fn centre_of(&self, item: Item) -> [f32; 2] {
        let r = self.button(item).unwrap_or([0.0; 4]);
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        self.pills.iter().any(|&r| rect_contains(r, p))
    }

    pub fn hit(&self, p: [f32; 2]) -> Option<Item> {
        self.items.iter().find(|(_, r)| rect_contains(*r, p)).map(|(i, _)| *i)
    }

    /// Drawn as it rests: nothing moving.
    #[cfg(test)]
    pub fn draw(&self, state: &State, ui: &mut Ui) {
        self.draw_with(state, &Look::resting(state, self), ui);
    }

    pub fn draw_with(&self, state: &State, look: &Look, ui: &mut Ui) {
        let u = self.unit;
        let px = self.dpr;
        for pill in self.pills {
            ui.floating(pill, 0.5 * (pill[3] - pill[1]), px);
        }
        let icons = icons();
        let line = (u * 0.04).max(1.4 * px);
        // The mark of the tool in use: one wash that slides from button to button.
        if let Some(rect) = self.mark_of_tool(look.tool) {
            let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
            ui.panel([cx - 0.46 * u, cy - 0.46 * u, cx + 0.46 * u, cy + 0.46 * u], 0.3 * u, ui.pal.accent_wash);
        }
        let mut last_end: Option<(usize, f32)> = None;
        for (slot, &(item, r)) in self.items.iter().enumerate() {
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            let pill = self.pills.iter().position(|&p| rect_contains(p, [cx, cy])).unwrap_or(0);
            if let Some((was, end)) = last_end
                && was == pill
                && r[0] - end > 1.0
            {
                let x = end + (r[0] - end) / 2.0;
                ui.line([x, cy - 0.32 * u], [x, cy + 0.32 * u], px.max(1.0), ui.pal.separator);
            }
            last_end = Some((pill, r[2]));
            // The grey wash of the pointer being over a button, as lit as the look says; a press spreading from its middle.
            let square = [cx - 0.46 * u, cy - 0.46 * u, cx + 0.46 * u, cy + 0.46 * u];
            if look.hover[slot] > 0.0 {
                ui.panel(
                    square,
                    0.3 * u,
                    look::mix([ui.pal.hover_wash[0], ui.pal.hover_wash[1], ui.pal.hover_wash[2], 0], ui.pal.hover_wash, look.hover[slot]),
                );
            }
            let spread = look.ripple[slot];
            if spread > 0.0 && spread < 1.0 {
                let reach = 0.46 * u * (1.0 - (1.0 - spread) * (1.0 - spread));
                let colour = look::mix(ui.pal.accent_wash, [ui.pal.accent[0], ui.pal.accent[1], ui.pal.accent[2], 0], spread);
                ui.panel([cx - reach, cy - reach, cx + reach, cy + reach], (0.3 * u).min(reach), colour);
            }
            let half = 0.33 * u;
            let ink = |amount: f32| look::mix(ui.pal.ink_dim, ui.pal.ink, amount);
            let tool_ink = |index: usize| look::mix(ui.pal.ink, ui.pal.accent, look::nearness(look.tool, index));
            match item {
                Item::Undo => icons.undo.draw(&mut ui.lines, cx, cy, half, line, ink(look.enabled[slot])),
                Item::Redo => icons.redo.draw(&mut ui.lines, cx, cy, half, line, ink(look.enabled[slot])),
                Item::Save => icons.save.draw(&mut ui.lines, cx, cy, half, line, look::mix(ui.pal.ink, ui.pal.accent, look.unsaved)),
                Item::Close => icons.close.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                Item::DeletePage => icons.trash.draw(&mut ui.lines, cx, cy, half, line, ink(look.enabled[slot])),
                Item::AddPage => icons.plus.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                Item::Pages => icons.pages.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                // The mode in use, drawn as the machine it is for; the tablet in the accent colour, as the one you chose
                // to leave the desk for.
                Item::ModeSwitch => match state.mode {
                    Mode::Desktop => icons.desktop.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                    Mode::Tablet => icons.tablet.draw(&mut ui.lines, cx, cy, half, line, ui.pal.accent),
                },
                Item::Theme => {
                    let icon = match state.appearance {
                        Appearance::System => &icons.theme_system,
                        Appearance::Light => &icons.theme_light,
                        Appearance::Dark => &icons.theme_dark,
                    };
                    icon.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink);
                }
                // In the accent colour when the pages go dark; dimmer while the look is light and it has nothing to do yet.
                Item::DarkPages => {
                    let colour = match (state.dark_pages, state.dark) {
                        (true, _) => ui.pal.accent,
                        (false, true) => ui.pal.ink,
                        (false, false) => ui.pal.ink_dim,
                    };
                    icons.dark_pages.draw(&mut ui.lines, cx, cy, half, line, colour);
                }
                Item::Fullscreen => {
                    let icon = if state.fullscreen { &icons.exit_fullscreen } else { &icons.fullscreen };
                    icon.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink);
                }
                Item::Tool(tool @ (Tool::Pen | Tool::Geometry | Tool::Text)) => {
                    let (icon, colour, index) = match tool {
                        Tool::Pen => (&icons.pen, state.pen_colour, 0),
                        Tool::Geometry => (&icons.geometry, state.shape_colour, 1),
                        _ => (&icons.text, state.text_colour, 2),
                    };
                    icon.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(index));
                    // The colour the tool writes in, as a dot on its corner.
                    let dot = [r[2] - 0.2 * u, r[3] - 0.2 * u];
                    ui.disc(dot, 0.3 * u, ui.pal.white);
                    ui.disc(dot, 0.24 * u, ui.pal.rim);
                    ui.disc(dot, 0.2 * u, colour);
                }
                Item::Tool(Tool::Select) => icons.text_select.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(3)),
                Item::Tool(Tool::Laser) => icons.laser.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(4)),
            }
        }
    }

    /// Where the mark of the tool in use is when it is `position` along the tools' buttons (between two while it slides).
    fn mark_of_tool(&self, position: f32) -> Option<[f32; 4]> {
        let rect = |index: usize| self.button_rect(Item::Tool(look::TOOLS[index.min(look::TOOLS.len() - 1)]));
        let position = position.clamp(0.0, (look::TOOLS.len() - 1) as f32);
        let (a, b) = (rect(position.floor() as usize)?, rect(position.ceil() as usize)?);
        let t = position - position.floor();
        Some([0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t))
    }

    fn button_rect(&self, item: Item) -> Option<[f32; 4]> {
        self.items.iter().find(|(i, _)| *i == item).map(|(_, r)| *r)
    }
}
