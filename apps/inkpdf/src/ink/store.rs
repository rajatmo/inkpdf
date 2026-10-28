//! The set of strokes in a document, with undo and redo, dirty-page tracking and hit testing.

use std::collections::{BTreeSet, HashSet};

use crate::figure::Figure;

use super::{P, PageStroke, Stroke, Style, TextBlock};

/// Longest stroke kept, in points; a stray gesture cannot grow without bound.
const MAX_POINTS: usize = 100_000;
/// Furthest from the origin, in either direction, that a stroke may be moved to. A page is never more
/// than a few thousand points across, and numbers this large are where arithmetic on shapes starts
/// to lose its precision.
const MAX_COORD: f32 = 1.0e6;
/// Undo steps kept.
const MAX_HISTORY: usize = 500;

enum Cmd {
    Add(Vec<Stroke>),
    /// With each stroke's position, so undo puts it back where it was in the drawing order.
    Remove(Vec<(usize, Stroke)>),
    Move {
        ids: Vec<u64>,
        dx: f32,
        dy: f32,
        from: Vec<(u64, u32)>,
        to_page: Option<u32>,
    },
    /// A shape reshaped by its anchors: the stroke before and after.
    Reshape {
        before: Box<Stroke>,
        after: Box<Stroke>,
    },
}

#[derive(Default)]
pub struct InkStore {
    strokes: Vec<Stroke>,
    next_id: u64,
    undo: Vec<Cmd>,
    redo: Vec<Cmd>,
    dirty: BTreeSet<u32>,
    /// Counts every change to the strokes.
    revision: u64,
    /// The revision of the last change that was not just a stroke added at the end.
    structural: u64,
}

impl InkStore {
    pub fn new() -> Self {
        Self { next_id: 1, ..Self::default() }
    }

    /// Replace everything with strokes read from a file: no undo history, nothing dirty.
    pub fn load(&mut self, strokes: Vec<PageStroke>) {
        *self = Self::new();
        for (page, style, pts, figure, text) in strokes {
            if let Some(pts) = sanitize(pts) {
                let id = self.take_id();
                // A figure that does not draw what was stored is not to be trusted.
                let figure = figure.filter(|f| f.points() == pts);
                self.strokes.push(Stroke::new(id, page, style, pts, figure, text));
            }
        }
        self.revision += 1;
        self.structural = self.revision;
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether everything since `revision` was strokes added at the end: what was drawn then is
    /// still there, in the same order, and the strokes past the count it had then are new.
    pub fn only_appended_since(&self, revision: u64) -> bool {
        self.structural <= revision
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.strokes.len()
    }

    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    pub fn get(&self, id: u64) -> Option<&Stroke> {
        self.strokes.iter().find(|s| s.id == id)
    }

    pub fn dirty_pages(&self) -> &BTreeSet<u32> {
        &self.dirty
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Flag a page as changed without touching its ink (a page that was added).
    pub fn mark_dirty(&mut self, page: u32) {
        self.dirty.insert(page);
    }

    pub fn mark_clean(&mut self) {
        self.dirty.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn record(&mut self, cmd: Cmd) {
        let appended = matches!(cmd, Cmd::Add(_));
        self.undo.push(cmd);
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
        if !appended {
            self.structural = self.revision;
        }
    }

    /// Add a finished stroke. `None` when it has no usable points.
    pub fn add(&mut self, page: u32, style: Style, pts: Vec<P>) -> Option<u64> {
        self.add_stroke(page, style, pts, None, None)
    }

    /// Add a text box: `outline` is its rectangle (see [`TextBlock`]), `style.rgba` the text's colour.
    pub fn add_text(&mut self, page: u32, style: Style, text: TextBlock, outline: Vec<P>) -> Option<u64> {
        self.add_stroke(page, style, outline, None, Some(text))
    }

    /// Add a shape, drawn from `figure`, so that its anchors can be edited later.
    pub fn add_figure(&mut self, page: u32, style: Style, figure: Figure) -> Option<u64> {
        let pts = figure.points();
        self.add_stroke(page, style, pts, Some(figure), None)
    }

    fn add_stroke(&mut self, page: u32, style: Style, pts: Vec<P>, figure: Option<Figure>, text: Option<TextBlock>) -> Option<u64> {
        let pts = sanitize(pts)?;
        let id = self.take_id();
        let stroke = Stroke::new(id, page, style, pts, figure, text);
        self.dirty.insert(page);
        self.strokes.push(stroke.clone());
        self.record(Cmd::Add(vec![stroke]));
        Some(id)
    }

    /// Remove strokes by id as one undo step. Returns how many were removed.
    pub fn remove(&mut self, ids: &[u64]) -> usize {
        let wanted: HashSet<u64> = ids.iter().copied().collect();
        let mut removed: Vec<(usize, Stroke)> = Vec::new();
        for (i, s) in self.strokes.iter().enumerate() {
            if wanted.contains(&s.id) {
                removed.push((i, s.clone()));
            }
        }
        if removed.is_empty() {
            return 0;
        }
        self.strokes.retain(|s| !wanted.contains(&s.id));
        for (_, s) in &removed {
            self.dirty.insert(s.page);
        }
        let n = removed.len();
        self.record(Cmd::Remove(removed));
        n
    }

    /// A blank page was inserted at index `at`, pushing the pages from there on down the document by
    /// `dy`: the strokes on them (and in the undo history) go with them, and keep their place on their
    /// page. A move that carried a stroke across the new page cannot be undone sensibly any more, so
    /// the history up to it is dropped.
    pub fn insert_page_at(&mut self, at: u32, dy: f32) {
        fn shift(s: &mut Stroke, at: u32, dy: f32) {
            if s.page >= at {
                s.page += 1;
                s.translate(0.0, dy);
            }
        }
        let crosses = |cmd: &Cmd| match cmd {
            Cmd::Move { from, to_page, .. } => from.iter().any(|&(_, p)| (p < at) != (to_page.unwrap_or(p) < at)),
            _ => false,
        };
        if let Some(last) = self.undo.iter().rposition(&crosses) {
            self.undo.drain(..=last);
        }
        if self.redo.iter().any(&crosses) {
            self.redo.clear();
        }
        self.strokes.iter_mut().for_each(|s| shift(s, at, dy));
        for cmd in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            match cmd {
                Cmd::Add(strokes) => strokes.iter_mut().for_each(|s| shift(s, at, dy)),
                Cmd::Remove(removed) => removed.iter_mut().for_each(|(_, s)| shift(s, at, dy)),
                Cmd::Move { from, to_page, .. } => {
                    from.iter_mut().filter(|(_, p)| *p >= at).for_each(|(_, p)| *p += 1);
                    if let Some(p) = to_page.as_mut().filter(|p| **p >= at) {
                        *p += 1;
                    }
                }
                Cmd::Reshape { before, after } => {
                    shift(before, at, dy);
                    shift(after, at, dy);
                }
            }
        }
        self.dirty = self.dirty.iter().map(|&p| if p >= at { p + 1 } else { p }).collect();
        self.revision += 1;
        self.structural = self.revision;
    }

    /// Move strokes by `(dx, dy)` as one undo step, optionally onto another page.
    pub fn move_by(&mut self, ids: &[u64], dx: f32, dy: f32, to_page: Option<u32>) -> usize {
        if !dx.is_finite() || !dy.is_finite() {
            return 0;
        }
        let wanted: HashSet<u64> = ids.iter().copied().collect();
        let from: Vec<(u64, u32)> = self.strokes.iter().filter(|s| wanted.contains(&s.id)).map(|s| (s.id, s.page)).collect();
        if from.is_empty() || (dx == 0.0 && dy == 0.0 && to_page.is_none_or(|p| from.iter().all(|f| f.1 == p))) {
            return 0;
        }
        // Not off into space: the result must stay where numbers are still exact enough to draw.
        let out_of_bounds = self.strokes.iter().filter(|s| wanted.contains(&s.id)).any(|s| {
            let b = s.bbox();
            [b[0] + dx, b[2] + dx].iter().chain(&[b[1] + dy, b[3] + dy]).any(|v| v.abs() > MAX_COORD)
        });
        if out_of_bounds {
            return 0;
        }
        let ids: Vec<u64> = from.iter().map(|f| f.0).collect();
        let cmd = Cmd::Move { ids, dx, dy, from, to_page };
        self.apply(&cmd);
        let n = match &cmd {
            Cmd::Move { ids, .. } => ids.len(),
            _ => 0,
        };
        self.record(cmd);
        n
    }

    /// Change what a shape is made of (its anchors moved, one added or taken away) as one undo
    /// step. `false` if there is no such shape or the figure is no good.
    pub fn reshape(&mut self, id: u64, figure: Figure) -> bool {
        let pts = figure.points();
        let Some(old) = self.strokes.iter().find(|s| s.id == id && s.figure.is_some()) else { return false };
        let style = old.style;
        self.replace_stroke(id, style, pts, Some(figure), None)
    }

    /// Change a text box's text, size and colour (and its outline, which follows the text) as one
    /// undo step. `false` if there is no such text box or nothing changed.
    pub fn edit_text(&mut self, id: u64, style: Style, text: TextBlock, outline: Vec<P>) -> bool {
        if !self.strokes.iter().any(|s| s.id == id && s.text.is_some()) {
            return false;
        }
        self.replace_stroke(id, style, outline, None, Some(text))
    }

    fn replace_stroke(&mut self, id: u64, style: Style, pts: Vec<P>, figure: Option<Figure>, text: Option<TextBlock>) -> bool {
        let Some(pts) = sanitize(pts) else { return false };
        let Some(old) = self.strokes.iter().find(|s| s.id == id) else { return false };
        if old.pts == pts && old.style == style && old.text == text {
            return false;
        }
        let after = Stroke::new(id, old.page, style, pts, figure, text);
        let cmd = Cmd::Reshape { before: Box::new(old.clone()), after: Box::new(after) };
        self.apply(&cmd);
        self.record(cmd);
        true
    }

    fn replace(&mut self, with: &Stroke) {
        if let Some(slot) = self.strokes.iter_mut().find(|s| s.id == with.id) {
            *slot = with.clone();
        }
        self.dirty.insert(with.page);
    }

    fn apply(&mut self, cmd: &Cmd) {
        match cmd {
            Cmd::Reshape { after, .. } => self.replace(after),
            Cmd::Add(strokes) => {
                for s in strokes {
                    self.dirty.insert(s.page);
                    self.strokes.push(s.clone());
                }
            }
            Cmd::Remove(removed) => {
                let ids: HashSet<u64> = removed.iter().map(|(_, s)| s.id).collect();
                for (_, s) in removed {
                    self.dirty.insert(s.page);
                }
                self.strokes.retain(|s| !ids.contains(&s.id));
            }
            Cmd::Move { ids, dx, dy, from, to_page } => {
                let ids: HashSet<u64> = ids.iter().copied().collect();
                for s in self.strokes.iter_mut().filter(|s| ids.contains(&s.id)) {
                    self.dirty.insert(s.page);
                    s.translate(*dx, *dy);
                    if let Some(p) = to_page {
                        s.page = *p;
                    }
                    self.dirty.insert(s.page);
                }
                let _ = from;
            }
        }
    }

    fn revert(&mut self, cmd: &Cmd) {
        match cmd {
            Cmd::Reshape { before, .. } => self.replace(before),
            Cmd::Add(strokes) => {
                let ids: HashSet<u64> = strokes.iter().map(|s| s.id).collect();
                for s in strokes {
                    self.dirty.insert(s.page);
                }
                self.strokes.retain(|s| !ids.contains(&s.id));
            }
            Cmd::Remove(removed) => {
                // Ascending positions, so each insert lands where the stroke was.
                for (i, s) in removed {
                    self.dirty.insert(s.page);
                    let at = (*i).min(self.strokes.len());
                    self.strokes.insert(at, s.clone());
                }
            }
            Cmd::Move { ids, dx, dy, from, .. } => {
                let ids: HashSet<u64> = ids.iter().copied().collect();
                let from: std::collections::HashMap<u64, u32> = from.iter().copied().collect();
                for s in self.strokes.iter_mut().filter(|s| ids.contains(&s.id)) {
                    self.dirty.insert(s.page);
                    s.translate(-*dx, -*dy);
                    if let Some(&page) = from.get(&s.id) {
                        s.page = page;
                    }
                    self.dirty.insert(s.page);
                }
            }
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(cmd) = self.undo.pop() else { return false };
        self.revert(&cmd);
        self.redo.push(cmd);
        self.revision += 1;
        self.structural = self.revision;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(cmd) = self.redo.pop() else { return false };
        self.apply(&cmd);
        self.undo.push(cmd);
        self.revision += 1;
        self.structural = self.revision;
        true
    }

    // ── queries ─────────────────────────────────────────────────────────────────────────────

    /// The topmost stroke within `radius` of `p`.
    pub fn hit_test(&self, p: P, radius: f32) -> Option<u64> {
        self.strokes.iter().rev().find(|s| s.touches_circle(p, radius)).map(|s| s.id)
    }

    /// Strokes the eraser touches moving from `a` to `b` with the given radius.
    pub fn touched_by_segment(&self, a: P, b: P, radius: f32) -> Vec<u64> {
        self.strokes.iter().filter(|s| s.touches_segment(a, b, radius)).map(|s| s.id).collect()
    }

    /// Strokes mostly (at least half their points) inside the lasso.
    pub fn in_polygon(&self, poly: &[P]) -> Vec<u64> {
        self.strokes.iter().filter(|s| s.fraction_inside(poly) >= 0.5).map(|s| s.id).collect()
    }

    /// The bounding box of the strokes with these ids.
    pub fn bounds_of(&self, ids: &[u64]) -> Option<[f32; 4]> {
        let wanted: HashSet<u64> = ids.iter().copied().collect();
        let mut out: Option<[f32; 4]> = None;
        for s in self.strokes.iter().filter(|s| wanted.contains(&s.id)) {
            let b = s.bbox();
            out = Some(out.map_or(b, |o| [o[0].min(b[0]), o[1].min(b[1]), o[2].max(b[2]), o[3].max(b[3])]));
        }
        out
    }
}

/// A stroke's points if usable: finite, at least one, capped; a single point becomes a dot.
fn sanitize(mut pts: Vec<P>) -> Option<Vec<P>> {
    pts.retain(|p| p[0].is_finite() && p[1].is_finite());
    pts.truncate(MAX_POINTS);
    (!pts.is_empty()).then_some(pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Style = Style { rgba: [0, 0, 0, 255], width: 2.0 };

    fn line(y: f32) -> Vec<P> {
        vec![[0.0, y], [10.0, y], [20.0, y]]
    }
    #[test]
    fn bbox_includes_half_the_width() {
        let mut store = InkStore::new();
        let id = store.add(0, Style { width: 4.0, ..BLACK }, vec![[10.0, 10.0], [20.0, 30.0]]).expect("stroke");
        assert_eq!(store.get(id).expect("get").bbox(), [8.0, 8.0, 22.0, 32.0]);
    }

    #[test]
    fn hit_test_takes_the_topmost_and_respects_width() {
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, line(0.0)).expect("a");
        let b = store.add(0, BLACK, line(0.0)).expect("b");
        assert_eq!(store.hit_test([5.0, 0.5], 0.5), Some(b));
        assert_eq!(store.hit_test([5.0, 3.0], 0.5), None);
        // Width 2 means a 1pt reach, so a 1.4pt radius reaches 2.4pt.
        assert_eq!(store.hit_test([5.0, 2.3], 1.4), Some(b));
        assert_ne!(a, b);
    }

    #[test]
    fn a_dot_can_be_hit() {
        let mut store = InkStore::new();
        let id = store.add(0, BLACK, vec![[50.0, 50.0]]).expect("dot");
        assert_eq!(store.hit_test([50.5, 50.5], 0.5), Some(id));
        assert_eq!(store.hit_test([60.0, 50.0], 0.5), None);
    }

    #[test]
    fn eraser_segment_finds_crossed_strokes_only() {
        let mut store = InkStore::new();
        let near = store.add(0, BLACK, line(0.0)).expect("near");
        let _far = store.add(0, BLACK, line(50.0)).expect("far");
        let hit = store.touched_by_segment([5.0, -10.0], [5.0, 10.0], 1.0);
        assert_eq!(hit, vec![near]);
        // Sweeping across both finds both.
        assert_eq!(store.touched_by_segment([5.0, -10.0], [5.0, 60.0], 1.0).len(), 2);
    }

    #[test]
    fn lasso_selects_strokes_that_are_mostly_inside() {
        let mut store = InkStore::new();
        let inside = store.add(0, BLACK, vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0]]).expect("in");
        let _outside = store.add(0, BLACK, vec![[200.0, 200.0], [210.0, 200.0]]).expect("out");
        let half = store.add(0, BLACK, vec![[10.0, 40.0], [20.0, 40.0], [150.0, 40.0], [160.0, 40.0]]).expect("half");
        let poly = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let mut got = store.in_polygon(&poly);
        got.sort_unstable();
        assert_eq!(got, vec![inside, half]);
        // Fewer than three polygon points selects nothing.
        assert!(store.in_polygon(&poly[..2]).is_empty());
    }

    #[test]
    fn undo_and_redo_an_add() {
        let mut store = InkStore::new();
        let id = store.add(2, BLACK, line(0.0)).expect("add");
        assert!(store.can_undo() && !store.can_redo());
        assert!(store.undo());
        assert_eq!(store.len(), 0);
        assert!(store.redo());
        assert_eq!(store.get(id).map(|s| s.page), Some(2));
        assert!(!store.redo());
    }

    #[test]
    fn a_new_action_clears_redo() {
        let mut store = InkStore::new();
        store.add(0, BLACK, line(0.0));
        store.undo();
        assert!(store.can_redo());
        store.add(0, BLACK, line(1.0));
        assert!(!store.can_redo());
    }

    #[test]
    fn undoing_a_removal_restores_the_drawing_order() {
        let mut store = InkStore::new();
        let ids: Vec<u64> = (0..5).map(|i| store.add(0, BLACK, line(i as f32)).expect("add")).collect();
        assert_eq!(store.remove(&[ids[1], ids[3]]), 2);
        assert_eq!(store.strokes().iter().map(|s| s.id).collect::<Vec<_>>(), vec![ids[0], ids[2], ids[4]]);
        assert!(store.undo());
        assert_eq!(store.strokes().iter().map(|s| s.id).collect::<Vec<_>>(), ids);
        assert!(store.redo());
        assert_eq!(store.len(), 3);
        // Removing nothing records nothing.
        let before = store.revision();
        assert_eq!(store.remove(&[9999]), 0);
        assert_eq!(store.revision(), before);
    }

    #[test]
    fn move_and_undo_restore_position_and_page_and_dirty_both_pages() {
        let mut store = InkStore::new();
        let id = store.add(0, BLACK, line(0.0)).expect("add");
        store.mark_clean();
        assert_eq!(store.move_by(&[id], 5.0, 7.0, Some(1)), 1);
        let s = store.get(id).expect("moved");
        assert_eq!((s.page, s.pts[0]), (1, [5.0, 7.0]));
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![0, 1]);
        store.mark_clean();
        assert!(store.undo());
        let s = store.get(id).expect("restored");
        assert_eq!((s.page, s.pts[0]), (0, [0.0, 0.0]));
        assert_eq!(store.dirty_pages().len(), 2);
        assert!(store.redo());
        assert_eq!(store.get(id).map(|s| s.page), Some(1));
        // A move by nothing onto the same page is not an edit.
        assert_eq!(store.move_by(&[id], 0.0, 0.0, None), 0);
        assert_eq!(store.move_by(&[id], f32::NAN, 0.0, None), 0);
    }

    #[test]
    fn dirty_tracking_and_load() {
        let mut store = InkStore::new();
        store.load(vec![(3, BLACK, line(0.0), None, None), (3, BLACK, vec![], None, None), (4, BLACK, vec![[f32::NAN, 0.0]], None, None)]);
        // Loading keeps only usable strokes, with no history and nothing to save.
        assert_eq!(store.len(), 1);
        assert!(!store.is_dirty() && !store.can_undo());
        store.add(7, BLACK, line(0.0));
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![7]);
        store.mark_clean();
        assert!(!store.is_dirty());
    }

    #[test]
    fn unusable_strokes_are_refused() {
        let mut store = InkStore::new();
        assert_eq!(store.add(0, BLACK, vec![]), None);
        assert_eq!(store.add(0, BLACK, vec![[f32::INFINITY, 0.0]]), None);
        assert_eq!(store.len(), 0);
        let long = store.add(0, BLACK, vec![[1.0, 1.0]; MAX_POINTS + 50]).expect("capped");
        assert_eq!(store.get(long).map(|s| s.pts.len()), Some(MAX_POINTS));
    }

    #[test]
    fn history_is_capped() {
        let mut store = InkStore::new();
        for i in 0..(MAX_HISTORY + 20) {
            store.add(0, BLACK, line(i as f32));
        }
        let mut undone = 0;
        while store.undo() {
            undone += 1;
        }
        assert_eq!(undone, MAX_HISTORY);
        assert_eq!(store.len(), 20);
    }

    #[test]
    fn bounds_of_a_selection() {
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, vec![[0.0, 0.0], [10.0, 10.0]]).expect("a");
        let b = store.add(0, BLACK, vec![[50.0, 60.0]]).expect("b");
        assert_eq!(store.bounds_of(&[a, b]), Some([-1.0, -1.0, 51.0, 61.0]));
        assert_eq!(store.bounds_of(&[]), None);
    }
    // ── pages inserted among the others ──────────────────────────────────────────────────────

    fn on_page(page: u32, y: f32) -> Vec<P> {
        vec![[10.0, y + page as f32 * 1000.0], [60.0, y + 20.0 + page as f32 * 1000.0]]
    }

    #[test]
    fn a_page_inserted_pushes_the_strokes_from_that_page_on_down_with_their_pages() {
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, on_page(0, 10.0)).expect("a");
        let b = store.add(1, BLACK, on_page(1, 10.0)).expect("b");
        let c = store.add(2, BLACK, on_page(2, 10.0)).expect("c");
        store.insert_page_at(1, 800.0);
        let (sa, sb, sc) = (store.get(a).expect("a"), store.get(b).expect("b"), store.get(c).expect("c"));
        assert_eq!((sa.page, sb.page, sc.page), (0, 2, 3));
        assert_eq!(sa.pts, on_page(0, 10.0), "before the new page: untouched");
        assert_eq!(sb.pts[0], [10.0, 1810.0], "after it: down by the page");
        assert_eq!(sc.pts[0], [10.0, 2810.0]);
        assert_eq!(sb.bbox()[1], 1810.0 - 1.0, "the bounding box moved too");
    }

    #[test]
    fn the_history_follows_the_pages_so_undo_and_redo_put_things_where_the_pages_are_now() {
        let mut store = InkStore::new();
        let a = store.add(1, BLACK, on_page(1, 10.0)).expect("a");
        let b = store.add(1, BLACK, on_page(1, 200.0)).expect("b");
        store.remove(&[a]);
        store.insert_page_at(0, 500.0);
        store.undo();
        let back = store.get(a).expect("the erased stroke is back");
        assert_eq!((back.page, back.pts[0]), (2, [10.0, 1510.0]), "on its page, which moved");
        store.undo();
        assert!(store.get(b).is_none());
        store.redo();
        assert_eq!(store.get(b).expect("b").pts[0][1], 1700.0);
        // Dirty pages move too.
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_move_across_the_inserted_page_cannot_be_undone_sensibly_so_the_history_up_to_it_goes() {
        let mut store = InkStore::new();
        let early = store.add(0, BLACK, on_page(0, 10.0)).expect("early");
        let mover = store.add(0, BLACK, on_page(0, 100.0)).expect("mover");
        store.move_by(&[mover], 0.0, 1000.0, Some(1));
        let late = store.add(1, BLACK, on_page(1, 300.0)).expect("late");
        store.insert_page_at(1, 800.0);
        // The newest step is still there; the move, and what came before it, are not.
        assert!(store.undo());
        assert!(store.get(late).is_none());
        assert!(!store.can_undo(), "nothing older survives the crossing move");
        assert!(store.get(early).is_some() && store.get(mover).is_some(), "the ink itself is all still there");
    }

    #[test]
    fn a_move_within_the_pages_that_shifted_keeps_its_history() {
        let mut store = InkStore::new();
        let s = store.add(1, BLACK, on_page(1, 10.0)).expect("s");
        store.move_by(&[s], 30.0, 40.0, None);
        store.insert_page_at(1, 800.0);
        let at = store.get(s).expect("s").pts[0];
        store.undo();
        let undone = store.get(s).expect("s").pts[0];
        assert_eq!([at[0] - undone[0], at[1] - undone[1]], [30.0, 40.0], "undoing the move still takes back exactly the move");
        assert_eq!(store.get(s).expect("s").page, 2);
    }

    #[test]
    fn inserting_at_the_end_changes_nothing_and_hostile_inputs_do_no_harm() {
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, on_page(0, 10.0)).expect("a");
        let before = store.get(a).expect("a").clone();
        store.insert_page_at(5, 800.0);
        assert_eq!(store.get(a), Some(&before));
        store.insert_page_at(0, f32::MAX);
        store.insert_page_at(0, f32::NAN);
        assert!(store.get(a).is_some());
    }
}
