//! The bar under the tools: what the plain tip of the pen (or the left mouse button) does with the tool in use,
//! what can be done to what is selected, and how it looks. It is how a device with no eraser end, no barrel
//! button and no keyboard selects, erases, deletes, copies and restyles: every one of those is a button here,
//! and a keyboard or a stylus's own buttons are only shortcuts for them. Like the toolbar it is icons and rounded
//! panels. (It is at the top, not the bottom, because the bottom of the window is where a hand holding a stylus
//! rests.)
//!
//! Left to right: draw / select / erase, then duplicate, copy, paste, delete; the buttons that edit the point
//! of a shape (take it away, add one, smooth or sharpen it) when a shape is being edited; the five colours (a tap
//! gives the tool, and what is selected, that colour; a tap on the one in use opens its editor); and last the
//! settings button, which unfolds the tool's panel (thickness, smoothing, the shape) from the bar itself. On a
//! window too narrow for five colours they are one dot, which opens the panel.

use crate::figure::NodeAction;
use crate::ink::{MarkupKind, rect_contains};
use crate::tool::{MAX_ACTIONS, ToolAction};

use super::Tool;
use super::Toolbar;
use super::icons::icons;
use super::theme::Ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextItem {
    Duplicate,
    Copy,
    Paste,
    Delete,
    /// An action of the tool in use (what it is editing decides which): see [`crate::tools::Tool::actions`].
    Action(ToolAction),
    /// One of the colours: the tool (and what is selected) takes it.
    Swatch(usize),
    /// Unfold the panel with the thickness and the tool's other settings. (On a narrow window this is the one dot
    /// that stands for the colours.)
    Style,
}

/// Everything the bar's look depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextState {
    pub tool: Tool,
    /// Something is selected.
    pub selection: bool,
    /// The clipboard holds something the tool can paste.
    pub paste: bool,
    /// A point of the shape being edited is selected (so it can be taken away or smoothed).
    /// The tool's own actions on offer, each with whether it can be pressed now.
    pub actions: [Option<(ToolAction, bool)>; MAX_ACTIONS],
    /// The tool's panel is unfolded.
    pub style_open: bool,
    /// The colours, and the one the tool writes in.
    pub palette: [[u8; 4]; 5],
    pub colour: usize,
}

impl Default for ContextState {
    fn default() -> Self {
        Self { tool: Tool::Pen, selection: false, paste: false, actions: [None; MAX_ACTIONS], style_open: false, palette: super::PALETTE, colour: 0 }
    }
}

/// How far under the toolbar the bar is, and its padding, in interface pixels.
pub(super) const CONTEXT_GAP: f32 = 8.0;
pub(super) const CONTEXT_PAD: f32 = 8.0;
/// A colour's button is this much of a button wide.
const SWATCH_WIDTH: f32 = 0.74;
/// The least a button is, in interface pixels, for the colours to be shown one by one; narrower and they are one dot.
const LEAST_UNIT_WITH_COLOURS: f32 = 30.0;
/// Buttons at most.
const BUTTONS: usize = 16;
/// Slots at most: the buttons and the gaps between the groups.
const SLOTS: usize = 20;
/// Gaps between groups at most.
const GAPS: usize = 4;

/// What goes in the bar, left to right, and how many slots that is; `None` is a gap with a hairline in it. The tool's
/// own actions come after the selection's, if it offers any, and the colours are one dot if `compact`.
fn slots(actions: &[ToolAction], compact: bool) -> ([Option<ContextItem>; SLOTS], usize) {
    use ContextItem::{Action, Copy, Delete, Duplicate, Paste, Style, Swatch};
    // The text selection's bar has no pen to choose, nothing to duplicate or paste, no colours: copy, delete, and its marks.
    if actions.iter().any(|a| matches!(a, ToolAction::Mark(_))) {
        let mut out = [None; SLOTS];
        let all = [Some(Copy), Some(Delete), None].into_iter().chain(actions.iter().take(MAX_ACTIONS).map(|a| Some(Action(*a))));
        let mut len = 0;
        for slot in all {
            out[len] = slot;
            len += 1;
        }
        return (out, len);
    }
    let selection = [Some(Duplicate), Some(Copy), Some(Paste), Some(Delete), None];
    let colours = [Some(Swatch(0)), Some(Swatch(1)), Some(Swatch(2)), Some(Swatch(3)), Some(Swatch(4)), None];
    let look = [Some(Style)];
    let mut out = [None; SLOTS];
    let mut len = 0;
    let own = actions.iter().take(MAX_ACTIONS).map(|a| Some(Action(*a))).chain((!actions.is_empty()).then_some(None));
    let colours = colours.iter().filter(|_| !compact).copied();
    for slot in selection.iter().copied().chain(own).chain(colours).chain(look) {
        out[len] = slot;
        len += 1;
    }
    (out, len)
}

/// How many buttons wide a slot is (a gap is not counted: it is a fixed width).
fn weight(slot: &Option<ContextItem>) -> f32 {
    match slot {
        Some(ContextItem::Swatch(_)) => SWATCH_WIDTH,
        Some(_) => 1.0,
        None => 0.0,
    }
}

#[derive(Clone, Copy)]
pub struct ContextBar {
    /// `[x0, y0, x1, y1]` in pixels.
    pub rect: [f32; 4],
    /// Every button and where it is, in a fixed array so that laying the bar out (which happens on every pen and
    /// mouse event) allocates nothing.
    items: [(ContextItem, [f32; 4]); BUTTONS],
    count: usize,
    /// The gaps between groups, as the x of their middles.
    gaps: [f32; GAPS],
    gap_count: usize,
    /// The colours are one dot, the window being too narrow for five.
    compact: bool,
    unit: f32,
    dpr: f32,
}

impl ContextBar {
    /// Just under the tools of `toolbar` (out of the way of a resting palm, which is what the bottom of the window
    /// is not), centred like them, in a viewport of `viewport` pixels; with the tool's own `actions`.
    pub fn below(toolbar: &Toolbar, viewport: [f32; 2], actions: &[ToolAction]) -> ContextBar {
        let dpr = toolbar.dpr;
        let (gap, pad, margin) = (10.0 * dpr, CONTEXT_PAD * dpr, 12.0 * dpr);
        // The size of a button that makes `slots` just fit the window.
        let fit = |(all, len): ([Option<ContextItem>; SLOTS], usize)| {
            let slots = &all[..len];
            let gaps = slots.iter().filter(|s| s.is_none()).count();
            (viewport[0] - 2.0 * margin - 2.0 * pad - gaps as f32 * gap) / slots.iter().map(weight).sum::<f32>()
        };
        let compact = fit(slots(actions, false)) < LEAST_UNIT_WITH_COLOURS * dpr;
        let (all, len) = slots(actions, compact);
        // The toolbar's buttons, shrunk to fit narrow windows, never below something still tappable.
        let least = (if compact { 22.0 } else { LEAST_UNIT_WITH_COLOURS }) * dpr;
        let unit = toolbar.unit.max(least).min(fit((all, len)).max(least));
        let slots = &all[..len];
        let gaps = slots.iter().filter(|s| s.is_none()).count();
        let width = slots.iter().map(weight).sum::<f32>() * unit + gaps as f32 * gap + 2.0 * pad;
        let height = unit + 2.0 * pad;
        let left = ((viewport[0] - width) / 2.0).max(0.0);
        let top = toolbar.bottom() + CONTEXT_GAP * dpr;
        let mut items = [(ContextItem::Style, [0.0; 4]); BUTTONS];
        let mut gap_x = [0.0; GAPS];
        let (mut placed, mut placed_gaps) = (0, 0);
        let mut x = left + pad;
        for slot in slots {
            match slot {
                Some(item) => {
                    let w = weight(slot) * unit;
                    items[placed] = (*item, [x, top + pad, x + w, top + pad + unit]);
                    placed += 1;
                    x += w;
                }
                None => {
                    if placed_gaps < gap_x.len() {
                        gap_x[placed_gaps] = x + gap / 2.0;
                        placed_gaps += 1;
                    }
                    x += gap;
                }
            }
        }
        ContextBar { rect: [left, top, left + width, top + height], items, count: placed, gaps: gap_x, gap_count: placed_gaps, compact, unit, dpr }
    }

    pub(crate) fn items(&self) -> &[(ContextItem, [f32; 4])] {
        &self.items[..self.count]
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.rect, p)
    }

    pub fn hit(&self, p: [f32; 2]) -> Option<ContextItem> {
        self.items().iter().find(|(_, r)| rect_contains(*r, p)).map(|(i, _)| *i)
    }

    /// The card the bar and its unfolded panel make together, when the panel reaches down to `bottom`: as wide as the
    /// bar.
    pub fn card(&self, bottom: f32) -> [f32; 4] {
        [self.rect[0], self.rect[1], self.rect[2], bottom.max(self.rect[3])]
    }

    /// The middle of a button, for tests that press it.
    #[cfg(test)]
    pub fn centre_of(&self, item: ContextItem) -> [f32; 2] {
        let r = self.items().iter().find(|(i, _)| *i == item).map_or([0.0; 4], |(_, r)| *r);
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    }

    /// Whether a button can be pressed now.
    pub fn enabled(item: ContextItem, state: &ContextState) -> bool {
        match item {
            ContextItem::Duplicate | ContextItem::Copy | ContextItem::Delete => state.selection,
            ContextItem::Paste => state.paste,
            ContextItem::Action(a) => state.actions.iter().flatten().any(|(own, on)| *own == a && *on),
            ContextItem::Swatch(_) | ContextItem::Style => true,
        }
    }

    /// Draw the bar. With the panel unfolded (`drawer_bottom`, where it ends) the bar and the panel are one card.
    pub fn draw(&self, state: &ContextState, drawer_bottom: Option<f32>, ui: &mut Ui) {
        let (u, px) = (self.unit, self.dpr);
        let (top, bottom) = (self.rect[1], self.rect[3]);
        match drawer_bottom {
            Some(end) => {
                ui.floating(self.card(end), 0.45 * u, px);
                ui.line([self.rect[0] + 0.5 * u, bottom], [self.rect[2] - 0.5 * u, bottom], px.max(1.0), ui.pal.separator);
            }
            None => ui.floating(self.rect, 0.5 * (bottom - top), px),
        }
        let mid = (top + bottom) / 2.0;
        for &x in &self.gaps[..self.gap_count] {
            ui.line([x, mid - 0.32 * u], [x, mid + 0.32 * u], px.max(1.0), ui.pal.separator);
        }
        let all = icons();
        let line = (u * 0.04).max(1.4 * px);
        let half = 0.33 * u;
        for &(item, r) in self.items() {
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            let on = match item {
                ContextItem::Style => state.style_open,
                _ => false,
            };
            if on {
                ui.panel([cx - 0.46 * u, cy - 0.46 * u, cx + 0.46 * u, cy + 0.46 * u], 0.3 * u, ui.pal.accent_wash);
            }
            let enabled = Self::enabled(item, state);
            let colour = if on {
                ui.pal.accent
            } else if enabled {
                ui.pal.ink
            } else {
                ui.pal.ink_dim
            };
            let icon = match item {
                ContextItem::Duplicate => &all.duplicate,
                ContextItem::Copy => &all.copy,
                ContextItem::Paste => &all.paste,
                ContextItem::Delete => &all.trash,
                ContextItem::Action(ToolAction::Node(NodeAction::Remove)) => &all.node_remove,
                ContextItem::Action(ToolAction::Node(NodeAction::Add)) => &all.node_add,
                ContextItem::Action(ToolAction::Node(NodeAction::Smooth)) => &all.node_smooth,
                ContextItem::Action(ToolAction::Mark(kind)) => match kind {
                    MarkupKind::Highlight => &all.highlight,
                    MarkupKind::Underline => &all.underline,
                    MarkupKind::StrikeOut | MarkupKind::Squiggly => &all.strikeout,
                },
                ContextItem::Swatch(slot) => {
                    // A colour: the one in use is ringed, as in the panel.
                    let colour = state.palette[slot % state.palette.len()];
                    if slot == state.colour {
                        ui.disc([cx, cy], 0.74 * u, ui.pal.accent);
                        ui.disc([cx, cy], 0.64 * u, ui.pal.white);
                        ui.disc([cx, cy], 0.54 * u, colour);
                    } else {
                        ui.disc([cx, cy], 0.56 * u, ui.pal.rim);
                        ui.disc([cx, cy], 0.5 * u, colour);
                    }
                    continue;
                }
                ContextItem::Style if self.compact => {
                    // The colour in use, as a dot in a ring: the panel with the colours opens from it.
                    ui.disc([cx, cy], 0.78 * u, if on { ui.pal.accent } else { ui.pal.rim });
                    ui.disc([cx, cy], 0.66 * u, ui.pal.white);
                    ui.disc([cx, cy], 0.54 * u, state.palette[state.colour % state.palette.len()]);
                    continue;
                }
                ContextItem::Style => &all.options,
            };
            icon.draw(&mut ui.lines, cx, cy, half, line, colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [(f32, f32); 6] = [(360.0, 1.0), (480.0, 1.0), (800.0, 1.0), (1100.0, 1.5), (1650.0, 1.5), (3840.0, 2.0)];

    const NODE_ACTIONS: [ToolAction; 3] =
        [ToolAction::Node(NodeAction::Remove), ToolAction::Node(NodeAction::Add), ToolAction::Node(NodeAction::Smooth)];

    fn bar_at(w: f32, dpr: f32, nodes: bool) -> (Toolbar, ContextBar) {
        let toolbar = Toolbar::layout([w, 900.0], dpr);
        let bar = ContextBar::below(&toolbar, [w, 900.0], if nodes { &NODE_ACTIONS } else { &[] });
        (toolbar, bar)
    }

    #[test]
    fn every_button_is_hit_at_its_centre_and_the_node_buttons_come_only_with_a_shape() {
        for nodes in [false, true] {
            let (_, bar) = bar_at(1650.0, 1.5, nodes);
            let mut seen = Vec::new();
            for &(item, r) in bar.items() {
                let c = [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
                assert_eq!(bar.hit(c), Some(item));
                assert!(bar.contains(c));
                seen.push(item);
            }
            let node_buttons = seen.iter().filter(|i| matches!(i, ContextItem::Action(_))).count();
            assert_eq!(node_buttons, if nodes { 3 } else { 0 });
            assert_eq!(seen.len(), if nodes { 13 } else { 10 });
            assert_eq!(seen.first(), Some(&ContextItem::Duplicate));
            assert_eq!(seen.last(), Some(&ContextItem::Style));
            let colours: Vec<&ContextItem> = seen.iter().filter(|i| matches!(i, ContextItem::Swatch(_))).collect();
            assert_eq!(colours.len(), 5, "the five colours, one by one");
        }
    }

    #[test]
    fn a_window_too_narrow_for_five_colours_has_one_dot_for_them() {
        for (w, dpr) in WINDOWS {
            for nodes in [false, true] {
                let (_, bar) = bar_at(w, dpr, nodes);
                let colours = bar.items().iter().filter(|(i, _)| matches!(i, ContextItem::Swatch(_))).count();
                assert!(colours == 5 || (colours == 0 && bar.compact), "{w}: {colours} colours");
                // Whichever it is, the colours can be got at (a dot opens the panel, which has them all).
                assert!(bar.items().iter().any(|(i, _)| *i == ContextItem::Style), "{w}");
            }
        }
        assert!(!bar_at(1650.0, 1.5, false).1.compact);
        assert!(bar_at(360.0, 1.0, true).1.compact);
    }

    #[test]
    fn the_bar_is_centred_just_under_the_tools_and_fits_any_reasonable_window() {
        for (w, dpr) in WINDOWS {
            for nodes in [false, true] {
                let (toolbar, bar) = bar_at(w, dpr, nodes);
                let [x0, y0, x1, y1] = bar.rect;
                assert!(((x0 + x1) / 2.0 - w / 2.0).abs() < 1.0, "not centred at {w}");
                assert!(x0 >= 0.0 && x1 <= w + 0.5, "off the sides at {w} ({nodes}): {:?}", bar.rect);
                assert!(
                    y0 > toolbar.bottom() && y0 < toolbar.bottom() + 12.0 * dpr && y1 < 150.0 * dpr,
                    "not just under the tools at {w}: {:?}",
                    bar.rect
                );
                for &(item, r) in bar.items() {
                    assert!(r[2] - r[0] >= 20.0 * dpr - 0.5 && r[3] - r[1] >= 20.0 * dpr - 0.5, "{item:?} too small to press at {w}");
                }
            }
        }
    }

    #[test]
    fn the_card_the_bar_and_its_panel_make_is_as_wide_as_the_bar_and_reaches_the_panels_end() {
        let (_, bar) = bar_at(1650.0, 1.5, false);
        let card = bar.card(500.0);
        assert_eq!([card[0], card[1], card[2], card[3]], [bar.rect[0], bar.rect[1], bar.rect[2], 500.0]);
        assert_eq!(bar.card(0.0)[3], bar.rect[3], "never shorter than the bar");
    }

    #[test]
    fn a_press_between_the_buttons_or_outside_the_bar_hits_nothing() {
        let (_, bar) = bar_at(1650.0, 1.5, false);
        assert_eq!(bar.hit([5.0, 5.0]), None);
        assert!(!bar.contains([5.0, 5.0]));
        let (a, b) = (bar.items()[3].1, bar.items()[4].1);
        // The gap after the four buttons for what is selected.
        assert_eq!(bar.hit([(a[2] + b[0]) / 2.0, (a[1] + a[3]) / 2.0]), None);
    }

    #[test]
    fn what_needs_a_selection_or_a_clipboard_is_dim_without_one() {
        let none = ContextState::default();
        for item in [ContextItem::Duplicate, ContextItem::Copy, ContextItem::Delete] {
            assert!(!ContextBar::enabled(item, &none), "{item:?}");
            assert!(ContextBar::enabled(item, &ContextState { selection: true, ..none }), "{item:?}");
        }
        assert!(!ContextBar::enabled(ContextItem::Paste, &none));
        assert!(ContextBar::enabled(ContextItem::Paste, &ContextState { paste: true, ..none }));
        // A tool's own action is dim unless the tool says it can be done now.
        let remove = ContextItem::Action(ToolAction::Node(NodeAction::Remove));
        let add = ContextItem::Action(ToolAction::Node(NodeAction::Add));
        assert!(!ContextBar::enabled(remove, &none) && !ContextBar::enabled(add, &none));
        let offered = ContextState { actions: [Some((NODE_ACTIONS[0], false)), Some((NODE_ACTIONS[1], true)), None], ..none };
        assert!(!ContextBar::enabled(remove, &offered) && ContextBar::enabled(add, &offered));
        assert!((0..5).all(|slot| ContextBar::enabled(ContextItem::Swatch(slot), &none)));
    }

    #[test]
    fn the_colour_in_use_and_the_unfolded_panel_change_what_is_drawn() {
        let (_, bar) = bar_at(1650.0, 1.5, true);
        let count = |state: &ContextState, drawer: Option<f32>| {
            let mut ui = Ui::default();
            bar.draw(state, drawer, &mut ui);
            (ui.lines.len(), ui.panels.len())
        };
        let plain = count(&ContextState::default(), None);
        assert!(plain.0 > 0 && plain.1 > 0);
        let busy = ContextState {
            selection: true,
            paste: true,
            actions: [Some((NODE_ACTIONS[0], true)), None, None],
            style_open: true,
            colour: 3,
            ..ContextState::default()
        };
        assert_ne!(plain, count(&busy, None), "a selection and a pasteable clipboard draw differently");
        // Unfolded, the card and a hairline take the place of the pill.
        let open = count(&ContextState::default(), Some(bar.rect[3] + 300.0));
        assert_eq!(open.0, plain.0 + 1, "one hairline more");
    }
}
