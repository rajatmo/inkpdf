//! The ink and interface line work on the GPU: committed strokes, the overlays drawn over them
//! (selection, the stroke being drawn, lasso, eraser ring, pop-ups) and the toolbar.

use std::collections::HashSet;
use std::ops::Range;

use crate::editor::Editor;
use crate::ink::{InkStore, MarkupKind, P, Stroke};
use crate::palette::Palette;
use crate::scene::Capsule;
use crate::smooth::curve_through;
use crate::toolbar::{self, ContextState, Look, PanelView, PromptView, Ui};
use crate::tools::Painter;
use crate::tools::paint::CURVE_STEP;

use super::{Paint, UiLayers};

/// What the GPU's committed-ink buffer was built from: the ink, how many strokes it had, the
/// selection and the strokes being erased (the last two as hashes of their ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InkStamp {
    revision: u64,
    strokes: usize,
    selection: u64,
    erasing: u64,
    /// Whether the pages are dark: the ink is drawn in other colours then.
    dark: bool,
}

impl InkStamp {
    /// Matches no real state, so the first sync always builds.
    pub const NEVER: InkStamp = InkStamp { revision: u64::MAX, strokes: 0, selection: 0, erasing: 0, dark: false };

    fn of(ed: &Editor) -> Self {
        Self {
            revision: ed.ink.revision(),
            strokes: ed.ink.strokes().len(),
            // (With the pages overview up the document is not seen, and its pictures show every stroke: none is hidden.)
            selection: if ed.overview_open() { 0 } else { hash_ids(ed.selection.iter(), 0x9E37_79B9_7F4A_7C15) },
            erasing: if ed.overview_open() { 0 } else { hash_ids(ed.erasing.iter(), 0xC2B2_AE3D_27D4_EB4F) },
            dark: ed.dark_pages_active(),
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
    let hidden_unchanged = built.selection == now.selection && built.erasing == now.erasing && built.dark == now.dark;
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
    panel: Option<(PanelView, u32)>,
    prompt: Option<(PromptView, u32)>,
    /// The bar under the tools.
    context: Option<ContextState>,
    /// The toolbar's buttons, in the middle of whatever is moving in them.
    look: Look,
    /// How far the bar under the tools has come in.
    bar_in: u32,
    /// The colours it was drawn in (they move while the scheme fades).
    pal: Palette,
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

impl UiLayers {
    /// Bring the GPU's committed-ink buffer up to date with the ink, the selection and the strokes
    /// being erased. Selected strokes are left out: they are drawn as an overlay.
    pub(super) fn sync_committed(&mut self, cx: &mut Paint) {
        let now = InkStamp::of(cx.ed);
        match plan_refresh(&self.ink_built, &now, &cx.ed.ink, self.ink_pages.len()) {
            Refresh::Current => return,
            Refresh::Append { from } if self.append_committed(cx, from) => {}
            Refresh::Append { .. } | Refresh::Rebuild => self.rebuild_committed(cx),
        }
        self.ink_built = now;
    }

    /// Add the strokes from index `from` on to the end of the buffer. `false` when they do not fit
    /// (the buffer is then rebuilt, with room to spare).
    fn append_committed(&mut self, cx: &mut Paint, from: usize) -> bool {
        let Some(new) = cx.ed.ink.strokes().get(from..) else { return false };
        let base = cx.gpu.committed_len();
        let mut capsules = Vec::new();
        let mut ranges: Vec<(u32, Range<u32>)> = Vec::new();
        for s in new {
            let start = base + capsules.len() as u32;
            stroke_capsules(s, [0.0, 0.0], cx.ed.on_page(s.style.rgba), &mut capsules);
            push_range(&mut ranges, s.page, start..base + capsules.len() as u32);
        }
        if !cx.gpu.append_committed(&capsules) {
            return false;
        }
        for (page, range) in ranges {
            push_range(&mut self.ink_pages, page, range);
        }
        true
    }

    /// Rebuild the buffer from scratch: the visible strokes grouped by page (so a page's ink is one
    /// range), in drawing order within each page.
    fn rebuild_committed(&mut self, cx: &mut Paint) {
        let ed = &cx.ed;
        let hidden: HashSet<u64> = if ed.overview_open() { HashSet::new() } else { ed.selection.iter().chain(&ed.erasing).copied().collect() };
        let mut shown: Vec<&Stroke> = ed.ink.strokes().iter().filter(|s| !hidden.contains(&s.id)).collect();
        shown.sort_by_key(|s| s.page);
        let mut all = Vec::with_capacity(shown.len() * 32);
        self.ink_pages.clear();
        for s in shown {
            let start = all.len() as u32;
            stroke_capsules(s, [0.0, 0.0], ed.on_page(s.style.rgba), &mut all);
            push_range(&mut self.ink_pages, s.page, start..all.len() as u32);
        }
        cx.gpu.set_committed(&all);
    }
    /// The ranges of the committed-ink buffer for the pages on screen.
    pub(super) fn visible_ink_ranges(&self, cx: &Paint) -> Vec<Range<u32>> {
        let (top, bottom) = cx.ed.view.visible_y();
        let pages = cx.ed.layout.pages_in(top, bottom);
        self.ink_pages.iter().filter(|(p, _)| pages.contains(&(*p as usize))).map(|(_, r)| r.clone()).collect()
    }

    /// Rebuild the toolbars and pen panel on the GPU when their look has changed: a different state,
    /// window, scale or panel. Otherwise they stay as they are and cost nothing per frame.
    pub(super) fn sync_ui(&mut self, cx: &mut Paint) {
        // The pages overview takes over the window: the interface steps out of the way.
        if self.sync_overview_ui(cx) {
            return;
        }
        if self.sync_save_as_ui(cx) {
            return;
        }
        let ed = &cx.ed;
        let state = ed.toolbar_state();
        let [w, h] = ed.view.viewport.map(f32::to_bits);
        // (What comes and goes is drawn as far in as it is: how far is part of what the interface was built from.)
        let stamp = UiStamp {
            state,
            viewport: [w, h],
            dpr: ed.dpr.to_bits(),
            panel: ed.panel_look().map(|(view, amount)| (view, amount.to_bits())),
            prompt: ed.prompt_look().map(|(view, amount)| (view, amount.to_bits())),
            context: ed.context_view(),
            look: ed.toolbar_look(),
            bar_in: ed.motions.bar_amount().to_bits(),
            pal: ed.palette(),
        };
        if self.ui_built.as_ref() == Some(&stamp) {
            return;
        }
        let mut ui = Ui::with_palette(ed.palette());
        ed.draw_interface(&mut ui);
        cx.gpu.set_ui(&ui.panels, &ui.lines);
        self.ui_built = Some(stamp);
    }
}

/// Line work drawn over the committed ink, as `(document space, screen pixels)`; `now_ms` is
/// the pen clock, for the laser's fading trail.
pub(super) fn overlays(ed: &Editor, now_ms: f64, mut out: Overlays) -> Overlays {
    out.doc.clear();
    out.screen.clear();
    out.now_ms = now_ms;
    selection_overlay(ed, &mut out);
    ed.tools.get(ed.tool).paint(ed, &mut out);
    pull_overlay(ed, &mut out);
    out
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
            Some((id, pts)) if *id == s.id => push_polyline(&mut out.doc, pts, s.style.width, ed.on_page(s.style.rgba)),
            _ => stroke_capsules(s, offset, ed.on_page(s.style.rgba), &mut out.doc),
        }
    }
    // A single shape is shown by its anchors instead of a box, and a text box being typed into by
    // its own frame.
    if ed.editing().is_some() || ed.text_edit().is_some() {
        return;
    }
    if let Some(b) = ed.ink.bounds_of(&ed.selection) {
        let pad = 5.0 * px;
        push_rect(
            &mut out.doc,
            [b[0] + offset[0] - pad, b[1] + offset[1] - pad, b[2] + offset[0] + pad, b[3] + offset[1] + pad],
            1.5 * px,
            ed.theme.selection,
        );
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
        out.screen.push(Capsule::new(point(a), point(b), 3.0 * dpr, {
            let a = ed.theme.selection;
            [a[0], a[1], a[2], alpha]
        }));
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
fn stroke_capsules(s: &Stroke, offset: [f32; 2], colour: [u8; 4], out: &mut Vec<Capsule>) {
    if s.text.is_some() {
        return;
    }
    if let Some(kind) = s.markup {
        for q in s.quads() {
            markup_capsules(kind, [q[0] + offset[0], q[1] + offset[1], q[2] + offset[0], q[3] + offset[1]], colour, out);
        }
        return;
    }
    let at = |p: &P| [p[0] + offset[0], p[1] + offset[1]];
    let curve = if s.figure.is_some() { s.pts.clone() } else { curve_through(&s.pts, CURVE_STEP) };
    match curve.as_slice() {
        [] => {}
        [p] => out.push(Capsule::new(at(p), at(p), s.style.width, colour)),
        pts => out.extend(pts.windows(2).map(|w| Capsule::new(at(&w[0]), at(&w[1]), s.style.width, colour))),
    }
}

/// One box of a text markup as capsules: a highlighter is one fat capsule (its rounded ends are a marker's), an underline and a strike-out a
/// line a tenth of the box's height thick along its bottom and through its middle, a squiggly a zig-zag along the bottom.
fn markup_capsules(kind: MarkupKind, q: [f32; 4], colour: [u8; 4], out: &mut Vec<Capsule>) {
    let (w, h) = (q[2] - q[0], q[3] - q[1]);
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return;
    }
    let thin = (h * 0.09).max(0.6);
    match kind {
        MarkupKind::Highlight => {
            let ends = (h / 2.0).min(w / 2.0);
            out.push(Capsule::new([q[0] + ends, q[1] + h / 2.0], [q[2] - ends, q[1] + h / 2.0], h, colour));
        }
        MarkupKind::Underline => out.push(Capsule::new([q[0], q[3] - thin], [q[2], q[3] - thin], thin, colour)),
        MarkupKind::StrikeOut => out.push(Capsule::new([q[0], q[1] + h / 2.0], [q[2], q[1] + h / 2.0], thin, colour)),
        MarkupKind::Squiggly => {
            let (step, amp) = ((h * 0.3).max(1.0), h * 0.1);
            let base = q[3] - thin - amp;
            let n = ((w / step).ceil() as usize).clamp(1, 4000);
            let point = |i: usize| [(q[0] + i as f32 * step).min(q[2]), base + if i.is_multiple_of(2) { -amp } else { amp }];
            out.extend((0..n).map(|i| Capsule::new(point(i), point(i + 1), thin, colour)));
        }
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
        InkStamp { revision: ink.revision(), strokes: ink.strokes().len(), selection: 0, erasing: 0, dark: false }
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
        stroke_capsules(&ink.strokes()[0], [0.0, 0.0], PEN.rgba, &mut hand);
        stroke_capsules(&ink.strokes()[1], [0.0, 0.0], PEN.rgba, &mut shape);
        assert!(hand.len() > 20, "handwriting is drawn along a smooth curve ({} pieces)", hand.len());
        assert_eq!(shape.len(), 2, "a shape's straight segments are drawn as they are");
        assert_eq!((shape[0].a, shape[0].b, shape[1].a, shape[1].b), (pts[0], pts[1], pts[1], pts[2]));
        // Moved with a drag, the same.
        let mut moved = Vec::new();
        stroke_capsules(&ink.strokes()[1], [5.0, -2.0], PEN.rgba, &mut moved);
        assert_eq!((moved[0].a, moved[1].b), ([5.0, -2.0], [205.0, -2.0]));
    }

    #[test]
    fn a_change_of_the_pages_look_rebuilds_the_ink_because_it_is_drawn_in_other_colours() {
        let mut ink = InkStore::new();
        drawn(&mut ink, 3);
        let dark = InkStamp { dark: true, ..stamp(&ink) };
        assert_eq!(plan_refresh(&stamp(&ink), &dark, &ink, 1), Refresh::Rebuild);
        assert_eq!(plan_refresh(&dark, &dark, &ink, 1), Refresh::Current);
    }

    #[test]
    fn each_kind_of_markup_is_drawn_over_the_box_it_marks_and_nowhere_else() {
        let q = [100.0, 200.0, 180.0, 216.0];
        let red = [255, 0, 0, 120];
        for kind in MarkupKind::ALL {
            let mut out = Vec::new();
            markup_capsules(kind, q, red, &mut out);
            assert!(!out.is_empty(), "{kind:?} draws something");
            for c in &out {
                assert_eq!(c.color, red);
                for p in [c.a, c.b] {
                    assert!(
                        p[0] >= q[0] - 0.01 && p[0] <= q[2] + 0.01 && p[1] >= q[1] - 0.01 && p[1] <= q[3] + 0.01,
                        "{kind:?} stays in its box: {p:?}"
                    );
                }
            }
        }
        let mut hi = Vec::new();
        markup_capsules(MarkupKind::Highlight, q, red, &mut hi);
        assert_eq!((hi.len(), hi[0].width), (1, 16.0), "a highlighter is one capsule as high as the box");
        let (mut under, mut strike) = (Vec::new(), Vec::new());
        markup_capsules(MarkupKind::Underline, q, red, &mut under);
        markup_capsules(MarkupKind::StrikeOut, q, red, &mut strike);
        assert!(under[0].a[1] > strike[0].a[1], "the underline is below the strike-out");
        // A box with no size, or one that is not a number, draws nothing.
        let mut none = Vec::new();
        for bad in [[0.0, 0.0, 0.0, 10.0], [0.0, 0.0, 10.0, 0.0], [f32::NAN, 0.0, 10.0, 10.0], [10.0, 10.0, 0.0, 0.0]] {
            markup_capsules(MarkupKind::Squiggly, bad, red, &mut none);
        }
        assert!(none.is_empty());
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
