//! Where everything of the pages overview is, for a window of `viewport` pixels at scale `dpr`: the card, its header
//! with the buttons, the table of contents on the left (when the document has one) and the grid of pages. Pure
//! arithmetic, so that every window size can be tested; the grid is laid out by arithmetic too, so a document of
//! a million pages costs no more than one of three.

use std::ops::Range;

use crate::ink::rect_contains;

/// The buttons of the header, right to left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Close,
    Redo,
    Undo,
    /// The check mark: edit mode on and off.
    Edit,
    /// A blank page after the one being looked at.
    Add,
    /// The flag: show only the pages that carry one, and all of them again.
    Flagged,
}

impl Control {
    /// From the right of the header to the left.
    pub const ALL: [Control; 6] = [Control::Close, Control::Redo, Control::Undo, Control::Edit, Control::Add, Control::Flagged];
}

/// What can be done to the pages that are chosen in edit mode: the buttons of the bar under the pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Choose every page, or none if all are chosen.
    SelectAll,
    Duplicate,
    /// Move the chosen pages one place earlier, or later.
    Earlier,
    Later,
    Delete,
}

impl Action {
    /// From the left of the bar to the right.
    pub const ALL: [Action; 5] = [Action::SelectAll, Action::Duplicate, Action::Earlier, Action::Later, Action::Delete];
}

/// What the menu of one page offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// A blank page before this one, or after it.
    Before,
    After,
    Duplicate,
    /// Mark the page with a flag, or take its flag away.
    Flag,
    Delete,
}

impl MenuItem {
    /// From the left of the menu to the right.
    pub const ALL: [MenuItem; 5] = [MenuItem::Before, MenuItem::After, MenuItem::Duplicate, MenuItem::Flag, MenuItem::Delete];
}

/// A row of the table of contents as it is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TocRow {
    /// Which entry of the outline it is.
    pub item: usize,
    pub depth: usize,
    /// Whether it has entries under it, which can be folded away.
    pub foldable: bool,
}

/// What a press on the table of contents is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TocHit {
    /// The row's title: go to its page.
    Title(usize),
    /// The row's arrow: fold or unfold it.
    Arrow(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct OverviewLayout {
    /// `[x0, y0, x1, y1]`.
    pub card: [f32; 4],
    pub header: [f32; 4],
    /// Each header button, with where it is.
    pub controls: [(Control, [f32; 4]); 6],
    /// The table of contents' column, if there is one.
    pub toc: Option<[f32; 4]>,
    /// The part of the card the pages are in.
    pub grid: [f32; 4],
    /// The bar of actions under the pages, in edit mode, and each button of it.
    pub bar: Option<[f32; 4]>,
    pub actions: [(Action, [f32; 4]); 5],
    /// The table of contents' rows' area: under its heading.
    pub toc_rows: [f32; 4],
    /// The window, in pixels.
    pub viewport: [f32; 2],
    unit: f32,
    px: f32,
    columns: usize,
    cell: [f32; 2],
    thumb: [f32; 2],
    pad: f32,
}

/// Room round the pages in the grid, in units.
const GRID_PAD: f32 = 0.8;
const THUMB_W: f32 = 3.0;
const THUMB_H: f32 = 4.2;
const LABEL_H: f32 = 1.0;
const GAP_X: f32 = 0.7;
const GAP_Y: f32 = 0.5;
const ROW_H: f32 = 1.0;
const TOC_HEAD: f32 = 1.2;

impl OverviewLayout {
    pub fn new(viewport: [f32; 2], dpr: f32, has_toc: bool, edit: bool) -> Self {
        let px = dpr.max(0.25);
        // A comfortable size, shrunk to fit a narrow window.
        let nominal = 40.0 * px;
        // (The header's six buttons and the card's margin need about twelve units across.)
        let fit = ((viewport[0] / nominal - 1.0) / 12.0).clamp(0.2, 1.0);
        let u = nominal * fit;
        let width = (viewport[0] - 1.0 * u).clamp(1.0, 30.0 * u);
        let height = (viewport[1] - 2.0 * u).max(1.0);
        let x0 = ((viewport[0] - width) / 2.0).max(0.0);
        let y0 = (1.0 * u).min((viewport[1] - height).max(0.0));
        let card = [x0, y0, x0 + width, y0 + height];
        let header = [card[0], card[1], card[2], card[1] + 2.2 * u];
        let button = 1.5 * u;
        let mut controls = [(Control::Close, [0.0; 4]); 6];
        let mut right = header[2] - 0.5 * u;
        for (i, control) in Control::ALL.into_iter().enumerate() {
            let top = header[1] + (header[3] - header[1] - button) / 2.0;
            controls[i] = (control, [right - button, top, right, top + button]);
            right -= button + 0.2 * u;
        }
        let toc_width = if has_toc && width >= 13.0 * u { (width * 0.28).clamp(4.5 * u, 8.0 * u) } else { 0.0 };
        let toc = (toc_width > 0.0).then_some([card[0], header[3], card[0] + toc_width, card[3]]);
        let mut grid = [card[0] + toc_width, header[3], card[2], card[3]];
        // In edit mode a bar of what can be done to the chosen pages takes the bottom of the card.
        let mut actions = [(Action::SelectAll, [0.0; 4]); 5];
        let bar = edit.then(|| {
            let bar = [grid[0], (grid[3] - 2.4 * u).max(grid[1]), grid[2], grid[3]];
            let (button, gap) = (1.5 * u, 0.3 * u);
            let total = button * Action::ALL.len() as f32 + gap * (Action::ALL.len() - 1) as f32;
            let mut x = (bar[0] + bar[2] - total) / 2.0;
            let top = bar[1] + (bar[3] - bar[1] - button) / 2.0;
            for (i, action) in Action::ALL.into_iter().enumerate() {
                actions[i] = (action, [x, top, x + button, top + button]);
                x += button + gap;
            }
            grid[3] = bar[1];
            bar
        });
        let toc_rows = toc.map_or([0.0; 4], |t| [t[0], t[1] + TOC_HEAD * u, t[2], t[3]]);
        let (thumb, cell) = ([THUMB_W * u, THUMB_H * u], [THUMB_W * u + GAP_X * u, (THUMB_H + LABEL_H + GAP_Y) * u]);
        let usable = (grid[2] - grid[0] - 2.0 * GRID_PAD * u + GAP_X * u).max(cell[0]);
        let columns = ((usable / cell[0]).floor() as usize).max(1);
        Self { card, header, controls, toc, grid, bar, actions, toc_rows, viewport, unit: u, px, columns, cell, thumb, pad: GRID_PAD * u }
    }

    /// The size of one interface unit, in pixels.
    pub fn unit(&self) -> f32 {
        self.unit
    }

    /// One interface pixel at this scale.
    pub fn px(&self) -> f32 {
        self.px
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    #[cfg(test)]
    pub fn button(&self, control: Control) -> [f32; 4] {
        self.controls.iter().find(|(c, _)| *c == control).map_or([0.0; 4], |(_, r)| *r)
    }

    pub fn control_at(&self, p: [f32; 2]) -> Option<Control> {
        self.controls.iter().find(|(_, r)| rect_contains(*r, p)).map(|(c, _)| *c)
    }

    pub fn action_at(&self, p: [f32; 2]) -> Option<Action> {
        self.bar?;
        self.actions.iter().find(|(_, r)| rect_contains(*r, p)).map(|(a, _)| *a)
    }

    pub fn action(&self, action: Action) -> [f32; 4] {
        self.actions.iter().find(|(a, _)| *a == action).map_or([0.0; 4], |(_, r)| *r)
    }

    pub fn in_card(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    // ── the pages ────────────────────────────────────────────────────────────────────────────

    fn rows(&self, count: usize) -> usize {
        count.div_ceil(self.columns)
    }

    /// How far the grid can be scrolled for `count` pages, in pixels.
    pub fn max_scroll(&self, count: usize) -> f32 {
        let content = self.rows(count) as f32 * self.cell[1] + 2.0 * self.pad - self.cell_gap_y();
        (content - (self.grid[3] - self.grid[1])).max(0.0)
    }

    fn cell_gap_y(&self) -> f32 {
        GAP_Y * self.unit
    }

    /// `scroll` brought back into what there is to scroll to.
    pub fn clamp_scroll(&self, scroll: f32, count: usize) -> f32 {
        if scroll.is_finite() { scroll.clamp(0.0, self.max_scroll(count)) } else { 0.0 }
    }

    /// The pages whose rows are, at least partly, in the grid.
    pub fn visible_pages(&self, count: usize, scroll: f32) -> Range<usize> {
        let height = self.grid[3] - self.grid[1];
        let first_row = ((scroll - self.pad) / self.cell[1]).floor().max(0.0) as usize;
        let end_row = (((scroll + height - self.pad) / self.cell[1]).ceil().max(0.0) as usize).max(first_row);
        (first_row * self.columns).min(count)..(end_row * self.columns).min(count)
    }

    /// Page `i`'s cell: the picture's box and the number under it.
    pub fn cell(&self, i: usize, scroll: f32) -> [f32; 4] {
        let (col, row) = (i % self.columns, i / self.columns);
        let x = self.grid[0] + self.pad + col as f32 * self.cell[0];
        let y = self.grid[1] + self.pad + row as f32 * self.cell[1] - scroll;
        [x, y, x + self.thumb[0], y + self.thumb[1] + LABEL_H * self.unit]
    }

    /// Where page `i`'s picture goes: its shape, as big as the box allows, in the middle of it across and resting on the bottom of it, so that
    /// its number is right under it whatever the shape (a wide page is not left floating above a gap).
    pub fn thumb(&self, i: usize, scroll: f32, page: [f32; 2]) -> [f32; 4] {
        let cell = self.cell(i, scroll);
        let aspect = if page[0] > 0.0 && page[1] > 0.0 { page[0] / page[1] } else { 0.77 };
        let (mut w, mut h) = (self.thumb[0], self.thumb[0] / aspect);
        if h > self.thumb[1] {
            (w, h) = (self.thumb[1] * aspect, self.thumb[1]);
        }
        let (cx, bottom) = ((cell[0] + cell[2]) / 2.0, cell[1] + self.thumb[1]);
        [cx - w / 2.0, bottom - h, cx + w / 2.0, bottom]
    }

    /// The row of the cell under page `i`'s picture where its number goes: `[x0, y0, x1, y1]`.
    pub fn number_row(&self, i: usize, scroll: f32) -> [f32; 4] {
        let cell = self.cell(i, scroll);
        [cell[0], cell[1] + self.thumb[1], cell[2], cell[3]]
    }

    /// The page whose cell `p` is in, if it is in the grid.
    pub fn page_at(&self, p: [f32; 2], count: usize, scroll: f32) -> Option<usize> {
        if !rect_contains(self.grid, p) {
            return None;
        }
        let (dx, dy) = (p[0] - self.grid[0] - self.pad, p[1] - self.grid[1] - self.pad + scroll);
        if dx < 0.0 || dy < 0.0 {
            return None;
        }
        let (col, row) = ((dx / self.cell[0]).floor() as usize, (dy / self.cell[1]).floor() as usize);
        let i = row.checked_mul(self.columns)?.checked_add(col)?;
        (col < self.columns && i < count && rect_contains(self.cell(i, scroll), p)).then_some(i)
    }

    /// The place among the pages nearest to `p`, for dropping a page that is being dragged: 0 to `count`, where `k`
    /// is the place before page `k`.
    pub fn slot_at(&self, p: [f32; 2], count: usize, scroll: f32) -> usize {
        let dy = (p[1] - self.grid[1] - self.pad + scroll).max(0.0);
        let row = ((dy / self.cell[1]).floor() as usize).min(self.rows(count).saturating_sub(1));
        let dx = (p[0] - self.grid[0] - self.pad).max(0.0);
        // Before the column whose middle is the next one to the right of `p`.
        let col = (((dx + self.cell[0] / 2.0 - GAP_X * self.unit / 2.0) / self.cell[0]).floor() as usize).min(self.columns);
        (row * self.columns + col).min(count)
    }

    /// The menu of page `page` and where each of its items is: a row of round buttons under the page's number, or over it if there
    /// is no room under, and always inside the card.
    pub fn menu(&self, page: usize, scroll: f32) -> ([f32; 4], [(MenuItem, [f32; 4]); 5]) {
        let u = self.unit;
        let (item, gap, pad) = (1.5 * u, 0.2 * u, 0.3 * u);
        let n = MenuItem::ALL.len() as f32;
        let width = n * item + (n - 1.0) * gap + 2.0 * pad;
        let height = item + 2.0 * pad;
        let row = self.number_row(page, scroll);
        let centre = (row[0] + row[2]) / 2.0;
        let x0 = (centre - width / 2.0).clamp(self.card[0] + 0.4 * u, (self.card[2] - 0.4 * u - width).max(self.card[0] + 0.4 * u));
        let below = row[3] + 0.1 * u;
        let wanted = if below + height <= self.card[3] - 0.4 * u { below } else { row[1] - 0.1 * u - height };
        let y0 = wanted.min(self.card[3] - 0.4 * u - height).max(self.card[1] + 0.4 * u);
        let rect = [x0, y0, x0 + width, y0 + height];
        let mut items = [(MenuItem::Before, [0.0; 4]); 5];
        for (k, which) in MenuItem::ALL.into_iter().enumerate() {
            let left = x0 + pad + k as f32 * (item + gap);
            items[k] = (which, [left, y0 + pad, left + item, y0 + pad + item]);
        }
        (rect, items)
    }

    pub fn menu_item_at(&self, p: [f32; 2], page: usize, scroll: f32) -> Option<MenuItem> {
        self.menu(page, scroll).1.iter().find(|(_, r)| rect_contains(*r, p)).map(|(m, _)| *m)
    }

    /// The mark that shows where a dragged page would land if it were dropped at `slot` (see [`OverviewLayout::slot_at`]):
    /// a thin bar in the gap before that page, or after the last page for the slot at the end.
    pub fn slot_marker(&self, slot: usize, count: usize, scroll: f32) -> [f32; 4] {
        let (slot, after) = if slot >= count && count > 0 { (count - 1, true) } else { (slot, false) };
        let cell = self.cell(slot, scroll);
        let x = if after { cell[2] + GAP_X * self.unit / 2.0 } else { cell[0] - GAP_X * self.unit / 2.0 };
        let half = 0.12 * self.unit;
        [x - half, cell[1], x + half, cell[1] + self.thumb[1]]
    }

    /// The scroll that shows page `page`: unchanged if it already is, else the page's row about a third of the way
    /// down the grid.
    pub fn show(&self, page: usize, count: usize, scroll: f32) -> f32 {
        let height = self.grid[3] - self.grid[1];
        let top = self.pad + (page / self.columns) as f32 * self.cell[1];
        let bottom = top + self.cell[1] - self.cell_gap_y();
        let kept = if top >= scroll && bottom <= scroll + height { scroll } else { top - height / 3.0 };
        self.clamp_scroll(kept, count)
    }

    // ── the table of contents ────────────────────────────────────────────────────────────────

    /// The top of row `k` of the table of contents.
    fn row_top(&self, k: usize, scroll: f32) -> f32 {
        self.toc_rows[1] + 0.3 * self.unit + k as f32 * ROW_H * self.unit - scroll
    }

    /// Row `k`'s box, when the table is scrolled by `scroll`.
    pub fn toc_row(&self, k: usize, scroll: f32) -> [f32; 4] {
        let top = self.row_top(k, scroll);
        [self.toc_rows[0] + 0.3 * self.unit, top, self.toc_rows[2] - 0.3 * self.unit, top + ROW_H * self.unit]
    }

    /// How far the table can be scrolled for `rows` rows.
    pub fn toc_max_scroll(&self, rows: usize) -> f32 {
        let content = rows as f32 * ROW_H * self.unit + 0.6 * self.unit;
        (content - (self.toc_rows[3] - self.toc_rows[1])).max(0.0)
    }

    pub fn clamp_toc_scroll(&self, scroll: f32, rows: usize) -> f32 {
        if scroll.is_finite() { scroll.clamp(0.0, self.toc_max_scroll(rows)) } else { 0.0 }
    }

    /// The rows of the table that are, at least partly, in view.
    pub fn toc_visible(&self, rows: usize, scroll: f32) -> Range<usize> {
        let height = self.toc_rows[3] - self.toc_rows[1];
        let first = ((scroll - 0.3 * self.unit) / (ROW_H * self.unit)).floor().max(0.0) as usize;
        let end = (((scroll + height - 0.3 * self.unit) / (ROW_H * self.unit)).ceil().max(0.0) as usize).max(first);
        first.min(rows)..end.min(rows)
    }

    /// Where the arrow of a foldable row is (the title starts after it, or at the same place on a row with none).
    pub fn toc_arrow(&self, row: TocRow, k: usize, scroll: f32) -> [f32; 4] {
        let r = self.toc_row(k, scroll);
        let x = r[0] + row.depth as f32 * 0.6 * self.unit;
        [x, r[1], x + 0.9 * self.unit, r[3]]
    }

    /// Where a row's title starts.
    pub fn toc_title_x(&self, row: TocRow, k: usize, scroll: f32) -> f32 {
        self.toc_arrow(row, k, scroll)[2] + 0.1 * self.unit
    }

    /// What `p` is on in the table of contents, if it is on a row.
    pub fn toc_hit(&self, p: [f32; 2], rows: &[TocRow], scroll: f32) -> Option<TocHit> {
        if !self.toc.is_some_and(|t| rect_contains(t, p)) || !rect_contains(self.toc_rows, p) {
            return None;
        }
        let k = ((p[1] - self.row_top(0, scroll)) / (ROW_H * self.unit)).floor();
        if k < 0.0 {
            return None;
        }
        let (k, row) = (k as usize, rows.get(k as usize)?);
        if !rect_contains(self.toc_row(k, scroll), p) {
            return None;
        }
        Some(if row.foldable && rect_contains(self.toc_arrow(*row, k, scroll), p) { TocHit::Arrow(k) } else { TocHit::Title(k) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 6] = [
        ([2000.0, 1070.0], 1.0),
        ([3840.0, 2054.0], 1.5),
        ([1280.0, 720.0], 1.0),
        ([800.0, 600.0], 2.0),
        ([360.0, 640.0], 1.0),
        ([300.0, 260.0], 3.0),
    ];

    fn inside(outer: [f32; 4], inner: [f32; 4]) -> bool {
        let slack = 0.01;
        inner[0] >= outer[0] - slack && inner[1] >= outer[1] - slack && inner[2] <= outer[2] + slack && inner[3] <= outer[3] + slack
    }

    fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[2] - 0.01 && b[0] < a[2] - 0.01 && a[1] < b[3] - 0.01 && b[1] < a[3] - 0.01
    }

    #[test]
    fn everything_is_in_the_window_and_in_its_place_at_every_size() {
        for ([w, h], dpr) in WINDOWS {
            for has_toc in [false, true] {
                let l = OverviewLayout::new([w, h], dpr, has_toc, false);
                assert!(inside([0.0, 0.0, w, h], l.card), "the card is in the window at {w}x{h}: {:?}", l.card);
                assert!(inside(l.card, l.header) && inside(l.card, l.grid), "{w}x{h}");
                assert!(l.header[3] <= l.grid[1] + 0.01, "the grid is under the header");
                for (control, r) in l.controls {
                    assert!(inside(l.header, r), "{control:?} is in the header at {w}x{h}: {r:?}");
                    assert!(r.iter().all(|v| v.is_finite()));
                }
                for pair in l.controls.windows(2) {
                    assert!(!overlap(pair[0].1, pair[1].1), "{:?} and {:?} do not overlap", pair[0].0, pair[1].0);
                    assert!(pair[1].1[2] <= pair[0].1[0] + 0.01, "right to left");
                }
                if let Some(toc) = l.toc {
                    assert!(has_toc && inside(l.card, toc) && !overlap(toc, l.grid), "{w}x{h}");
                    assert!(inside(toc, l.toc_rows));
                    assert!(l.grid[2] - l.grid[0] > 3.0 * l.unit(), "there is room left for pages");
                } else {
                    assert_eq!(l.grid[0], l.card[0], "with no table of contents the pages have the whole width");
                }
                assert!(l.columns() >= 1);
            }
        }
        assert!(OverviewLayout::new([2000.0, 1070.0], 1.0, true, false).toc.is_some());
        assert!(OverviewLayout::new([360.0, 640.0], 1.0, true, false).toc.is_none(), "too narrow for a table of contents beside the pages");
    }

    #[test]
    fn the_cells_of_the_visible_pages_are_in_the_grid_and_apart_and_hit_where_they_are() {
        for ([w, h], dpr) in WINDOWS {
            let l = OverviewLayout::new([w, h], dpr, false, false);
            for count in [1, 2, 7, 40, 1000] {
                let shown = l.visible_pages(count, 0.0);
                assert_eq!(shown.start, 0);
                assert!(shown.end >= 1.min(count) && shown.end <= count);
                for i in shown.clone() {
                    let c = l.cell(i, 0.0);
                    assert!(c[0] >= l.grid[0] && c[2] <= l.grid[2], "page {i} is in the grid's width at {w}x{h}");
                    let centre = [(c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0];
                    if centre[1] < l.grid[3] {
                        assert_eq!(l.page_at(centre, count, 0.0), Some(i), "the middle of page {i}'s cell is on it");
                    }
                    for j in shown.clone().filter(|&j| j != i) {
                        assert!(!overlap(c, l.cell(j, 0.0)), "cells {i} and {j} do not overlap");
                    }
                    let t = l.thumb(i, 0.0, [612.0, 792.0]);
                    assert!(inside(c, t), "the picture is in its cell");
                }
            }
            assert_eq!(l.page_at([l.grid[0] - 5.0, l.grid[1] + 50.0], 10, 0.0), None, "left of the grid");
        }
    }

    #[test]
    fn pictures_take_the_shape_of_their_pages_and_stay_in_their_boxes() {
        let l = OverviewLayout::new([2000.0, 1070.0], 1.0, false, false);
        let cell = l.cell(0, 0.0);
        for page in [[612.0, 792.0], [1000.0, 200.0], [200.0, 1000.0], [10.0, 10.0], [0.0, 0.0], [f32::NAN, 5.0]] {
            let t = l.thumb(0, 0.0, page);
            assert!(inside(cell, t), "{page:?}: {t:?} in {cell:?}");
            assert!(t.iter().all(|v| v.is_finite()), "{page:?}");
            let aspect = (t[2] - t[0]) / (t[3] - t[1]);
            if page[0] > 0.0 && page[1] > 0.0 && page[0].is_finite() {
                assert!((aspect - page[0] / page[1]).abs() < 0.01, "{page:?} keeps its shape");
            }
        }
    }

    #[test]
    fn scrolling_covers_every_page_and_never_goes_beyond() {
        for ([w, h], dpr) in WINDOWS {
            let l = OverviewLayout::new([w, h], dpr, false, false);
            let count = 333;
            let max = l.max_scroll(count);
            assert!(max > 0.0, "{w}x{h}: 333 pages are more than a window shows");
            if l.grid[3] - l.grid[1] >= 7.0 * l.unit() {
                assert_eq!(l.max_scroll(1), 0.0, "one page needs no scrolling where the window has room for a row");
            }
            assert_eq!(l.clamp_scroll(-50.0, count), 0.0);
            assert_eq!(l.clamp_scroll(1.0e9, count), max);
            assert_eq!(l.clamp_scroll(f32::NAN, count), 0.0);
            // Scrolled to the end, the last page is in the grid; every page is in view at some scroll.
            let last = l.cell(count - 1, max);
            if l.grid[3] - l.grid[1] >= 7.0 * l.unit() {
                assert!(last[3] <= l.grid[3] + 0.5 && last[1] >= l.grid[1], "the last page's cell is in the grid at the end: {last:?} {:?}", l.grid);
            }
            let mut seen = vec![false; count];
            let mut at: f32 = 0.0;
            loop {
                for i in l.visible_pages(count, at.min(max)) {
                    seen[i] = true;
                }
                if at >= max {
                    break;
                }
                at += l.grid[3] - l.grid[1];
            }
            assert!(seen.iter().all(|s| *s), "every page is in view at some scroll");
        }
    }

    #[test]
    fn showing_a_page_scrolls_only_when_it_is_out_of_view() {
        let l = OverviewLayout::new([1280.0, 720.0], 1.0, false, false);
        let count = 500;
        assert_eq!(l.show(0, count, 0.0), 0.0, "already in view");
        let far = l.show(400, count, 0.0);
        assert!(far > 0.0);
        let c = l.cell(400, far);
        assert!(c[1] >= l.grid[1] && c[3] <= l.grid[3], "now in view");
        assert_eq!(l.show(400, count, far), far, "and once in view it does not move");
        assert_eq!(l.show(499, count, 0.0), l.max_scroll(count), "the last page cannot be brought higher than the end");
    }

    #[test]
    fn a_dragged_page_is_dropped_before_the_page_nearest_to_it() {
        let l = OverviewLayout::new([1280.0, 720.0], 1.0, false, false);
        let count = 20;
        let c0 = l.cell(0, 0.0);
        assert_eq!(l.slot_at([c0[0] + 2.0, c0[1] + 10.0], count, 0.0), 0, "at the left edge of the first page: before it");
        let c1 = l.cell(1, 0.0);
        assert_eq!(l.slot_at([c1[0] + 2.0, c1[1] + 10.0], count, 0.0), 1);
        assert_eq!(l.slot_at([c1[2] - 2.0, c1[1] + 10.0], count, 0.0), 2, "at the right edge of the second: after it");
        assert_eq!(l.slot_at([l.grid[2] + 500.0, l.grid[3] + 5000.0], count, 0.0), count, "far past the end: at the end");
        assert_eq!(l.slot_at([-100.0, -100.0], count, 0.0), 0, "far before it: in front");
    }

    #[test]
    fn the_menu_of_a_page_is_inside_the_card_under_its_number_or_over_it_and_its_items_are_found() {
        for ([w, h], dpr) in WINDOWS {
            let l = OverviewLayout::new([w, h], dpr, false, false);
            for i in l.visible_pages(40, 0.0) {
                let (rect, items) = l.menu(i, 0.0);
                assert!(inside(l.card, rect), "the menu of page {i} is in the card at {w}x{h}: {rect:?} {:?}", l.card);
                for (item, r) in items {
                    assert!(inside(rect, r), "{item:?} is in the menu");
                    assert_eq!(l.menu_item_at([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0], i, 0.0), Some(item));
                }
                for pair in items.windows(2) {
                    assert!(!overlap(pair[0].1, pair[1].1), "the items do not overlap");
                }
                assert_eq!(l.menu_item_at([rect[0] - 5.0, rect[1] - 5.0], i, 0.0), None, "outside it is nothing");
            }
        }
        // Near the bottom of the card it opens upwards.
        let l = OverviewLayout::new([1280.0, 720.0], 1.0, false, false);
        let last_row = l.visible_pages(60, 0.0).end - 1;
        let (rect, _) = l.menu(last_row, 0.0);
        let number = l.number_row(last_row, 0.0);
        assert!(rect[3] <= number[1] || rect[1] >= number[3], "it does not cover the number it belongs to");
    }

    #[test]
    fn a_million_pages_cost_what_three_do() {
        let l = OverviewLayout::new([3840.0, 2054.0], 1.5, true, false);
        let count = 1 << 20;
        let max = l.max_scroll(count);
        assert!(max.is_finite() && max > 1.0e6);
        let shown = l.visible_pages(count, max);
        assert!(shown.end == count && shown.len() < 400, "{shown:?}");
        let start = std::time::Instant::now();
        for i in shown {
            let c = l.cell(i, max);
            let middle = [(c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0];
            // (A row scrolled half out of the grid has its middle outside it, where nothing is hit.)
            if middle[1] > l.grid[1] && middle[1] < l.grid[3] {
                assert_eq!(l.page_at(middle, count, max), Some(i), "page {i} at the end of a million");
            }
        }
        assert!(start.elapsed().as_millis() < 100);
    }

    #[test]
    fn the_table_of_contents_scrolls_and_is_hit_by_row_and_by_arrow() {
        let l = OverviewLayout::new([2000.0, 1070.0], 1.0, true, false);
        let rows: Vec<TocRow> = (0..100).map(|i| TocRow { item: i, depth: i % 3, foldable: i % 2 == 0 }).collect();
        let max = l.toc_max_scroll(rows.len());
        assert!(max > 0.0);
        assert_eq!(l.toc_max_scroll(2), 0.0);
        assert_eq!(l.clamp_toc_scroll(1.0e9, rows.len()), max);
        let shown = l.toc_visible(rows.len(), 0.0);
        assert_eq!(shown.start, 0);
        for k in shown.clone() {
            let r = l.toc_row(k, 0.0);
            assert!(r[0] >= l.toc_rows[0] && r[2] <= l.toc_rows[2], "row {k} is in the column");
            let title = [r[2] - 4.0, (r[1] + r[3]) / 2.0];
            if title[1] < l.toc_rows[3] {
                assert_eq!(l.toc_hit(title, &rows, 0.0), Some(TocHit::Title(k)), "the end of row {k}");
                let arrow = l.toc_arrow(rows[k], k, 0.0);
                let on_arrow = [(arrow[0] + arrow[2]) / 2.0, (arrow[1] + arrow[3]) / 2.0];
                let expect = if rows[k].foldable { TocHit::Arrow(k) } else { TocHit::Title(k) };
                if on_arrow[1] < l.toc_rows[3] {
                    assert_eq!(l.toc_hit(on_arrow, &rows, 0.0), Some(expect), "the arrow of row {k}");
                }
            }
        }
        assert_eq!(l.toc_hit([l.grid[0] + 50.0, l.grid[1] + 50.0], &rows, 0.0), None, "the pages are not the table of contents");
        assert_eq!(l.toc_hit([l.toc_rows[0] + 20.0, l.toc_rows[1] - 3.0], &rows, 0.0), None, "nor is its heading");
        let none = OverviewLayout::new([2000.0, 1070.0], 1.0, false, false);
        assert_eq!(none.toc_hit([20.0, 300.0], &rows, 0.0), None, "with no table there is nothing to hit");
    }

    #[test]
    fn the_header_buttons_are_found_where_they_are_and_the_card_is_the_card() {
        let l = OverviewLayout::new([1280.0, 720.0], 1.0, false, false);
        for (control, r) in l.controls {
            assert_eq!(l.control_at([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]), Some(control));
        }
        assert_eq!(l.control_at([l.card[0] + 5.0, l.header[1] + 5.0]), None);
        assert!(l.in_card([(l.card[0] + l.card[2]) / 2.0, (l.card[1] + l.card[3]) / 2.0]));
        assert!(!l.in_card([l.card[0] - 3.0, 100.0]));
        assert_eq!(l.button(Control::Close), l.controls[0].1);
    }
}
