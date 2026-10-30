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
mod search_card;
mod theme;
mod theme_card;
mod tool_panel;

#[cfg(test)]
mod tests;

use crate::ink::rect_contains;
use crate::input::Mode;
use crate::settings::Appearance;
#[cfg(test)]
use crate::tool::{MAX_WIDTH, MIN_WIDTH};
use crate::tool::{PALETTE, Tool, width_range};

/// Whether an SVG draws something and keeps inside its box (what an extension's icon must do: a test says so for each).
#[cfg(test)]
pub fn icon_is_drawable(svg: &str) -> bool {
    icons::is_drawable(svg)
}

pub use context_bar::{ContextBar, ContextItem, ContextState};
use icons::icons;
pub use look::Look;
pub use overview::{
    Action as OverviewAction, Control as OverviewControl, DragView, Hover as OverviewHover, MenuItem as OverviewMenuItem, OverviewLayout,
    OverviewView, PageMotion, TocHit, TocRow, draw as draw_overview,
};
pub use prompt::{PromptLayout, PromptView};
pub use search_card::{ROWS as SEARCH_ROWS, SearchItem, SearchLayout, SearchRow, SearchView};
pub use theme::{Align, Image, Label, Ui};
pub use theme_card::{ThemeLayout, ThemeView};
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
    /// Ask the system's picker for a PDF to open (there only where the system has one); it sits just left of Save.
    Open,
    /// Open or close the search card.
    Search,
    Tool(Tool),
    /// Switch between the desktop and the tablet mode (and show which it is).
    ModeSwitch,
    /// Fill the whole screen, or give it back.
    Fullscreen,
    /// The look of the interface: the system's, light, dark (each press goes to the next).
    Theme,
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
    /// The card under the theme button is open.
    pub theme_menu: bool,
    /// The search card is open.
    pub search: bool,
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
            theme_menu: false,
            search: false,
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
const LEFT: [Option<Item>; 10] = [
    Some(Item::Close),
    None,
    Some(Item::Undo),
    Some(Item::Redo),
    Some(Item::Save),
    None,
    Some(Item::DeletePage),
    Some(Item::AddPage),
    Some(Item::Pages),
    Some(Item::Search),
];
const RIGHT: [Option<Item>; 3] = [Some(Item::Theme), Some(Item::ModeSwitch), Some(Item::Fullscreen)];

/// Buttons at most: the left pill's, the right pill's and as many tools as a toolbar can hold. (A toolbar has the buttons of the tools there are.)
pub const BUTTONS: usize = 9 + 4 + crate::tool::MAX_TOOLS;

pub struct Toolbar {
    /// The three pills, `[x0, y0, x1, y1]` in pixels: left, middle, right.
    pub pills: [[f32; 4]; 3],
    /// Every button and where it is; a fixed array, so laying the toolbar out (which happens on
    /// every pen and mouse event near it) allocates nothing.
    items: [(Item, [f32; 4]); BUTTONS],
    /// How many of `items` are buttons.
    len: usize,
    /// The tools, in the order of their buttons, and the icon of each one an extension made (the program's own are drawn from `icons`).
    tools: [Tool; crate::tool::MAX_TOOLS],
    tool_icons: [Option<&'static str>; crate::tool::MAX_TOOLS],
    tool_count: usize,
    unit: f32,
    dpr: f32,
}

impl Toolbar {
    /// Along the top of a viewport of `viewport` pixels, sized for `dpr`, with the tools of this build.
    #[cfg(test)]
    pub fn layout(viewport: [f32; 2], dpr: f32) -> Toolbar {
        Self::layout_for(&crate::tool::reference_registry(), viewport, dpr)
    }

    /// The same, with the tools `registry` lists.
    pub fn layout_for(registry: &crate::tool::Registry, viewport: [f32; 2], dpr: f32) -> Toolbar {
        let (gap, pad, margin) = (10.0 * dpr, 8.0 * dpr, 12.0 * dpr);
        let mut tools = [Tool::Pen; crate::tool::MAX_TOOLS];
        let mut tool_icons = [None; crate::tool::MAX_TOOLS];
        let mut middle = [None; crate::tool::MAX_TOOLS];
        let tool_count = registry.len().min(crate::tool::MAX_TOOLS);
        for (i, tool) in registry.tools().take(tool_count).enumerate() {
            tools[i] = tool;
            tool_icons[i] = registry.icon(tool);
            middle[i] = Some(Item::Tool(tool));
        }
        let middle = &middle[..tool_count];
        // The left pill: the document's buttons (and one to open a PDF, where the system has a picker).
        let mut left = [None; LEFT.len() + 1];
        let mut left_len = 0;
        for slot in LEFT {
            if slot == Some(Item::Save) && registry.has_open_button() {
                left[left_len] = Some(Item::Open);
                left_len += 1;
            }
            left[left_len] = slot;
            left_len += 1;
        }
        let left = &left[..left_len];
        // Shrink to fit narrow windows, never below something still tappable: the left pill (eight
        // buttons and two gaps) and half the middle one must fit in half the window.
        let across = 8.0 + f32::from(registry.has_open_button()) + tool_count as f32 / 2.0;
        let unit = (38.0 * dpr).min(((viewport[0] / 2.0 - margin - 3.0 * pad - 3.0 * gap) / across).max(17.0 * dpr));
        let height = unit + 2.0 * pad;
        let width_of = |slots: &[Option<Item>]| slots.iter().map(|s| if s.is_some() { unit } else { gap }).sum::<f32>() + 2.0 * pad;
        let (left_w, middle_w, right_w) = (width_of(left), width_of(middle), width_of(&RIGHT));
        let lefts = [margin, (viewport[0] - middle_w) / 2.0, (viewport[0] - margin - right_w).max(0.0)];
        let widths = [left_w, middle_w, right_w];
        let mut items = [(Item::Close, [0.0; 4]); BUTTONS];
        let mut placed = 0;
        let mut pills = [[0.0; 4]; 3];
        for (i, slots) in [left, middle, &RIGHT[..]].into_iter().enumerate() {
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
        Toolbar { pills, items, len: placed, tools, tool_icons, tool_count, unit, dpr }
    }

    /// Every button and where it is.
    pub(crate) fn items(&self) -> &[(Item, [f32; 4])] {
        &self.items[..self.len]
    }

    /// The tools, in the order of their buttons.
    pub fn tools(&self) -> &[Tool] {
        &self.tools[..self.tool_count]
    }

    /// Where the tool's button is among the tools' (what the mark of the tool in use is placed by).
    pub fn tool_index(&self, tool: Tool) -> Option<usize> {
        self.tools().iter().position(|&t| t == tool)
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
        self.items().iter().position(|(i, _)| *i == item)
    }

    #[cfg(test)]
    fn button(&self, item: Item) -> Option<[f32; 4]> {
        self.items().iter().find(|(i, _)| *i == item).map(|(_, r)| *r)
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
        self.items().iter().find(|(_, r)| rect_contains(*r, p)).map(|(i, _)| *i)
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
        for (slot, &(item, r)) in self.items().iter().enumerate() {
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
            let tool_ink = |index: Option<usize>| look::mix(ui.pal.ink, ui.pal.accent, index.map_or(0.0, |i| look::nearness(look.tool, i)));
            match item {
                Item::Undo => icons.undo.draw(&mut ui.lines, cx, cy, half, line, ink(look.enabled[slot])),
                Item::Redo => icons.redo.draw(&mut ui.lines, cx, cy, half, line, ink(look.enabled[slot])),
                Item::Save => icons.save.draw(&mut ui.lines, cx, cy, half, line, look::mix(ui.pal.ink, ui.pal.accent, look.unsaved)),
                Item::Close => icons.close.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                Item::DeletePage => icons.trash.draw(&mut ui.lines, cx, cy, half, line, ink(look.enabled[slot])),
                Item::AddPage => icons.plus.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                Item::Pages => icons.pages.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                Item::Open => icons.open.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink),
                Item::Search => icons.search.draw(&mut ui.lines, cx, cy, half, line, if state.search { ui.pal.accent } else { ui.pal.ink }),
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
                    // (The accent while its card is open.)
                    icon.draw(&mut ui.lines, cx, cy, half, line, if state.theme_menu { ui.pal.accent } else { ui.pal.ink });
                }
                Item::Fullscreen => {
                    let icon = if state.fullscreen { &icons.exit_fullscreen } else { &icons.fullscreen };
                    icon.draw(&mut ui.lines, cx, cy, half, line, ui.pal.ink);
                }
                Item::Tool(tool @ (Tool::Pen | Tool::Geometry | Tool::Text)) => {
                    let (icon, colour) = match tool {
                        Tool::Pen => (&icons.pen, state.pen_colour),
                        Tool::Geometry => (&icons.geometry, state.shape_colour),
                        _ => (&icons.text, state.text_colour),
                    };
                    icon.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(self.tool_index(tool)));
                    // The colour the tool writes in, as a dot on its corner.
                    let dot = [r[2] - 0.2 * u, r[3] - 0.2 * u];
                    ui.disc(dot, 0.3 * u, ui.pal.white);
                    ui.disc(dot, 0.24 * u, ui.pal.rim);
                    ui.disc(dot, 0.2 * u, colour);
                }
                Item::Tool(Tool::Lasso) => icons.select.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(self.tool_index(Tool::Lasso))),
                Item::Tool(Tool::Eraser) => icons.erase.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(self.tool_index(Tool::Eraser))),
                Item::Tool(Tool::Select) => icons.text_select.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(self.tool_index(Tool::Select))),
                // An extension's tool: its own icon.
                Item::Tool(other) => {
                    let index = self.tool_index(other);
                    if let Some(icon) = index.and_then(|i| self.tool_icons[i]).map(icons::from_svg_cached) {
                        icon.draw(&mut ui.lines, cx, cy, half * 1.05, line, tool_ink(index));
                    }
                }
            }
        }
    }

    /// Where the mark of the tool in use is when it is `position` along the tools' buttons (between two while it slides).
    fn mark_of_tool(&self, position: f32) -> Option<[f32; 4]> {
        let last = self.tool_count.saturating_sub(1);
        let rect = |index: usize| self.button_rect(Item::Tool(self.tools[index.min(last)]));
        let position = position.clamp(0.0, last as f32);
        let (a, b) = (rect(position.floor() as usize)?, rect(position.ceil() as usize)?);
        let t = position - position.floor();
        Some([0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t))
    }

    fn button_rect(&self, item: Item) -> Option<[f32; 4]> {
        self.items().iter().find(|(i, _)| *i == item).map(|(_, r)| *r)
    }
}
