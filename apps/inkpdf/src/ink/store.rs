//! The set of strokes in a document, with undo and redo, dirty-page tracking and hit testing.

use std::collections::{BTreeSet, HashSet};

use crate::figure::Figure;
use crate::pages::{PageEdit, Remap};

use super::{MarkupKind, P, PageStroke, Stroke, Style, TextBlock};

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
    /// Several strokes restyled at once (a new colour, thickness or text size): each before and after.
    Restyle {
        before: Vec<Stroke>,
        after: Vec<Stroke>,
    },
    /// The pages were edited (added, deleted, rearranged).
    Pages(Box<PageStep>),
}

/// An edit of the pages as the history holds it.
struct PageStep {
    edit: PageEdit,
    /// The edit that takes the pages back.
    undo: PageEdit,
    forward: Remap,
    backward: Remap,
    /// The strokes that were on the pages that went, each with its place in the drawing order.
    buried: Vec<(usize, Stroke)>,
    /// The strokes of the pages that were copied, as the copies have them: they come back when the step is redone.
    made: Vec<Stroke>,
}

/// A stroke that is to become something else, keeping its id and its page (see [`InkStore::replace_many`]).
pub struct Replacement {
    pub id: u64,
    pub style: Style,
    pub pts: Vec<P>,
    pub figure: Option<Figure>,
    pub text: Option<TextBlock>,
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
    /// The edit of the pages the last undo or redo made, until it is taken (see [`InkStore::take_page_edit`]).
    page_edit: Option<PageEdit>,
    /// The graves of deletions whose steps the history let go of, until they are taken (see [`InkStore::take_evicted_graves`]).
    evicted: Vec<u64>,
}

impl InkStore {
    pub fn new() -> Self {
        Self { next_id: 1, ..Self::default() }
    }

    /// Replace everything with strokes read from a file: no undo history, nothing dirty.
    pub fn load(&mut self, strokes: Vec<PageStroke>) {
        *self = Self::new();
        for (page, style, pts, figure, text, markup) in strokes {
            if let Some(pts) = sanitize(pts) {
                let pts = if markup.is_some() { box_points(pts) } else { pts };
                if pts.is_empty() {
                    continue;
                }
                let id = self.take_id();
                // A figure that does not draw what was stored is not to be trusted.
                let figure = figure.filter(|f| f.points() == pts);
                self.strokes.push(Stroke::new(id, page, style, pts, figure, text, markup));
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
    #[cfg(test)]
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
        if self.undo.len() > MAX_HISTORY
            && let Cmd::Pages(step) = self.undo.remove(0)
            && let PageEdit::Delete { grave, .. } = step.edit
            && grave != 0
        {
            // The oldest step goes: a deletion that can no longer be undone has nothing to be kept for.
            self.evicted.push(grave);
        }
        self.redo.clear();
        self.revision += 1;
        if !appended {
            self.structural = self.revision;
        }
    }

    /// Add a finished stroke. `None` when it has no usable points.
    pub fn add(&mut self, page: u32, style: Style, pts: Vec<P>) -> Option<u64> {
        self.add_stroke(page, style, pts, None, None, None)
    }

    /// Add a text markup of `kind` over `quads` (boxes, `[x0, y0, x1, y1]` in document space); `style.rgba` is its colour and opacity.
    pub fn add_markup(&mut self, page: u32, style: Style, kind: MarkupKind, quads: &[[f32; 4]]) -> Option<u64> {
        let pts = box_points(quads.iter().flat_map(|q| [[q[0], q[1]], [q[2], q[3]]]));
        self.add_stroke(page, style, pts, None, None, Some(kind))
    }

    /// Add a text box: `outline` is its rectangle (see [`TextBlock`]), `style.rgba` the text's colour.
    pub fn add_text(&mut self, page: u32, style: Style, text: TextBlock, outline: Vec<P>) -> Option<u64> {
        self.add_stroke(page, style, outline, None, Some(text), None)
    }

    /// Add a shape, drawn from `figure`, so that its anchors can be edited later.
    pub fn add_figure(&mut self, page: u32, style: Style, figure: Figure) -> Option<u64> {
        let pts = figure.points();
        self.add_stroke(page, style, pts, Some(figure), None, None)
    }

    fn add_stroke(
        &mut self,
        page: u32,
        style: Style,
        pts: Vec<P>,
        figure: Option<Figure>,
        text: Option<TextBlock>,
        markup: Option<MarkupKind>,
    ) -> Option<u64> {
        let pts = sanitize(pts)?;
        let id = self.take_id();
        let stroke = Stroke::new(id, page, style, pts, figure, text, markup);
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

    /// Carry the strokes through an edit of the pages as one undo step: those on a page that goes are taken out (and
    /// kept in the step, for its undoing), the others go with their pages and keep their place on them. `undo` is the
    /// edit that takes the pages back, `forward` and `backward` what each does to the pages (see [`Remap`]).
    /// The step is the one the history holds, so older steps need no change: they are undone only after this one is,
    /// when the pages are numbered as they were.
    pub fn edit_pages(&mut self, edit: PageEdit, undo: PageEdit, forward: Remap, backward: Remap) {
        let made = self.copy_strokes(&forward);
        let buried = self.remap_strokes(&forward);
        self.strokes.extend(made.iter().cloned());
        self.record(Cmd::Pages(Box::new(PageStep { edit, undo, forward, backward, buried, made })));
        self.structural = self.revision;
    }

    /// The names of the graves of deletions that can no longer be undone, because the history has let go of their steps: what
    /// was kept for them can go. Taken, so it is given once.
    pub fn take_evicted_graves(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.evicted)
    }

    /// The edit of the pages that the last undo or redo was, if it was one: what the rest of the document (the page
    /// table, the links, the view) must now be given. Taken, so it is given once.
    pub fn take_page_edit(&mut self) -> Option<PageEdit> {
        self.page_edit.take()
    }

    /// Copies of the strokes on the pages that `remap` copies, each on its copy and in the same place on it (with an id of
    /// its own), before the pages have their new places.
    fn copy_strokes(&mut self, remap: &Remap) -> Vec<Stroke> {
        let mut made = Vec::new();
        for copy in remap.copies() {
            let sources: Vec<Stroke> = self.strokes.iter().filter(|s| s.page as usize == copy.from).cloned().collect();
            for mut stroke in sources {
                stroke.translate(copy.dx, copy.dy);
                stroke.page = copy.page as u32;
                stroke.id = self.take_id();
                made.push(stroke);
            }
        }
        made
    }

    /// The strokes on pages that `remap` drops come out (with their places in the drawing order) and the others take
    /// the pages' new numbers and places; the dirty pages follow, and the pages the edit touches are dirty.
    fn remap_strokes(&mut self, remap: &Remap) -> Vec<(usize, Stroke)> {
        let mut buried = Vec::new();
        for (i, mut s) in std::mem::take(&mut self.strokes).into_iter().enumerate() {
            match remap.moved(s.page as usize) {
                Some(m) => {
                    s.page = m.page as u32;
                    s.translate(m.dx, m.dy);
                    self.strokes.push(s);
                }
                None => buried.push((i, s)),
            }
        }
        self.dirty = self.dirty.iter().filter_map(|&p| remap.page(p as usize)).map(|p| p as u32).collect();
        self.dirty.extend(remap.touched().into_iter().map(|p| p as u32));
        buried
    }

    /// Nothing before the last page that was deleted can be undone once the file is written without it: the page
    /// cannot be put back into a file that no longer has it.
    pub fn forget_deleted_pages(&mut self) {
        let deleted = |cmd: &Cmd| matches!(cmd, Cmd::Pages(step) if matches!(step.edit, PageEdit::Delete { .. }));
        if let Some(last) = self.undo.iter().rposition(deleted) {
            self.undo.drain(..=last);
        }
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
        let after = Stroke::new(id, old.page, style, pts, figure, text, None);
        let cmd = Cmd::Reshape { before: Box::new(old.clone()), after: Box::new(after) };
        self.apply(&cmd);
        self.record(cmd);
        true
    }

    /// Add copies of strokes (a paste, a duplicate) as one undo step, each with an id of its own, at the end of
    /// the drawing order and on the page it says. Returns the new ids.
    pub fn add_copies(&mut self, copies: Vec<Stroke>) -> Vec<u64> {
        let mut added: Vec<Stroke> = Vec::new();
        for s in copies {
            let Some(pts) = sanitize(s.pts) else { continue };
            let id = self.take_id();
            added.push(Stroke::new(id, s.page, s.style, pts, s.figure, s.text, s.markup));
        }
        if added.is_empty() {
            return Vec::new();
        }
        for s in &added {
            self.dirty.insert(s.page);
            self.strokes.push(s.clone());
        }
        let ids = added.iter().map(|s| s.id).collect();
        self.record(Cmd::Add(added));
        ids
    }

    /// Change several strokes at once (a new colour, thickness or text size) as one undo step. Strokes that would
    /// not change are left out; `false` if none would. With `coalesce`, a change to strokes that the last step
    /// already changed this way (a slider being dragged) is added to that step rather than being one of its own.
    pub fn replace_many(&mut self, items: Vec<Replacement>, coalesce: bool) -> bool {
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for it in items {
            let Some(pts) = sanitize(it.pts) else { continue };
            let Some(old) = self.strokes.iter().find(|s| s.id == it.id) else { continue };
            if old.pts == pts && old.style == it.style && old.text == it.text {
                continue;
            }
            after.push(Stroke::new(it.id, old.page, it.style, pts, it.figure, it.text, old.markup));
            before.push(old.clone());
        }
        if after.is_empty() {
            return false;
        }
        let continues =
            coalesce && matches!(self.undo.last(), Some(Cmd::Restyle { after: last, .. }) if after.iter().all(|s| last.iter().any(|l| l.id == s.id)));
        if continues {
            if let Some(Cmd::Restyle { after: last, .. }) = self.undo.last_mut() {
                for s in &after {
                    if let Some(slot) = last.iter_mut().find(|l| l.id == s.id) {
                        *slot = s.clone();
                    }
                }
            }
            after.iter().for_each(|s| self.replace(s));
            self.redo.clear();
            self.revision += 1;
            self.structural = self.revision;
            return true;
        }
        let cmd = Cmd::Restyle { before, after };
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
            Cmd::Restyle { after, .. } => after.iter().for_each(|s| self.replace(s)),
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
            Cmd::Pages(step) => {
                self.remap_strokes(&step.forward);
                self.strokes.extend(step.made.iter().cloned());
                self.page_edit = Some(step.edit.clone());
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
            Cmd::Restyle { before, .. } => before.iter().for_each(|s| self.replace(s)),
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
            Cmd::Pages(step) => {
                self.remap_strokes(&step.backward);
                // Ascending positions, so each insert lands where the stroke was.
                for (i, s) in &step.buried {
                    self.dirty.insert(s.page);
                    self.strokes.insert((*i).min(self.strokes.len()), s.clone());
                }
                self.page_edit = Some(step.undo.clone());
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
    #[cfg(test)]
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

/// The corners of the boxes of a markup that can be used: both corners of a box are finite, and a point that is left over is dropped.
fn box_points(pts: impl IntoIterator<Item = P>) -> Vec<P> {
    let pts: Vec<P> = pts.into_iter().collect();
    pts.as_chunks::<2>().0.iter().filter(|c| c.iter().all(|p| p[0].is_finite() && p[1].is_finite())).flatten().copied().collect()
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
    use crate::view::Layout;

    const BLACK: Style = Style { rgba: [0, 0, 0, 255], width: 2.0 };

    fn line(y: f32) -> Vec<P> {
        vec![[0.0, y], [10.0, y], [20.0, y]]
    }
    fn marked(store: &mut InkStore, page: u32, kind: MarkupKind, boxes: &[[f32; 4]]) -> u64 {
        store.add_markup(page, Style { rgba: [255, 235, 0, 110], width: 1.0 }, kind, boxes).expect("markup")
    }

    #[test]
    fn a_markup_is_a_stroke_of_boxes_that_is_hit_inside_them_and_not_between_them() {
        let mut store = InkStore::new();
        let id = marked(&mut store, 0, MarkupKind::Highlight, &[[10.0, 10.0, 60.0, 24.0], [10.0, 40.0, 40.0, 54.0]]);
        let s = store.get(id).expect("get");
        assert_eq!(s.markup, Some(MarkupKind::Highlight));
        assert_eq!(s.quads().collect::<Vec<_>>(), [[10.0, 10.0, 60.0, 24.0], [10.0, 40.0, 40.0, 54.0]]);
        assert_eq!(store.hit_test([30.0, 17.0], 1.0), Some(id), "inside a box");
        assert_eq!(store.hit_test([30.0, 33.0], 1.0), None, "between the two boxes");
        assert_eq!(store.touched_by_segment([30.0, 0.0], [30.0, 100.0], 1.0), vec![id], "an eraser's line across it");
        assert!(store.touched_by_segment([100.0, 0.0], [100.0, 100.0], 1.0).is_empty());
        assert_eq!(store.in_polygon(&[[0.0, 0.0], [80.0, 0.0], [80.0, 60.0], [0.0, 60.0]]), vec![id], "a lasso round it");
        assert!(store.in_polygon(&[[200.0, 200.0], [300.0, 200.0], [300.0, 300.0]]).is_empty());
        assert_eq!(store.bounds_of(&[id]).map(|b| (b[0], b[1])), Some((9.5, 9.5)));
        // A stroke that is not a markup has no boxes.
        let plain = store.add(0, BLACK, line(5.0)).expect("stroke");
        assert_eq!(store.get(plain).expect("get").quads().count(), 0);
    }

    #[test]
    fn a_markup_can_be_moved_erased_copied_restyled_and_undone_like_any_stroke() {
        let mut store = InkStore::new();
        let id = marked(&mut store, 0, MarkupKind::Underline, &[[10.0, 10.0, 60.0, 24.0]]);
        store.mark_clean();
        assert_eq!(store.move_by(&[id], 5.0, 7.0, Some(1)), 1);
        let moved = store.get(id).expect("get");
        assert_eq!((moved.page, moved.quads().next()), (1, Some([15.0, 17.0, 65.0, 31.0])));
        assert!(store.dirty_pages().contains(&0) && store.dirty_pages().contains(&1));
        assert!(store.undo());
        assert_eq!(store.get(id).expect("get").quads().next(), Some([10.0, 10.0, 60.0, 24.0]));
        // Copies keep the kind.
        let copies: Vec<Stroke> = store.strokes().iter().map(|s| s.moved(100.0, 0.0, 0)).collect();
        let ids = store.add_copies(copies);
        assert_eq!(store.get(ids[0]).expect("copy").markup, Some(MarkupKind::Underline));
        // A new colour keeps it a markup of the same boxes.
        let red = Replacement {
            id,
            style: Style { rgba: [255, 0, 0, 255], width: 1.0 },
            pts: store.get(id).expect("get").pts.clone(),
            figure: None,
            text: None,
        };
        assert!(store.replace_many(vec![red], false));
        let after = store.get(id).expect("get");
        assert_eq!((after.markup, after.style.rgba), (Some(MarkupKind::Underline), [255, 0, 0, 255]));
        assert!(store.undo());
        assert_eq!(store.get(id).expect("get").style.rgba, [255, 235, 0, 110]);
        // Erased and brought back.
        assert_eq!(store.remove(&[id]), 1);
        assert!(store.undo());
        assert_eq!(store.get(id).expect("back").markup, Some(MarkupKind::Underline));
    }

    #[test]
    fn a_markup_without_boxes_is_refused_and_one_read_from_a_file_keeps_its_kind() {
        let mut store = InkStore::new();
        assert!(store.add_markup(0, BLACK, MarkupKind::Highlight, &[]).is_none());
        assert!(store.add_markup(0, BLACK, MarkupKind::Highlight, &[[f32::NAN, 0.0, 1.0, 1.0]]).is_none());
        store.load(vec![(2, BLACK, vec![[0.0, 0.0], [30.0, 12.0]], None, None, Some(MarkupKind::StrikeOut))]);
        assert_eq!(store.strokes()[0].markup, Some(MarkupKind::StrikeOut));
        assert_eq!(MarkupKind::from_name("squiggly"), Some(MarkupKind::Squiggly));
        assert!(MarkupKind::from_name("wavy").is_none());
        assert!(MarkupKind::ALL.iter().all(|k| MarkupKind::from_name(k.name()) == Some(*k)));
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

    fn thicker(store: &InkStore, id: u64, width: f32) -> Replacement {
        let s = store.get(id).expect("stroke");
        Replacement { id, style: Style { width, ..s.style }, pts: s.pts.clone(), figure: None, text: None }
    }

    #[test]
    fn a_run_of_restyles_is_one_undo_step_and_a_separate_one_is_its_own() {
        let mut store = InkStore::new();
        let (a, b) = (store.add(0, BLACK, line(0.0)).expect("a"), store.add(0, BLACK, line(30.0)).expect("b"));
        let width = |store: &InkStore, id| store.get(id).map(|s| s.style.width);
        // A drag: the first change is a step, the rest join it, whatever width they reach.
        for (i, w) in [3.0, 4.0, 5.0, 6.0].into_iter().enumerate() {
            let items = vec![thicker(&store, a, w), thicker(&store, b, w)];
            assert!(store.replace_many(items, i > 0));
        }
        assert_eq!((width(&store, a), width(&store, b)), (Some(6.0), Some(6.0)));
        assert!(store.undo());
        assert_eq!((width(&store, a), width(&store, b)), (Some(2.0), Some(2.0)), "one undo for the whole drag");
        assert!(store.redo());
        assert_eq!(width(&store, a), Some(6.0), "and redo does it all again");
        // Not continuing: a change of its own, which undo takes back alone.
        let items = vec![thicker(&store, a, 9.0), thicker(&store, b, 9.0)];
        assert!(store.replace_many(items, false));
        assert!(store.undo());
        assert_eq!(width(&store, a), Some(6.0));
        // Nothing changes: no step.
        let items = vec![thicker(&store, a, 6.0)];
        assert!(!store.replace_many(items, false));
        // Another stroke than the last step changed is not part of it.
        let items = vec![thicker(&store, a, 7.0)];
        assert!(store.replace_many(items, true));
        let c = store.add(0, BLACK, line(60.0)).expect("c");
        let items = vec![thicker(&store, c, 8.0)];
        assert!(store.replace_many(items, true));
        assert!(store.undo());
        assert_eq!((width(&store, c), width(&store, a)), (Some(2.0), Some(7.0)));
    }

    #[test]
    fn copies_are_added_as_one_step_with_ids_of_their_own() {
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, line(0.0)).expect("a");
        let copy = store.get(a).expect("a").moved(5.0, 7.0, 1);
        let ids = store.add_copies(vec![copy.clone(), copy]);
        assert_eq!(ids.len(), 2);
        assert!(!ids.contains(&a) && ids[0] != ids[1]);
        assert_eq!(store.get(ids[0]).map(|s| (s.page, s.pts[0])), Some((1, [5.0, 7.0])));
        assert!(store.undo());
        assert_eq!(store.len(), 1, "both copies at once");
        assert!(store.redo());
        assert_eq!(store.len(), 3);
        assert!(store.add_copies(Vec::new()).is_empty());
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
        store.load(vec![
            (3, BLACK, line(0.0), None, None, None),
            (3, BLACK, vec![], None, None, None),
            (4, BLACK, vec![[f32::NAN, 0.0]], None, None, None),
        ]);
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
    // ── the pages edited ─────────────────────────────────────────────────────────────────────

    /// A document of `n` pages, 600 x 800.
    fn pages(n: usize) -> Layout {
        Layout::new(&vec![[600.0, 800.0]; n])
    }

    /// A stroke `y` down on page `page` of `layout`.
    fn on_page(layout: &Layout, page: usize, y: f32) -> Vec<P> {
        let r = layout.page(page).expect("page");
        vec![[r.x + 10.0, r.y + y], [r.x + 60.0, r.y + y + 20.0]]
    }

    /// Edit the pages as the editor does: the step goes into the history, and the layout after it comes back.
    fn edit(store: &mut InkStore, before: &Layout, edit: PageEdit) -> Layout {
        let after = before.edited(&edit).expect("layout");
        let undo = edit.inverse(before).expect("inverse");
        let forward = Remap::new(&edit, before, &after).expect("remap");
        let backward = Remap::new(&undo, &after, before).expect("remap back");
        store.edit_pages(edit, undo, forward, backward);
        after
    }

    fn same(a: &[P], b: &[P]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (p[0] - q[0]).abs() < 1e-2 && (p[1] - q[1]).abs() < 1e-2)
    }

    #[test]
    fn a_page_inserted_pushes_the_strokes_from_that_page_on_down_with_their_pages() {
        let layout = pages(3);
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, on_page(&layout, 0, 10.0)).expect("a");
        let b = store.add(1, BLACK, on_page(&layout, 1, 10.0)).expect("b");
        let c = store.add(2, BLACK, on_page(&layout, 2, 10.0)).expect("c");
        store.mark_clean();
        let after = edit(&mut store, &layout, PageEdit::Insert { at: 1, size: [600.0, 800.0] });
        let (sa, sb, sc) = (store.get(a).expect("a"), store.get(b).expect("b"), store.get(c).expect("c"));
        assert_eq!((sa.page, sb.page, sc.page), (0, 2, 3));
        assert!(same(&sa.pts, &on_page(&layout, 0, 10.0)), "before the new page: untouched");
        assert!(same(&sb.pts, &on_page(&after, 2, 10.0)), "after it: down by the page, in the same place on it");
        assert!(same(&sc.pts, &on_page(&after, 3, 10.0)));
        assert!((sb.bbox()[1] - (after.page(2).expect("page").y + 9.0)).abs() < 1e-2, "the bounding box moved too");
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![1], "the new page is to be written, and only it");
    }

    #[test]
    fn a_deleted_page_takes_its_strokes_and_the_later_pages_move_up_with_theirs() {
        let layout = pages(3);
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, on_page(&layout, 0, 10.0)).expect("a");
        let b = store.add(1, BLACK, on_page(&layout, 1, 10.0)).expect("b");
        let c = store.add(2, BLACK, on_page(&layout, 2, 10.0)).expect("c");
        let after = edit(&mut store, &layout, PageEdit::delete(1));
        assert!(store.get(b).is_none(), "the page's own ink is gone");
        let (sa, sc) = (store.get(a).expect("a"), store.get(c).expect("c"));
        assert_eq!((sa.page, sc.page), (0, 1));
        assert!(same(&sa.pts, &on_page(&layout, 0, 10.0)), "before the deleted page: untouched");
        assert!(same(&sc.pts, &on_page(&after, 1, 10.0)), "after it: up by the page");
    }

    #[test]
    fn undoing_a_delete_brings_back_the_pages_ink_in_its_place_in_the_drawing_order_and_redo_takes_it_away_again() {
        let layout = pages(3);
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, on_page(&layout, 0, 10.0)).expect("a");
        let b = store.add(1, BLACK, on_page(&layout, 1, 10.0)).expect("b");
        let c = store.add(2, BLACK, on_page(&layout, 2, 10.0)).expect("c");
        let d = store.add(1, BLACK, on_page(&layout, 1, 200.0)).expect("d");
        let order: Vec<u64> = store.strokes().iter().map(|s| s.id).collect();
        edit(&mut store, &layout, PageEdit::Delete { pages: vec![1], grave: 3 });
        assert_eq!(store.len(), 2);
        assert!(store.undo());
        assert_eq!(store.strokes().iter().map(|s| s.id).collect::<Vec<_>>(), order, "back in the order they were drawn");
        for (id, page) in [(a, 0), (b, 1), (c, 2), (d, 1)] {
            assert_eq!(store.get(id).expect("stroke").page, page);
        }
        assert!(same(&store.get(c).expect("c").pts, &on_page(&layout, 2, 10.0)), "the later page is back down where it was");
        assert!(same(&store.get(d).expect("d").pts, &on_page(&layout, 1, 200.0)));
        assert_eq!(
            store.take_page_edit(),
            Some(PageEdit::Restore { grave: 3, pages: vec![(1, [600.0, 800.0])] }),
            "the rest of the document is told what to do"
        );
        assert_eq!(store.take_page_edit(), None, "once");
        assert!(store.redo());
        assert!(store.get(b).is_none() && store.get(d).is_none() && store.get(c).expect("c").page == 1);
        assert_eq!(store.take_page_edit(), Some(PageEdit::Delete { pages: vec![1], grave: 3 }));
    }

    #[test]
    fn page_edits_are_steps_of_the_one_history_older_steps_wait_for_them() {
        let layout = pages(2);
        let mut store = InkStore::new();
        let a = store.add(1, BLACK, on_page(&layout, 1, 10.0)).expect("a");
        store.remove(&[a]);
        let after = edit(&mut store, &layout, PageEdit::Insert { at: 0, size: [600.0, 800.0] });
        let b = store.add(2, BLACK, on_page(&after, 2, 10.0)).expect("b");
        assert!(store.undo(), "the newest step first");
        assert!(store.get(b).is_none() && store.take_page_edit().is_none());
        assert!(store.undo(), "then the page that was inserted");
        assert!(matches!(store.take_page_edit(), Some(PageEdit::Delete { .. })));
        assert!(store.undo(), "and only then the erasing, on the pages as they were");
        let back = store.get(a).expect("the erased stroke is back");
        assert_eq!(back.page, 1);
        assert!(same(&back.pts, &on_page(&layout, 1, 10.0)), "where it was");
        assert!(store.redo() && store.redo() && store.redo());
        assert_eq!(store.get(b).expect("b").page, 2);
        assert!(store.get(a).is_none());
    }

    #[test]
    fn rearranged_pages_carry_their_ink_and_undoing_puts_it_all_back() {
        let layout = Layout::new(&[[600.0, 800.0], [300.0, 400.0], [500.0, 700.0]]);
        let mut store = InkStore::new();
        let ids: Vec<u64> = (0..3_usize).map(|p| store.add(p as u32, BLACK, on_page(&layout, p, 10.0)).expect("stroke")).collect();
        store.mark_clean();
        let after = edit(&mut store, &layout, PageEdit::Move { pages: vec![2], to: 0 });
        for (id, page, was) in [(ids[2], 0_usize, 2_usize), (ids[0], 1, 0), (ids[1], 2, 1)] {
            let s = store.get(id).expect("stroke");
            assert_eq!(s.page as usize, page);
            let (on, rect) = (after.page(page).expect("page"), layout.page(was).expect("page"));
            let want: Vec<P> = on_page(&layout, was, 10.0).iter().map(|p| [p[0] - rect.x + on.x, p[1] - rect.y + on.y]).collect();
            assert!(same(&s.pts, &want), "page {was} is page {page} now and its ink is in the same place on it");
        }
        assert!(store.is_dirty(), "a change to write");
        assert!(store.undo());
        for (p, id) in ids.iter().enumerate() {
            let s = store.get(*id).expect("stroke");
            assert_eq!(s.page as usize, p);
            assert!(same(&s.pts, &on_page(&layout, p, 10.0)));
        }
    }

    #[test]
    fn deleting_pages_keeps_the_dirty_pages_straight_and_leaves_something_to_write() {
        let layout = pages(5);
        let mut store = InkStore::new();
        store.add(0, BLACK, on_page(&layout, 0, 10.0)).expect("a");
        store.add(1, BLACK, on_page(&layout, 1, 10.0)).expect("b");
        store.add(3, BLACK, on_page(&layout, 3, 10.0)).expect("d");
        store.mark_dirty(4);
        let before = store.revision();
        edit(&mut store, &layout, PageEdit::delete(1));
        // Page 1 is gone, the pages after it have a number less: 0 stays, 3 becomes 2, 4 becomes 3; the page that took
        // its place is to be written, so that the file loses the page.
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        assert!(store.revision() > before && !store.only_appended_since(before), "drawn afresh, not just appended to");
        // Deleting the last page: the page now last is written.
        let mut store = InkStore::new();
        edit(&mut store, &pages(3), PageEdit::delete(2));
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn once_the_file_is_written_without_a_deleted_page_it_cannot_be_put_back_but_newer_steps_still_undo() {
        let layout = pages(3);
        let mut store = InkStore::new();
        store.add(0, BLACK, on_page(&layout, 0, 10.0)).expect("early");
        let after = edit(&mut store, &layout, PageEdit::delete(1));
        let late = store.add(1, BLACK, on_page(&after, 1, 10.0)).expect("late");
        edit(&mut store, &after, PageEdit::Insert { at: 2, size: [600.0, 800.0] });
        store.forget_deleted_pages();
        assert!(store.undo() && store.undo(), "the insert and the stroke drawn after the delete");
        assert!(store.get(late).is_none());
        assert!(!store.can_undo(), "the delete, and what came before it, are not for undoing any more");
        // Pages that were only added or moved can always be undone: the file has them.
        let mut store = InkStore::new();
        edit(&mut store, &layout, PageEdit::Insert { at: 1, size: [600.0, 800.0] });
        store.forget_deleted_pages();
        assert!(store.can_undo());
    }

    #[test]
    fn duplicated_pages_have_copies_of_the_ink_that_undo_takes_away_and_redo_brings_back_as_the_same_strokes() {
        let layout = pages(3);
        let mut store = InkStore::new();
        let a = store.add(0, BLACK, on_page(&layout, 0, 10.0)).expect("a");
        let b = store.add(1, BLACK, on_page(&layout, 1, 10.0)).expect("b");
        let c = store.add(1, BLACK, on_page(&layout, 1, 200.0)).expect("c");
        store.mark_clean();
        let after = edit(&mut store, &layout, PageEdit::Duplicate { pages: vec![1] });
        assert_eq!(store.len(), 5, "the two strokes of page 1 are copied");
        let copies: Vec<&Stroke> = store.strokes().iter().filter(|s| s.page == 2).collect();
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|s| s.id != a && s.id != b && s.id != c), "each has an id of its own");
        assert!(
            same(&copies[0].pts, &on_page(&after, 2, 10.0)) && same(&copies[1].pts, &on_page(&after, 2, 200.0)),
            "in the same places on the copy"
        );
        assert_eq!(store.get(b).expect("b").page, 1, "the originals stay where they are");
        assert_eq!(store.get(c).expect("c").page, 1);
        assert_eq!(store.dirty_pages().iter().copied().collect::<Vec<_>>(), vec![2], "the copy is to be written, and only it");
        let ids: Vec<u64> = copies.iter().map(|s| s.id).collect();
        assert!(store.undo());
        assert_eq!(store.len(), 3, "the copies are gone");
        assert!(matches!(store.take_page_edit(), Some(PageEdit::Delete { pages, .. }) if pages == vec![2]));
        assert!(store.redo());
        assert_eq!(store.strokes().iter().filter(|s| s.page == 2).map(|s| s.id).collect::<Vec<_>>(), ids, "the same strokes, with the same ids");
        // A page after the copied one moves down with its ink.
        let mut store = InkStore::new();
        let last = store.add(2, BLACK, on_page(&layout, 2, 10.0)).expect("last");
        let after = edit(&mut store, &layout, PageEdit::Duplicate { pages: vec![0] });
        assert_eq!(store.get(last).expect("last").page, 3);
        assert!(same(&store.get(last).expect("last").pts, &on_page(&after, 3, 10.0)));
    }

    #[test]
    fn a_deletion_the_history_lets_go_of_gives_up_its_grave() {
        let mut store = InkStore::new();
        let mut layout = pages(2);
        let mut graves = 0;
        // A page added and a page deleted, over and over: far more steps than the history keeps.
        for round in 0..(MAX_HISTORY as u64) {
            layout = edit(&mut store, &layout, PageEdit::Insert { at: 1, size: [600.0, 800.0] });
            layout = edit(&mut store, &layout, PageEdit::Delete { pages: vec![1], grave: round + 1 });
            graves += 1;
        }
        let forgotten = store.take_evicted_graves();
        assert_eq!(forgotten.len(), graves - MAX_HISTORY / 2, "the deletions among the steps that went");
        assert_eq!(forgotten.first(), Some(&1), "the oldest first");
        assert!(store.take_evicted_graves().is_empty(), "given once");
        // And the ones still in the history are not forgotten.
        assert!(forgotten.iter().all(|g| *g <= (graves - MAX_HISTORY / 2) as u64));
    }
}
