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
mod prompt;
mod theme;
mod tool_panel;

#[cfg(test)]
mod tests;

use crate::ink::rect_contains;
use crate::input::Mode;
#[cfg(test)]
use crate::tool::{MAX_WIDTH, MIN_WIDTH};
use crate::tool::{PALETTE, Tool, width_range};

pub use context_bar::{ContextBar, ContextItem, ContextState};
use icons::icons;
pub use prompt::{PromptLayout, PromptView};
pub use theme::Ui;
use theme::{ACCENT, ACCENT_WASH, HOVER_WASH, INK, INK_DIM, RIM, SEPARATOR};
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
    Tool(Tool),
    /// Switch between the desktop and the tablet mode (and show which it is).
    ModeSwitch,
    /// Fill the whole screen, or give it back.
    Fullscreen,
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
    pub tool: Tool,
    /// The colours the pen and the shape tool write in, shown on their buttons.
    pub pen_colour: [u8; 4],
    pub shape_colour: [u8; 4],
    pub text_colour: [u8; 4],
    /// The button the mouse is over.
    pub hover: Option<Item>,
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
            tool: Tool::Pen,
            pen_colour: PALETTE[0],
            shape_colour: PALETTE[1],
            text_colour: PALETTE[0],
            hover: None,
        }
    }
}

/// What goes in each pill, left to right; `None` is a gap with a hairline in it.
const LEFT: [Option<Item>; 8] =
    [Some(Item::Close), None, Some(Item::Undo), Some(Item::Redo), Some(Item::Save), None, Some(Item::DeletePage), Some(Item::AddPage)];
const MIDDLE: [Option<Item>; 4] =
    [Some(Item::Tool(Tool::Pen)), Some(Item::Tool(Tool::Geometry)), Some(Item::Tool(Tool::Text)), Some(Item::Tool(Tool::Laser))];
const RIGHT: [Option<Item>; 2] = [Some(Item::ModeSwitch), Some(Item::Fullscreen)];

/// Buttons in all.
const BUTTONS: usize = 12;

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
        // Shrink to fit narrow windows, never below something still tappable: the left pill (six
        // buttons and two gaps) and half the middle one must fit in half the window.
        let unit = (38.0 * dpr).min(((viewport[0] / 2.0 - margin - 3.0 * pad - 3.0 * gap) / 8.0).max(20.0 * dpr));
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

    pub fn draw(&self, state: &State, ui: &mut Ui) {
        let u = self.unit;
        let px = self.dpr;
        for pill in self.pills {
            ui.floating(pill, 0.5 * (pill[3] - pill[1]), px);
        }
        let icons = icons();
        let line = (u * 0.04).max(1.4 * px);
        let mut last_end: Option<(usize, f32)> = None;
        for &(item, r) in &self.items {
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            let pill = self.pills.iter().position(|&p| rect_contains(p, [cx, cy])).unwrap_or(0);
            if let Some((was, end)) = last_end
                && was == pill
                && r[0] - end > 1.0
            {
                let x = end + (r[0] - end) / 2.0;
                ui.line([x, cy - 0.32 * u], [x, cy + 0.32 * u], px.max(1.0), SEPARATOR);
            }
            last_end = Some((pill, r[2]));
            let active = item == Item::Tool(state.tool);
            let enabled = match item {
                Item::Undo => state.can_undo,
                Item::Redo => state.can_redo,
                Item::DeletePage => state.can_delete,
                _ => true,
            };
            // The wash behind a button: blue for the tool in use, grey under the mouse.
            let wash = if active {
                Some(ACCENT_WASH)
            } else if state.hover == Some(item) && enabled {
                Some(HOVER_WASH)
            } else {
                None
            };
            if let Some(colour) = wash {
                ui.panel([cx - 0.46 * u, cy - 0.46 * u, cx + 0.46 * u, cy + 0.46 * u], 0.3 * u, colour);
            }
            let half = 0.33 * u;
            let ink = |on: bool| if on { INK } else { INK_DIM };
            match item {
                Item::Undo => icons.undo.draw(&mut ui.lines, cx, cy, half, line, ink(state.can_undo)),
                Item::Redo => icons.redo.draw(&mut ui.lines, cx, cy, half, line, ink(state.can_redo)),
                Item::Save => icons.save.draw(&mut ui.lines, cx, cy, half, line, if state.unsaved { ACCENT } else { INK }),
                Item::Close => icons.close.draw(&mut ui.lines, cx, cy, half, line, INK),
                Item::DeletePage => icons.trash.draw(&mut ui.lines, cx, cy, half, line, ink(state.can_delete)),
                Item::AddPage => icons.plus.draw(&mut ui.lines, cx, cy, half, line, INK),
                // The mode in use, drawn as the machine it is for; the tablet in the accent colour, as the one you chose
                // to leave the desk for.
                Item::ModeSwitch => match state.mode {
                    Mode::Desktop => icons.desktop.draw(&mut ui.lines, cx, cy, half, line, INK),
                    Mode::Tablet => icons.tablet.draw(&mut ui.lines, cx, cy, half, line, ACCENT),
                },
                Item::Fullscreen => {
                    let icon = if state.fullscreen { &icons.exit_fullscreen } else { &icons.fullscreen };
                    icon.draw(&mut ui.lines, cx, cy, half, line, INK);
                }
                Item::Tool(tool @ (Tool::Pen | Tool::Geometry | Tool::Text)) => {
                    let (icon, colour) = match tool {
                        Tool::Pen => (&icons.pen, state.pen_colour),
                        Tool::Geometry => (&icons.geometry, state.shape_colour),
                        _ => (&icons.text, state.text_colour),
                    };
                    icon.draw(&mut ui.lines, cx, cy, half * 1.05, line, if active { ACCENT } else { INK });
                    // The colour the tool writes in, as a dot on its corner.
                    let dot = [r[2] - 0.2 * u, r[3] - 0.2 * u];
                    ui.disc(dot, 0.3 * u, [255, 255, 255, 255]);
                    ui.disc(dot, 0.24 * u, RIM);
                    ui.disc(dot, 0.2 * u, colour);
                }
                Item::Tool(Tool::Laser) => icons.laser.draw(&mut ui.lines, cx, cy, half * 1.05, line, if active { ACCENT } else { INK }),
            }
        }
    }
}
