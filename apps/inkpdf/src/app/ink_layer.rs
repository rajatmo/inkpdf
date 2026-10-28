//! The ink and interface line work on the GPU: committed strokes, the overlays drawn over them
//! (selection, the stroke being drawn, lasso, eraser ring, pop-ups) and the toolbar.

use std::collections::HashSet;
use std::ops::Range;

use crate::editor::Editor;
use crate::gpu::Capsule;
use crate::ink::{InkStore, P, Stroke};
use crate::smooth::curve_through;
use crate::toolbar::{self, PanelView, PromptLayout, PromptView, Ui};
use crate::tools::Painter;
use crate::tools::paint::CURVE_STEP;

use super::State;

const ACCENT: [u8; 4] = [64, 132, 255, 255];

/// What the GPU's committed-ink buffer was built from: the ink, how many strokes it had, the
/// selection and the strokes being erased (the last two as hashes of their ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InkStamp {
    revision: u64,
    strokes: usize,
    selection: u64,
    erasing: u64,
}

impl InkStamp {
    /// Matches no real state, so the first sync always builds.
    pub const NEVER: InkStamp = InkStamp { revision: u64::MAX, strokes: 0, selection: 0, erasing: 0 };

    fn of(ed: &Editor) -> Self {
        Self {
            revision: ed.ink.revision(),
            strokes: ed.ink.strokes().len(),
            selection: hash_ids(ed.selection.iter(), 0x9E37_79B9_7F4A_7C15),
            erasing: hash_ids(ed.erasing.iter(), 0xC2B2_AE3D_27D4_EB4F),
        }
    }
}

/// How the GPU's committed-ink buffer gets from what it holds to what the ink is now.
#[derive(Debug, PartialEq, Eq)]
enum Refresh {
    Current,
    /// Only strokes drawn since (from this index on) need adding to the end.
    Append {
        from: usize,
    },
    Rebuild,
}

/// Separate ranges the buffer may collect through appends before it is rebuilt tidy (a range is a
/// draw call, so this bounds the work per frame).
const MAX_RANGES: usize = 48;

/// Finishing a stroke only adds to the end of the ink; appending it costs what the stroke costs,
/// where rebuilding costs everything on the page (many milliseconds on a crowded one).
fn plan_refresh(built: &InkStamp, now: &InkStamp, ink: &InkStore, ranges: usize) -> Refresh {
    if built == now {
        return Refresh::Current;
    }
    let hidden_unchanged = built.selection == now.selection && built.erasing == now.erasing;
    let appended = *built != InkStamp::NEVER && ink.only_appended_since(built.revision) && built.strokes <= now.strokes;
    if hidden_unchanged && appended && ranges < MAX_RANGES { Refresh::Append { from: built.strokes } } else { Refresh::Rebuild }
}

/// Add `range` of `page` to `pages`, joining the last entry when it continues it.
fn push_range(pages: &mut Vec<(u32, Range<u32>)>, page: u32, range: Range<u32>) {
    match pages.last_mut() {
        Some((p, r)) if *p == page && r.end == range.start => r.end = range.end,
        _ => pages.push((page, range)),
    }
}
/// An order-independent hash of stroke ids; `mix` tells the two sets apart.
fn hash_ids<'a>(ids: impl ExactSizeIterator<Item = &'a u64>, mix: u64) -> u64 {
    let count = ids.len() as u64;
    ids.fold(count, |h, id| h ^ id.wrapping_mul(mix))
}

/// What the toolbars, pen panel and prompt on the GPU were built from: their look, the window size
/// and the scale.
#[derive(Clone, PartialEq)]
pub(super) struct UiStamp {
    state: toolbar::State,
    viewport: [u32; 2],
    dpr: u32,
    panel: Option<PanelView>,
    prompt: Option<PromptView>,
}

/// Line work drawn over the committed ink.
#[derive(Default)]
pub(super) struct Overlays {
    /// In document space.
    pub(super) doc: Vec<Capsule>,
    /// In screen pixels.
    pub(super) screen: Vec<Capsule>,
    /// The pen clock, for what fades.
    now_ms: f64,
}

impl Painter for Overlays {
    fn doc_line(&mut self, a: P, b: P, width: f32, rgba: [u8; 4]) {
        self.doc.push(Capsule::new(a, b, width, rgba));
    }

    fn screen_line(&mut self, a: P, b: P, width: f32, rgba: [u8; 4]) {
        self.screen.push(Capsule::new(a, b, width, rgba));
    }

    fn now_ms(&self) -> f64 {
        self.now_ms
    }
}

impl State {
    /// Bring the GPU's committed-ink buffer up to date with the ink, the selection and the strokes
    /// being erased. Selected strokes are left out: they are drawn as an overlay.
    pub(super) fn sync_committed(&mut self) {
        let now = InkStamp::of(&self.ed);
        match plan_refresh(&self.ink_built, &now, &self.ed.ink, self.ink_pages.len()) {
            Refresh::Current => return,
            Refresh::Append { from } if self.append_committed(from) => {}
            Refresh::Append { .. } | Refresh::Rebuild => self.rebuild_committed(),
        }
        self.ink_built = now;
    }

    /// Add the strokes from index `from` on to the end of the buffer. `false` when they do not fit
    /// (the buffer is then rebuilt, with room to spare).
    fn append_committed(&mut self, from: usize) -> bool {
        let Some(new) = self.ed.ink.strokes().get(from..) else { return false };
        let base = self.gpu.committed_len();
        let mut capsules = Vec::new();
        let mut ranges: Vec<(u32, Range<u32>)> = Vec::new();
        for s in new {
            let start = base + capsules.len() as u32;
            stroke_capsules(s, [0.0, 0.0], &mut capsules);
            push_range(&mut ranges, s.page, start..base + capsules.len() as u32);
        }
        if !self.gpu.append_committed(&capsules) {
            return false;
        }
        for (page, range) in ranges {
            push_range(&mut self.ink_pages, page, range);
        }
        true
    }

    /// Rebuild the buffer from scratch: the visible strokes grouped by page (so a page's ink is one
    /// range), in drawing order within each page.
    fn rebuild_committed(&mut self) {
        let ed = &self.ed;
        let hidden: HashSet<u64> = ed.selection.iter().chain(&ed.erasing).copied().collect();
        let mut shown: Vec<&Stroke> = ed.ink.strokes().iter().filter(|s| !hidden.contains(&s.id)).collect();
        shown.sort_by_key(|s| s.page);
        let mut all = Vec::with_capacity(shown.len() * 32);
        self.ink_pages.clear();
        for s in shown {
            let start = all.len() as u32;
            stroke_capsules(s, [0.0, 0.0], &mut all);
            push_range(&mut self.ink_pages, s.page, start..all.len() as u32);
        }
        self.gpu.set_committed(&all);
    }
    /// The ranges of the committed-ink buffer for the pages on screen.
    pub(super) fn visible_ink_ranges(&self) -> Vec<Range<u32>> {
        let (top, bottom) = self.ed.view.visible_y();
        let pages = self.ed.layout.pages_in(top, bottom);
        self.ink_pages.iter().filter(|(p, _)| pages.contains(&(*p as usize))).map(|(_, r)| r.clone()).collect()
    }

    /// Rebuild the toolbars and pen panel on the GPU when their look has changed: a different state,
    /// window, scale or panel. Otherwise they stay as they are and cost nothing per frame.
    pub(super) fn sync_ui(&mut self) {
        let ed = &self.ed;
        let state = ed.toolbar_state();
        let [w, h] = ed.view.viewport.map(f32::to_bits);
        let stamp = UiStamp { state, viewport: [w, h], dpr: ed.dpr.to_bits(), panel: ed.panel_view(), prompt: ed.prompt };
        if self.ui_built.as_ref() == Some(&stamp) {
            return;
        }
        let mut ui = Ui::default();
        // The prompt takes over the window: the toolbars step out of the way.
        if let Some(view) = ed.prompt {
            PromptLayout::new(ed.view.viewport, ed.dpr).draw(view, &mut ui);
        } else {
            let bar = ed.toolbar();
            bar.draw(&state, &mut ui);
            if let (Some(view), Some(panel)) =
                (&stamp.panel, bar.tool_panel(ed.tool, ed.view.viewport[0], ed.panel.is_some_and(|p| p.editing.is_some())))
            {
                panel.draw(view, &mut ui);
            }
        }
        self.gpu.set_ui(&ui.panels, &ui.lines);
        self.ui_built = Some(stamp);
    }
    /// Line work drawn over the committed ink, as `(document space, screen pixels)`; `now_ms` is
    /// the pen clock, for the laser's fading trail.
    pub(super) fn overlays(&self, now_ms: f64, mut out: Overlays) -> Overlays {
        let ed = &self.ed;
        out.doc.clear();
        out.screen.clear();
        out.now_ms = now_ms;
        selection_overlay(ed, &mut out);
        ed.tools.get(ed.tool).paint(ed, &mut out);
        pull_overlay(ed, &mut out);
        out
    }
}

/// One interface pixel in document points: lines of constant on-screen thickness.
fn pixel_in_points(ed: &Editor) -> f32 {
    ed.dpr / ed.view.zoom
}

/// The selection: its strokes (moving with the drag) and a box around them.
fn selection_overlay(ed: &Editor, out: &mut Overlays) {
    if ed.selection.is_empty() {
        return;
    }
    let px = pixel_in_points(ed);
    let offset = ed.drag_offset();
    let selected: HashSet<u64> = ed.selection.iter().copied().collect();
    // A shape being reshaped is drawn as it is becoming, not as it is stored.
    let live = ed.tools.shape.controlling().map(|(stroke, _, figure)| (stroke, figure.points()));
    for s in ed.ink.strokes().iter().filter(|s| selected.contains(&s.id)) {
        match &live {
            // (Exactly as it is: a shape is never smoothed, so its straight segments stay straight.)
            Some((id, pts)) if *id == s.id => push_polyline(&mut out.doc, pts, s.style.width, s.style.rgba),
            _ => stroke_capsules(s, offset, &mut out.doc),
        }
    }
    // A single shape is shown by its anchors instead of a box, and a text box being typed into by
    // its own frame.
    if ed.editing().is_some() || ed.text_edit().is_some() {
        return;
    }
    if let Some(b) = ed.ink.bounds_of(&ed.selection) {
        let pad = 5.0 * px;
        push_rect(&mut out.doc, [b[0] + offset[0] - pad, b[1] + offset[1] - pad, b[2] + offset[0] + pad, b[3] + offset[1] + pad], 1.5 * px, ACCENT);
    }
}

/// A badge at the bottom of the window while a finger pulls past the end of the document: a plus
/// in a circle whose rim fills as the pull nears the length that adds a page.
fn pull_overlay(ed: &Editor, out: &mut Overlays) {
    let progress = ed.pull_progress();
    if progress <= 0.0 {
        return;
    }
    let dpr = ed.dpr;
    let radius = 26.0 * dpr;
    // It rises into view as the pull grows.
    let rise = (progress * 3.0).min(1.0);
    let centre = [ed.view.viewport[0] / 2.0, ed.view.viewport[1] - (30.0 + 46.0 * rise) * dpr];
    let alpha = (rise * 255.0) as u8;
    out.screen.push(Capsule::new(centre, centre, radius * 2.0 + 3.0 * dpr, [20, 24, 40, alpha / 6]));
    out.screen.push(Capsule::new(centre, centre, radius * 2.0, [255, 255, 255, alpha]));
    const STEPS: usize = 48;
    let point = |a: f32| [centre[0] + (radius - 2.0 * dpr) * a.cos(), centre[1] + (radius - 2.0 * dpr) * a.sin()];
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = progress * std::f32::consts::TAU;
    let steps = ((STEPS as f32 * progress).ceil() as usize).max(1);
    for i in 0..steps {
        let (a, b) = (start + sweep * i as f32 / steps as f32, start + sweep * (i + 1) as f32 / steps as f32);
        out.screen.push(Capsule::new(point(a), point(b), 3.0 * dpr, [ACCENT[0], ACCENT[1], ACCENT[2], alpha]));
    }
    let arm = 9.0 * dpr;
    let ink = [64, 69, 82, alpha];
    out.screen.push(Capsule::new([centre[0] - arm, centre[1]], [centre[0] + arm, centre[1]], 2.4 * dpr, ink));
    out.screen.push(Capsule::new([centre[0], centre[1] - arm], [centre[0], centre[1] + arm], 2.4 * dpr, ink));
}

/// A stroke as capsules, shifted by `offset`: handwriting along a smooth curve through its points, a
/// shape exactly as its points are (they are already as fine as the shape needs, and smoothing would
/// bend its straight segments). A text box has none: its points are only the outline, and its text
/// is drawn from a bitmap.
fn stroke_capsules(s: &Stroke, offset: [f32; 2], out: &mut Vec<Capsule>) {
    if s.text.is_some() {
        return;
    }
    let at = |p: &P| [p[0] + offset[0], p[1] + offset[1]];
    let curve = if s.figure.is_some() { s.pts.clone() } else { curve_through(&s.pts, CURVE_STEP) };
    match curve.as_slice() {
        [] => {}
        [p] => out.push(Capsule::new(at(p), at(p), s.style.width, s.style.rgba)),
        pts => out.extend(pts.windows(2).map(|w| Capsule::new(at(&w[0]), at(&w[1]), s.style.width, s.style.rgba))),
    }
}

fn push_polyline(out: &mut Vec<Capsule>, pts: &[P], width: f32, color: [u8; 4]) {
    match pts {
        [] => {}
        [p] => out.push(Capsule::new(*p, *p, width, color)),
        _ => out.extend(pts.windows(2).map(|w| Capsule::new(w[0], w[1], width, color))),
    }
}

/// The outline of `r` (`x0, y0, x1, y1`).
fn push_rect(out: &mut Vec<Capsule>, r: [f32; 4], width: f32, color: [u8; 4]) {
    let corners = [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]], [r[0], r[1]]];
    out.extend(corners.windows(2).map(|w| Capsule::new(w[0], w[1], width, color)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::Style;

    const PEN: Style = Style { rgba: [0, 0, 0, 255], width: 2.0 };

    fn stamp(ink: &InkStore) -> InkStamp {
        InkStamp { revision: ink.revision(), strokes: ink.strokes().len(), selection: 0, erasing: 0 }
    }

    fn drawn(ink: &mut InkStore, n: usize) {
        for i in 0..n {
            ink.add(0, PEN, vec![[i as f32, 0.0], [i as f32 + 5.0, 3.0]]);
        }
    }

    #[test]
    fn a_shape_is_drawn_exactly_as_its_points_and_handwriting_is_smoothed() {
        // A broken line with a gentle bend: smoothed, it would become a curve, which a shape must not.
        let pts = vec![[0.0, 0.0], [100.0, 10.0], [200.0, 0.0]];
        let mut ink = InkStore::new();
        ink.add(0, PEN, pts.clone());
        ink.add_figure(0, PEN, crate::figure::Figure::Poly { points: pts.clone(), closed: false });
        let (mut hand, mut shape) = (Vec::new(), Vec::new());
        stroke_capsules(&ink.strokes()[0], [0.0, 0.0], &mut hand);
        stroke_capsules(&ink.strokes()[1], [0.0, 0.0], &mut shape);
        assert!(hand.len() > 20, "handwriting is drawn along a smooth curve ({} pieces)", hand.len());
        assert_eq!(shape.len(), 2, "a shape's straight segments are drawn as they are");
        assert_eq!((shape[0].a, shape[0].b, shape[1].a, shape[1].b), (pts[0], pts[1], pts[1], pts[2]));
        // Moved with a drag, the same.
        let mut moved = Vec::new();
        stroke_capsules(&ink.strokes()[1], [5.0, -2.0], &mut moved);
        assert_eq!((moved[0].a, moved[1].b), ([5.0, -2.0], [205.0, -2.0]));
    }

    #[test]
    fn nothing_changed_means_nothing_to_do() {
        let mut ink = InkStore::new();
        drawn(&mut ink, 3);
        assert_eq!(plan_refresh(&stamp(&ink), &stamp(&ink), &ink, 1), Refresh::Current);
    }

    #[test]
    fn the_first_sync_and_a_new_document_always_rebuild() {
        let mut ink = InkStore::new();
        drawn(&mut ink, 3);
        assert_eq!(plan_refresh(&InkStamp::NEVER, &stamp(&ink), &ink, 0), Refresh::Rebuild);
    }

    #[test]
    fn strokes_added_at_the_end_are_appended_however_many() {
        let mut ink = InkStore::new();
        drawn(&mut ink, 5);
        let built = stamp(&ink);
        drawn(&mut ink, 1);
        assert_eq!(plan_refresh(&built, &stamp(&ink), &ink, 1), Refresh::Append { from: 5 });
        drawn(&mut ink, 4);
        assert_eq!(plan_refresh(&built, &stamp(&ink), &ink, 1), Refresh::Append { from: 5 }, "several strokes since the last sync");
    }

    #[test]
    fn anything_but_an_append_rebuilds() {
        let mut ink = InkStore::new();
        drawn(&mut ink, 5);
        let built = stamp(&ink);
        let ids: Vec<u64> = ink.strokes().iter().map(|s| s.id).collect();
        // An append followed by any other change.
        drawn(&mut ink, 1);
        ink.move_by(&ids[..1], 3.0, 0.0, None);
        assert_eq!(plan_refresh(&built, &stamp(&ink), &ink, 1), Refresh::Rebuild, "a move");
        let mut ink = InkStore::new();
        drawn(&mut ink, 5);
        let built = stamp(&ink);
        ink.remove(&[ink.strokes()[2].id]);
        assert_eq!(plan_refresh(&built, &stamp(&ink), &ink, 1), Refresh::Rebuild, "an erase");
        let mut ink = InkStore::new();
        drawn(&mut ink, 5);
        let built = stamp(&ink);
        ink.undo();
        assert_eq!(plan_refresh(&built, &stamp(&ink), &ink, 1), Refresh::Rebuild, "an undo");
        ink.redo();
        assert_eq!(plan_refresh(&built, &stamp(&ink), &ink, 1), Refresh::Rebuild, "a redo, even back to the same strokes");
    }

    #[test]
    fn a_changed_selection_or_eraser_rebuilds_and_so_does_a_fragmented_buffer() {
        let mut ink = InkStore::new();
        drawn(&mut ink, 5);
        let built = stamp(&ink);
        drawn(&mut ink, 1);
        let now = stamp(&ink);
        assert_eq!(plan_refresh(&InkStamp { selection: 7, ..built }, &now, &ink, 1), Refresh::Rebuild);
        assert_eq!(plan_refresh(&InkStamp { erasing: 7, ..built }, &now, &ink, 1), Refresh::Rebuild);
        assert_eq!(plan_refresh(&built, &now, &ink, MAX_RANGES), Refresh::Rebuild);
    }

    #[test]
    fn ranges_of_one_page_join_and_others_stay_apart() {
        let mut pages = Vec::new();
        push_range(&mut pages, 0, 0..10);
        push_range(&mut pages, 0, 10..25);
        push_range(&mut pages, 1, 25..30);
        push_range(&mut pages, 0, 30..40);
        push_range(&mut pages, 0, 41..45);
        assert_eq!(pages, vec![(0, 0..25), (1, 25..30), (0, 30..40), (0, 41..45)]);
    }
}
