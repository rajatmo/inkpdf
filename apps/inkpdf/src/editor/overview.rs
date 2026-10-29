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
use crate::outline;
use crate::motion::{Curve, Slides, Tween, seconds};
use crate::pages::{MAX_PAGES, PageEdit, Remap};
use crate::tools::Key;
use crate::toolbar::{
    self, DragView, OverviewAction as Action, OverviewControl as Control, OverviewHover as Hover, OverviewLayout, OverviewMenuItem as MenuItem, OverviewView, TocHit,
    TocRow, Ui,
};

use super::Editor;

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

    fn overview_layout_for(&self, edit: bool) -> OverviewLayout {
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

    fn build_rows(&self, folded: &HashSet<usize>) -> Vec<TocRow> {
        outline::visible(&self.outline, &|i| folded.contains(&i))
            .into_iter()
            .map(|item| TocRow { item, depth: self.outline[item].depth, foldable: self.outline[item].has_children })
            .collect()
    }

    /// Scroll the table of contents so that the entry that holds the page being looked at is in view.
    fn show_section_in_toc(&mut self) {
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
    pub(super) fn step_overview(&mut self, dt: f32) -> bool {
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
    pub(super) fn overview_enabled(&self) -> [bool; 5] {
        let Some(state) = &self.chrome.overview else { return [false; 5] };
        let (count, chosen) = (self.ws.layout.len(), state.selected.len());
        let first = state.selected.first().copied().unwrap_or(0);
        let some = chosen > 0;
        [
            count > 0,
            some && count + chosen <= MAX_PAGES,
            some && first > 0,
            some && first < count - chosen,
            some && chosen < count,
        ]
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
            motion: &|page| toolbar::PageMotion { shift: state.slides.offset(page), grow: state.grows.offset(page)[0], tick: state.ticks.offset(page)[0] },
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
    fn overview_hit(&self, pos: [f32; 2]) -> Option<Hover> {
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

    // ── input ────────────────────────────────────────────────────────────────────────────────

    /// A pen or touch event while the overview is up.
    pub(super) fn overview_event(&mut self, ev: &PointerEvent) {
        if ev.device == Device::Touch && !self.palm.admit_touch(ev, (self.clock)(), super::touch::PALM_PX * self.ws.dpr) {
            return;
        }
        let sample = if ev.phase == Phase::Down { ev.samples.first() } else { ev.samples.last() };
        if let Some(s) = sample {
            self.overview_pointer(ev.device, ev.id, ev.phase, s.pos);
        }
    }

    /// One pointer's position and phase.
    pub(super) fn overview_pointer(&mut self, dev: Device, id: u32, phase: Phase, pos: [f32; 2]) {
        self.ws.redraw = true;
        let layout = self.overview_layout();
        let hit = self.overview_hit(pos);
        let over_toc = layout.toc.is_some_and(|t| crate::ink::rect_contains(t, pos));
        let count = self.ws.layout.len();
        let dpr = self.ws.dpr;
        let Some(state) = &mut self.chrome.overview else { return };
        let mine = state.press.is_some_and(|p| p.dev == dev && p.id == id);
        match phase {
            Phase::Down => {
                if state.press.is_none() {
                    state.press = Some(Press { dev, id, on: hit, in_toc: over_toc, last: pos, start: pos, dragged: false });
                }
                state.hover = hit;
            }
            Phase::Move | Phase::Hover => {
                state.hover = hit;
                if mine && let Some(press) = state.press.as_mut() {
                    let travelled = ((pos[0] - press.start[0]).powi(2) + (pos[1] - press.start[1]).powi(2)).sqrt();
                    if travelled > DRAG_PX * dpr {
                        press.dragged = true;
                    }
                    let dy = pos[1] - press.last[1];
                    press.last = pos;
                    let (on, in_toc, dragged) = (press.on, press.in_toc, press.dragged);
                    match on {
                        // A page pulled by a pen or a mouse is carried to another place; a finger pulls the pages past it.
                        Some(Hover::Page(i)) if dragged && dev != Device::Touch => {
                            if state.drag.is_none() {
                                let pages = if state.edit && state.selected.contains(&i) { state.selected.iter().copied().collect() } else { vec![i] };
                                state.drag = Some(Drag { pages, pos, slot: layout.slot_at(pos, count, state.scroll) });
                            }
                            self.carry_pages(pos, &layout);
                        }
                        Some(Hover::Control(_) | Hover::Action(_)) => {}
                        _ if dragged => self.scroll_overview_by(-dy, in_toc),
                        _ => {}
                    }
                }
            }
            Phase::Up => {
                if mine && let Some(press) = state.press.take() {
                    state.hover = hit;
                    if state.menu.is_some() {
                        // An item of the menu is done if it is pressed and let go; anywhere else puts the menu away.
                        let item = press.on.filter(|on| Some(*on) == hit);
                        match item {
                            Some(Hover::Menu(item)) if !press.dragged => self.do_menu_item(item),
                            _ => state.menu = None,
                        }
                    } else if state.drag.is_some() {
                        let dropped = state.drag.take();
                        if let Some(drag) = dropped {
                            self.drop_pages(&drag);
                        }
                    } else if !press.dragged {
                        match (press.on, hit) {
                            (Some(a), Some(b)) if a == b => self.overview_activate(a),
                            // A tap on the dimmed document round the card puts the overview away.
                            (None, None) if !layout.in_card(pos) && !layout.in_card(press.start) => self.close_overview(),
                            _ => {}
                        }
                    }
                }
            }
            Phase::Cancel | Phase::Leave => {
                if mine || phase == Phase::Leave {
                    state.press = None;
                    state.drag = None;
                }
                state.hover = None;
            }
        }
    }

    /// The dragged pages follow the pointer: where they would land is found, and the pages scroll when it is held near
    /// the top or the bottom of them.
    fn carry_pages(&mut self, pos: [f32; 2], layout: &OverviewLayout) {
        let (count, edge, speed) = (self.ws.layout.len(), EDGE_UNITS * layout.unit(), EDGE_SPEED_UNITS * layout.unit());
        let toward = if pos[1] < layout.grid[1] + edge {
            -((layout.grid[1] + edge - pos[1]) / edge).min(1.5)
        } else if pos[1] > layout.grid[3] - edge {
            ((pos[1] - (layout.grid[3] - edge)) / edge).min(1.5)
        } else {
            0.0
        };
        if let Some(state) = &mut self.chrome.overview {
            state.goal = None;
            state.scroll = layout.clamp_scroll(state.scroll + toward * speed, count);
            let slot = layout.slot_at(pos, count, state.scroll);
            if let Some(drag) = &mut state.drag {
                drag.pos = pos;
                drag.slot = slot;
            }
        }
    }

    /// The pages that were being dragged are dropped at the place they were carried to.
    fn drop_pages(&mut self, drag: &Drag) {
        let mut pages = drag.pages.clone();
        pages.sort_unstable();
        // `to` counts the pages that stay: the ones before the place, but not the moving ones among them.
        let to = (0..drag.slot).filter(|p| !pages.contains(p)).count();
        if self.edit_pages(PageEdit::Move { pages: pages.clone(), to }) {
            let first = to;
            self.overview_show_page(first);
        }
    }

    /// Scroll the pages (or the table of contents) by `dy` pixels.
    fn scroll_overview_by(&mut self, dy: f32, toc: bool) {
        let layout = self.overview_layout();
        let (pages, rows) = (self.ws.layout.len(), self.toc_rows().len());
        if let Some(state) = &mut self.chrome.overview {
            state.menu = None;
            if toc {
                state.toc_goal = None;
                state.toc_scroll = layout.clamp_toc_scroll(state.toc_scroll + dy, rows);
            } else {
                state.goal = None;
                state.scroll = layout.clamp_scroll(state.scroll + dy, pages);
            }
        }
        self.ws.redraw = true;
    }

    /// Send the pages (or the table of contents) `dy` pixels further than they are going: they glide there.
    fn glide_overview_by(&mut self, dy: f32, toc: bool) {
        // (With motion reduced the pages go where they are sent at once.)
        if self.motions.prefs().reduced {
            return self.scroll_overview_by(dy, toc);
        }
        let layout = self.overview_layout();
        let (pages, rows) = (self.ws.layout.len(), self.toc_rows().len());
        if let Some(state) = &mut self.chrome.overview {
            state.menu = None;
            if toc {
                let to = layout.clamp_toc_scroll(state.toc_goal.unwrap_or(state.toc_scroll) + dy, rows);
                state.toc_goal = Some(to);
            } else {
                let to = layout.clamp_scroll(state.goal.unwrap_or(state.scroll) + dy, pages);
                state.goal = Some(to);
            }
        }
        self.ws.redraw = true;
    }

    /// The wheel scrolls the pages, or the table of contents when the pointer is over it.
    pub(super) fn overview_wheel(&mut self, delta: Wheel) {
        let unit = self.overview_layout().unit();
        let dy = match delta {
            Wheel::Lines(_, y) => -y * WHEEL_UNITS * unit,
            Wheel::Pixels(_, y) => -y,
        };
        let over_toc = self.overview_layout().toc.is_some_and(|t| crate::ink::rect_contains(t, self.cursor));
        self.glide_overview_by(dy, over_toc);
    }

    /// The mouse left the window: nothing is under it.
    pub(super) fn overview_pointer_left(&mut self) {
        if let Some(state) = &mut self.chrome.overview {
            state.hover = None;
            self.ws.redraw = true;
        }
    }

    /// The window lost the keyboard focus: a button let go of elsewhere never reaches us, so a press or a drag under way
    /// is given up.
    pub(super) fn overview_gestures_lost(&mut self) {
        if let Some(state) = &mut self.chrome.overview {
            state.press = None;
            state.drag = None;
            state.hover = None;
            self.ws.redraw = true;
        }
    }

    /// A key while the overview is up. `true` if it was used.
    pub(super) fn overview_key(&mut self, key: Key<'_>) -> bool {
        let layout = self.overview_layout();
        let page = layout.grid[3] - layout.grid[1];
        match key {
            Key::Escape => {
                // Out of what is under way, a step at a time: a drag, the pages chosen, edit mode, the overview.
                match &mut self.chrome.overview {
                    Some(state) if state.menu.is_some() => state.menu = None,
                    Some(state) if state.drag.is_some() => {
                        state.drag = None;
                        state.press = None;
                    }
                    Some(state) if !state.selected.is_empty() => state.selected.clear(),
                    Some(state) if state.edit => state.edit = false,
                    _ => self.close_overview(),
                }
                self.overview_settled();
            }
            Key::Delete => {
                if !self.apply_action(Action::Delete) {
                    return false;
                }
            }
            Key::Home(_) => self.glide_overview_by(f32::MIN / 2.0, false),
            Key::End(_) => self.glide_overview_by(f32::MAX / 2.0, false),
            Key::Arrow(dir, _) => {
                let step = match dir {
                    crate::tools::Dir::Up => -0.5,
                    crate::tools::Dir::Down => 0.5,
                    _ => return false,
                };
                self.glide_overview_by(step * page, false);
            }
            _ => return false,
        }
        self.ws.redraw = true;
        true
    }

    /// A shortcut with Ctrl while the overview is up (desktop mode): `a` chooses every page, `d` duplicates the chosen ones.
    /// Undo, redo and save are the document's, and are not asked of this.
    pub(super) fn overview_shortcut(&mut self, letter: &str) -> bool {
        let done = match letter {
            "a" => {
                if let Some(state) = &mut self.chrome.overview {
                    state.edit = true;
                }
                if let Some(state) = &mut self.chrome.overview {
                    state.selected = (0..self.ws.layout.len()).collect();
                }
                true
            }
            "d" => self.apply_action(Action::Duplicate),
            _ => false,
        };
        self.overview_settled();
        self.ws.redraw = true;
        done
    }

    // ── what the buttons do ──────────────────────────────────────────────────────────────────

    fn overview_activate(&mut self, on: Hover) {
        match on {
            Hover::Control(Control::Close) => self.close_overview(),
            Hover::Control(Control::Add) => {
                let current = self.ws.view.current_page(&self.ws.layout);
                if self.insert_page_after(current) {
                    self.overview_show_page(current + 1);
                }
            }
            Hover::Control(Control::Edit) => {
                if let Some(state) = &mut self.chrome.overview {
                    state.edit = !state.edit;
                    state.selected.clear();
                }
                self.overview_settled();
            }
            Hover::Control(Control::Undo) => {
                self.undo();
                self.overview_settled();
            }
            Hover::Control(Control::Redo) => {
                self.redo();
                self.overview_settled();
            }
            Hover::Action(action) => {
                self.apply_action(action);
            }
            Hover::Number(page) => {
                if let Some(state) = &mut self.chrome.overview {
                    state.menu = if state.menu == Some(page) { None } else { Some(page) };
                }
            }
            // (A menu item is done when the menu is open, where a press is taken first: see `do_menu_item`.)
            Hover::Menu(_) => {}
            Hover::Page(page) => {
                // In edit mode a tap chooses the page, or lets go of it; otherwise it goes there.
                match &mut self.chrome.overview {
                    Some(state) if state.edit => {
                        if !state.selected.remove(&page) {
                            state.selected.insert(page);
                        }
                    }
                    _ => self.overview_go_to(page),
                }
            }
            Hover::Toc(TocHit::Title(k)) => {
                let target = self.toc_rows().get(k).and_then(|row| self.outline[row.item].page);
                if let Some(page) = target {
                    self.overview_go_to(page);
                }
            }
            Hover::Toc(TocHit::Arrow(k)) => {
                if let Some(row) = self.toc_rows().get(k).copied()
                    && let Some(state) = &mut self.chrome.overview
                {
                    if !state.folded.remove(&row.item) {
                        state.folded.insert(row.item);
                    }
                    state.rows = std::cell::OnceCell::new();
                }
            }
        }
        self.ws.redraw = true;
    }

    /// Do an item of the menu of the page it is open for, and put the menu away.
    fn do_menu_item(&mut self, item: MenuItem) {
        let Some(page) = self.chrome.overview.as_mut().and_then(|o| o.menu.take()) else { return };
        match item {
            MenuItem::Before => {
                if let Some(like) = self.ws.layout.page(page)
                    && self.edit_pages(PageEdit::Insert { at: page, size: [like.w, like.h] })
                {
                    self.overview_show_page(page);
                }
            }
            MenuItem::After => {
                if self.insert_page_after(page) {
                    self.overview_show_page(page + 1);
                }
            }
            MenuItem::Duplicate => {
                if self.edit_pages(PageEdit::Duplicate { pages: vec![page] }) {
                    self.overview_show_page(page + 1);
                }
            }
            MenuItem::Delete => {
                self.edit_pages(PageEdit::delete(page));
            }
        }
        self.ws.redraw = true;
    }

    /// Do `action` to the chosen pages (or choose them all). `true` if it did something.
    fn apply_action(&mut self, action: Action) -> bool {
        let Some(state) = &self.chrome.overview else { return false };
        let enabled = self.overview_enabled();
        let index = Action::ALL.iter().position(|a| *a == action).unwrap_or(0);
        if !enabled[index] {
            return false;
        }
        let chosen: Vec<usize> = state.selected.iter().copied().collect();
        let (count, first) = (self.ws.layout.len(), chosen.first().copied().unwrap_or(0));
        match action {
            Action::SelectAll => {
                if let Some(state) = &mut self.chrome.overview {
                    state.selected = if chosen.len() == count { BTreeSet::new() } else { (0..count).collect() };
                }
                true
            }
            Action::Delete => self.edit_pages(PageEdit::Delete { pages: chosen, grave: 0 }),
            Action::Duplicate => {
                let edit = PageEdit::Duplicate { pages: chosen };
                let copies: Vec<usize> = edit.copies(count).into_iter().map(|(page, _)| page).collect();
                let done = self.edit_pages(edit);
                if done && let Some(state) = &mut self.chrome.overview {
                    // The copies are the pages chosen now, ready to be moved.
                    state.selected = copies.iter().copied().collect();
                    if let Some(&page) = copies.first() {
                        self.overview_show_page(page);
                    }
                }
                done
            }
            Action::Earlier | Action::Later => {
                let to = if action == Action::Earlier { first - 1 } else { first + 1 };
                let done = self.edit_pages(PageEdit::Move { pages: chosen, to });
                if done {
                    self.overview_show_page(to);
                }
                done
            }
        }
    }

    /// Look at `page` in the document, and put the overview away.
    fn overview_go_to(&mut self, page: usize) {
        self.close_overview();
        self.ws.scroll_to_page(page);
    }

    /// Scroll the pages so that `page` is in view.
    fn overview_show_page(&mut self, page: usize) {
        let layout = self.overview_layout();
        let count = self.ws.layout.len();
        if let Some(state) = &mut self.chrome.overview {
            state.goal = None;
            state.scroll = layout.show(page, count, state.scroll);
        }
    }

    /// After the pages changed while the overview is up (a page added, or an undo that put pages back or took them
    /// away), or after its mode changed: what it keeps still makes sense. `map` is what the change did to the pages, if it
    /// was one: the pages chosen go where their pages went.
    pub(super) fn overview_follow(&mut self, map: Option<&Remap>, before: usize) {
        let was = self.chrome.overview.as_ref().map(|s| s.scroll);
        if let (Some(state), Some(map)) = (&mut self.chrome.overview, map) {
            state.selected = state.selected.iter().filter_map(|&p| map.page(p)).collect();
            state.ticked = state.selected.clone();
            state.ticks = Slides::default();
        }
        self.overview_settled();
        if let (Some(map), Some(was)) = (map, was) {
            self.overview_slide(map, before, was);
        }
    }

    /// The pages of a change made while the overview is up go from where their pictures were to where they are: a page
    /// that moved slides, a new one (or a copy) grows in.
    fn overview_slide(&mut self, map: &Remap, before: usize, was: f32) {
        let layout = self.overview_layout();
        let count = self.ws.layout.len();
        let prefs = self.motions.prefs();
        let Some(state) = &mut self.chrome.overview else { return };
        let now = state.scroll;
        state.slides.rekey(|page| map.page(page));
        state.grows.rekey(|page| map.page(page));
        for old in layout.visible_pages(before, was) {
            if let Some(new) = map.page(old) {
                let (from, to) = (layout.cell(old, was), layout.cell(new, now));
                state.slides.start(new, [from[0] - to[0], from[1] - to[1]], prefs.time(seconds::MEDIUM));
            }
        }
        let new_pages = map.added().iter().copied().chain(map.copies().iter().map(|c| c.page));
        for page in new_pages.filter(|&p| p < count) {
            state.grows.start(page, [1.0, 0.0], prefs.time(seconds::MEDIUM));
        }
        self.ws.redraw = true;
    }

    /// What the overview keeps is brought back into range.
    pub(super) fn overview_settled(&mut self) {
        let layout = self.overview_layout();
        let (pages, rows) = (self.ws.layout.len(), self.toc_rows().len());
        if let Some(state) = &mut self.chrome.overview {
            state.goal = state.goal.map(|g| layout.clamp_scroll(g, pages));
            state.toc_goal = state.toc_goal.map(|g| layout.clamp_toc_scroll(g, rows));
            state.scroll = layout.clamp_scroll(state.scroll, pages);
            state.toc_scroll = layout.clamp_toc_scroll(state.toc_scroll, rows);
            state.selected.retain(|&p| p < pages);
            if !state.edit {
                state.selected.clear();
            }
            state.press = None;
            state.drag = None;
            // (A menu is for a page: it goes with a change of the pages.)
            state.menu = None;
        }
    }
}
