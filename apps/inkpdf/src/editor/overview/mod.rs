//! The pages overview, from the editor's side: opening and closing it, what a press, a drag, the wheel and the keys do
//! in it, and the state it keeps (how far it is scrolled, which entries of the table of contents are folded, which pages
//! are chosen in edit mode, a page being dragged). While it is open every device talks to it and to nothing else, as with
//! the prompt: the pen does not draw, a finger scrolls the pages and does not pan the document, and no shortcut of the
//! page fires. (How it looks and where everything is: see `toolbar::overview`.)
//!
//! Whatever changes the pages is a [`PageEdit`], done by the editor (see `pages`) like every other change to them, so
//! it is a step of the history and undoes with the rest.

use std::collections::{BTreeSet, HashSet};

use crate::input::{Device, Phase, PointerEvent, Wheel};
use crate::motion::{Curve, Slides, Tween, seconds};
use crate::outline;
use crate::pages::{MAX_PAGES, PageEdit, Remap};
use crate::toolbar::{
    self, DragView, OverviewAction as Action, OverviewControl as Control, OverviewHover as Hover, OverviewLayout, OverviewMenuItem as MenuItem,
    OverviewView, TocHit, TocRow, Ui,
};
use crate::tools::Key;

use super::Editor;

mod actions;
mod input;

/// How far a pointer must move before a press is a drag, in logical pixels.
const DRAG_PX: f32 = 6.0;
/// How long the pages and the table of contents take to settle where the wheel or a key sent them (the time in which
/// they cover nine tenths of the way), in seconds.
const GLIDE_SECONDS: f32 = 0.11;
/// What one notch of a wheel scrolls, in units of the overview's interface.
const WHEEL_UNITS: f32 = 2.4;
/// How near the edge of the pages a dragged page is held to scroll them, and how fast, in units.
const EDGE_UNITS: f32 = 1.2;
const EDGE_SPEED_UNITS: f32 = 0.45;

/// A press under way on the overview.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    dev: Device,
    id: u32,
    /// What it began on.
    on: Option<Hover>,
    /// Whether it began in the table of contents (so that dragging scrolls that).
    in_toc: bool,
    last: [f32; 2],
    start: [f32; 2],
    /// It has moved far enough to be a drag, not a tap.
    dragged: bool,
}

/// Pages being dragged to another place.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    pages: Vec<usize>,
    pos: [f32; 2],
    /// Where they would land if dropped now: before this page.
    slot: usize,
}

/// The overview's own state, while it is open.
#[derive(Default)]
pub struct OverviewState {
    /// How far the pages are scrolled, and the table of contents, in pixels.
    pub scroll: f32,
    pub toc_scroll: f32,
    /// Where a wheel turn or a key sent them, which they glide to; `None` when they are where they were put.
    goal: Option<f32>,
    toc_goal: Option<f32>,
    /// How much of the way into view the overview is, 0 to 1.
    appear: Tween,
    /// Pages that were moved are away from where they belong, and go there; new ones (and ticks just made) are still
    /// coming, which is the part of them still to come (see [`crate::toolbar::PageMotion`]). By the page's number now.
    slides: Slides<usize>,
    grows: Slides<usize>,
    ticks: Slides<usize>,
    /// The pages that have a tick, to see which are new.
    ticked: BTreeSet<usize>,
    /// The entries of the table of contents that are folded closed, by their index in the outline.
    folded: HashSet<usize>,
    /// The rows of the table that are shown, made when first wanted and again when an entry is folded or unfolded (an
    /// outline of thousands of entries is not walked for every move of the pointer).
    rows: std::cell::OnceCell<Vec<TocRow>>,
    /// Edit mode: a tap chooses a page instead of going to it, and there is a bar of what to do to the chosen pages.
    pub edit: bool,
    /// The pages chosen, in edit mode.
    pub selected: BTreeSet<usize>,
    hover: Option<Hover>,
    press: Option<Press>,
    drag: Option<Drag>,
    /// The page whose menu is open.
    menu: Option<usize>,
}

impl OverviewState {
    /// The page whose menu is open, for tests.
    #[cfg(test)]
    pub fn menu_for_tests(&self) -> Option<usize> {
        self.menu
    }

    /// Whether a page is being carried, for tests.
    #[cfg(test)]
    pub fn dragging_for_tests(&self) -> bool {
        self.drag.is_some()
    }

    /// How far a page is from where it belongs, and how much of it and of its tick is still to come, for tests.
    #[cfg(test)]
    pub fn slides_for_tests(&self, page: usize) -> [f32; 2] {
        self.slides.offset(page)
    }

    #[cfg(test)]
    pub fn grows_for_tests(&self, page: usize) -> f32 {
        self.grows.offset(page)[0]
    }

    #[cfg(test)]
    pub fn ticks_for_tests(&self, page: usize) -> f32 {
        self.ticks.offset(page)[0]
    }

    /// What the pointer is over, for tests.
    #[cfg(test)]
    pub fn hover_for_tests(&self) -> Option<Hover> {
        self.hover
    }
}

impl Editor {
    /// Whether the pages overview is up.
    pub fn overview_open(&self) -> bool {
        self.chrome.overview.is_some()
    }

    /// Put the overview up, scrolled to the page being looked at. Whatever was under way stops (what is typed is kept).
    pub fn open_overview(&mut self) {
        if self.chrome.overview.is_some() {
            return;
        }
        self.stop_what_was_under_way();
        self.ws.erasing.clear();
        self.mouse_press = None;
        self.fingers = crate::input::TouchGestures::default();
        self.end_pull();
        self.chrome.act = super::Act::Idle;
        self.chrome.panel = None;
        self.chrome.hover = None;
        let prefs = self.motions.prefs();
        let mut state = OverviewState::default();
        state.appear.go(1.0, prefs.time(seconds::MEDIUM), Curve::Decelerate);
        self.motions.overview_back();
        let current = self.ws.view.current_page(&self.ws.layout);
        let layout = self.overview_layout_for(false);
        state.scroll = layout.show(current, self.ws.layout.len(), 0.0);
        self.chrome.overview = Some(state);
        self.show_section_in_toc();
        self.ws.redraw = true;
    }

    pub fn close_overview(&mut self) {
        if self.chrome.overview.take().is_some() {
            // (It goes out over a moment, drawn as it was: see `Motions::overview_leaving`.)
            self.motions.overview_left();
            self.ws.redraw = true;
        }
    }

    pub(super) fn overview_layout_for(&self, edit: bool) -> OverviewLayout {
        OverviewLayout::new(self.ws.view.viewport, self.ws.dpr, !self.outline.is_empty(), edit)
    }

    pub(crate) fn overview_layout(&self) -> OverviewLayout {
        self.overview_layout_for(self.chrome.overview.as_ref().is_some_and(|o| o.edit))
    }

    /// The rows of the table of contents that are shown, with the entries folded away left out.
    pub(crate) fn toc_rows(&self) -> std::borrow::Cow<'_, [TocRow]> {
        match &self.chrome.overview {
            Some(state) => std::borrow::Cow::Borrowed(state.rows.get_or_init(|| self.build_rows(&state.folded)).as_slice()),
            None => std::borrow::Cow::Owned(self.build_rows(&HashSet::new())),
        }
    }

    pub(super) fn build_rows(&self, folded: &HashSet<usize>) -> Vec<TocRow> {
        outline::visible(&self.outline, &|i| folded.contains(&i))
            .into_iter()
            .map(|item| TocRow { item, depth: self.outline[item].depth, foldable: self.outline[item].has_children })
            .collect()
    }

    /// Scroll the table of contents so that the entry that holds the page being looked at is in view.
    pub(super) fn show_section_in_toc(&mut self) {
        let Some(section) = outline::section_of(&self.outline, self.ws.view.current_page(&self.ws.layout)) else { return };
        let (count, found) = {
            let rows = self.toc_rows();
            (rows.len(), rows.iter().position(|r| r.item == section))
        };
        let layout = self.overview_layout();
        let Some(k) = found else { return };
        if let Some(state) = &mut self.chrome.overview {
            let row = layout.toc_row(k, state.toc_scroll);
            if row[1] < layout.toc_rows[1] || row[3] > layout.toc_rows[3] {
                let wanted = state.toc_scroll + row[1] - layout.toc_rows[1] - (layout.toc_rows[3] - layout.toc_rows[1]) / 3.0;
                state.toc_goal = None;
                state.toc_scroll = layout.clamp_toc_scroll(wanted, count);
            }
        }
    }

    /// One frame of the overview's own motion: the pages and the table of contents glide to where they were sent, and it
    /// comes into view. Whether it is still moving.
    pub(in crate::editor) fn step_overview(&mut self, dt: f32) -> bool {
        let time = self.motions.prefs().time(seconds::SHORT);
        let Some(state) = &mut self.chrome.overview else { return false };
        // The share of the way covered in this step, for a motion that covers nine tenths in `seconds`.
        let share = |seconds: f32| 1.0 - (-dt.max(0.0) * std::f32::consts::LN_10 / seconds).exp();
        let mut moving = false;
        for (at, goal) in [(&mut state.scroll, &mut state.goal), (&mut state.toc_scroll, &mut state.toc_goal)] {
            if let Some(to) = *goal {
                *at += (to - *at) * share(GLIDE_SECONDS);
                if (to - *at).abs() < 0.5 {
                    *at = to;
                    *goal = None;
                } else {
                    moving = true;
                }
            }
        }
        moving |= state.appear.step(dt);
        // A page newly chosen gets its tick popping in; one let go of loses it.
        for &page in &state.selected {
            if state.ticked.insert(page) {
                state.ticks.start(page, [1.0, 0.0], time);
            }
        }
        let chosen = &state.selected;
        state.ticked.retain(|p| chosen.contains(p));
        moving |= state.slides.step(dt);
        moving |= state.grows.step(dt);
        moving |= state.ticks.step(dt);
        if moving {
            self.ws.redraw = true;
        }
        moving
    }

    /// How much of the way into view the overview is, 0 to 1.
    pub fn overview_appear(&self) -> f32 {
        self.chrome.overview.as_ref().map_or(1.0, |o| o.appear.value())
    }

    /// How much of the overview that has just been put away is still there, 0 to 1: it goes out over a moment, and the
    /// shell draws it as it was meanwhile.
    pub fn overview_leaving(&self) -> f32 {
        if self.chrome.overview.is_some() { 0.0 } else { self.motions.overview_leaving() }
    }

    /// Which of the bar's actions can be done now, in the order of [`Action::ALL`].
    pub(in crate::editor) fn overview_enabled(&self) -> [bool; 5] {
        let Some(state) = &self.chrome.overview else { return [false; 5] };
        let (count, chosen) = (self.ws.layout.len(), state.selected.len());
        let first = state.selected.first().copied().unwrap_or(0);
        let some = chosen > 0;
        [count > 0, some && count + chosen <= MAX_PAGES, some && first > 0, some && first < count - chosen, some && chosen < count]
    }

    /// What the overview looks like now, drawn into `ui`.
    pub fn draw_overview(&self, ui: &mut Ui) {
        let Some(state) = &self.chrome.overview else { return };
        let layout = self.overview_layout();
        let rows = self.toc_rows();
        let current = self.ws.view.current_page(&self.ws.layout);
        let folded = |i: usize| state.folded.contains(&i);
        let selected: Vec<usize> = state.selected.iter().copied().collect();
        let view = OverviewView {
            layout: &self.ws.layout,
            scroll: state.scroll,
            current,
            selected: &selected,
            edit: state.edit,
            can_undo: self.ws.ink.can_undo(),
            can_redo: self.ws.ink.can_redo(),
            outline: &self.outline,
            rows: &rows,
            toc_scroll: state.toc_scroll,
            section: outline::section_of(&self.outline, current),
            folded: &folded,
            hover: state.hover,
            pressed: state.press.filter(|p| !p.dragged).and_then(|p| p.on),
            drag: state.drag.as_ref().map(|d| DragView { pages: &d.pages, pos: d.pos, slot: d.slot }),
            enabled: self.overview_enabled(),
            menu: state.menu,
            menu_enabled: [true, true, self.ws.layout.len() < MAX_PAGES, self.ws.layout.len() > 1],
            motion: &|page| toolbar::PageMotion {
                shift: state.slides.offset(page),
                grow: state.grows.offset(page)[0],
                tick: state.ticks.offset(page)[0],
            },
        };
        toolbar::draw_overview(&view, &layout, ui);
        ui.fade(state.appear.value());
    }

    /// The pages whose pictures the overview shows now, with a row more on each side so that scrolling finds them ready.
    pub fn overview_pages(&self) -> Option<std::ops::Range<usize>> {
        let state = self.chrome.overview.as_ref()?;
        let layout = self.overview_layout();
        let count = self.ws.layout.len();
        let shown = layout.visible_pages(count, state.scroll);
        let row = layout.columns();
        Some(shown.start.saturating_sub(row)..(shown.end + row).min(count))
    }

    /// What `pos` is on.
    pub(super) fn overview_hit(&self, pos: [f32; 2]) -> Option<Hover> {
        let state = self.chrome.overview.as_ref()?;
        let layout = self.overview_layout();
        // With a menu open, only it is pressed: what is outside it is nothing (and a press there puts it away).
        if let Some(page) = state.menu {
            return layout.menu_item_at(pos, page, state.scroll).map(Hover::Menu);
        }
        if let Some(control) = layout.control_at(pos) {
            return Some(Hover::Control(control));
        }
        if let Some(action) = layout.action_at(pos) {
            return Some(Hover::Action(action));
        }
        if layout.toc.is_some_and(|t| crate::ink::rect_contains(t, pos))
            && let Some(hit) = layout.toc_hit(pos, &self.toc_rows(), state.toc_scroll)
        {
            return Some(Hover::Toc(hit));
        }
        let page = layout.page_at(pos, self.ws.layout.len(), state.scroll)?;
        Some(if crate::ink::rect_contains(layout.number_row(page, state.scroll), pos) { Hover::Number(page) } else { Hover::Page(page) })
    }
}
