//! Drawing a polyline or a spline by tapping, with the shape tool set to a line or a curve.
//!
//! Tap with the stylus on empty page to start a stroke; every further tap adds a node, joined to the one
//! before by a straight segment (a polyline) or a smooth curve (a spline). A double tap ends the stroke; a
//! double tap near the first node closes it into a polygon (or a closed curve). While it is being drawn the
//! stroke is only a draft, shown with the next segment following the pen as it hovers. Ending it makes one
//! undo step. Undo while drawing takes back the last node.
//!
//! (A drag from one point to another still makes a two-node line or curve straight away.)

use crate::figure::Figure;
use crate::geometry::Shape;
use crate::ink::{P, Style, dist2};
use crate::input::Sample;
use crate::plane::clamp_into;
use crate::workspace::Workspace;

use super::super::paint::{Painter, fill_square, polyline};
use super::{Gesture, ShapeTool};

/// Two taps this close in time (milliseconds) are a double tap.
const DOUBLE_TAP_MS: f64 = 450.0;
/// How near the second tap of a double tap must be to the first, and how near to the first node it closes
/// the stroke, in logical pixels.
const REACH_PX: f32 = 14.0;
/// A press that moves less than this, in logical pixels, is a tap.
const TAP_PX: f32 = 4.0;

/// A polyline or spline being drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub page: u32,
    pub nodes: Vec<P>,
    /// A spline (the curve tool) rather than straight lines.
    pub curve: bool,
    /// Where the pen hovers, for the segment that follows it.
    pub hover: Option<P>,
    /// When and where the last tap was, for telling a double tap.
    last_tap: Option<(f64, P)>,
}

impl ShapeTool {
    /// Start a stroke at `at` on `page`.
    pub(super) fn begin_draft(&mut self, ws: &mut Workspace, page: u32, at: P) {
        ws.selection.clear();
        self.anchor = None;
        self.draft = Some(Draft { page, nodes: vec![at], curve: ws.styles.shape == Shape::Curve, hover: None, last_tap: None });
        ws.redraw = true;
    }

    /// The pen moved while pressed: where it is now, for the node it will add.
    pub(super) fn drag_draft(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        let Some(s) = samples.last() else { return };
        let p = ws.to_doc(s.pos);
        if let Gesture::DraftTap { last, t_ms, .. } = &mut self.gesture {
            *last = p;
            *t_ms = s.t_ms;
        }
        self.hover_draft(ws, Some(s.pos));
    }

    /// The pen hovers (or, with `None`, has gone): the segment after the last node follows it.
    pub(super) fn hover_draft(&mut self, ws: &mut Workspace, pos: Option<[f32; 2]>) {
        let Some(page) = self.draft.as_ref().map(|d| d.page) else { return };
        let rect = ws.page_rect(page);
        let hover = pos.map(|pos| ws.to_doc(pos)).zip(rect).map(|(p, r)| clamp_into(r, p)).filter(|p| p[0].is_finite() && p[1].is_finite());
        if let Some(draft) = &mut self.draft
            && draft.hover != hover
        {
            draft.hover = hover;
            ws.redraw = true;
        }
    }

    /// The pen lifted after pressing at `down` and ending at `last`, at time `t_ms`. A tap adds a node (or,
    /// the second of a double tap, ends the stroke); a drag adds one where it ended.
    pub(super) fn draft_tap(&mut self, ws: &mut Workspace, down: P, last: P, t_ms: f64) {
        let Some(page) = self.draft.as_ref().map(|d| d.page) else { return };
        let Some(rect) = ws.page_rect(page) else {
            self.draft = None;
            return;
        };
        let tap = dist2(down, last).sqrt() * ws.view.zoom < TAP_PX * ws.dpr;
        let raw = clamp_into(rect, if tap { down } else { last });
        // (A press the window reported at no place, a NaN, is no node: a clamp lets it through.)
        if !(raw[0].is_finite() && raw[1].is_finite()) {
            return;
        }
        let reach = REACH_PX * ws.dpr / ws.view.zoom;
        let double = tap
            && self
                .draft
                .as_ref()
                .and_then(|d| d.last_tap)
                .is_some_and(|(t0, p0)| t_ms >= t0 && t_ms - t0 <= DOUBLE_TAP_MS && dist2(p0, raw) <= reach * reach);
        if double {
            self.end_draft_at(ws, raw);
            return;
        }
        // Nodes snap to the anchors of other shapes.
        let node = self.snapped(ws, raw, None);
        self.snap = None;
        if let Some(draft) = &mut self.draft {
            draft.nodes.push(node);
            draft.last_tap = tap.then_some((t_ms, raw));
        }
        ws.redraw = true;
    }

    /// A double tap at `at` ends the stroke: closed if that is near the first node (and the node the first
    /// tap of the double tap added is taken away again, for it was the closing tap), else open.
    fn end_draft_at(&mut self, ws: &mut Workspace, at: P) {
        let Some(mut draft) = self.draft.take() else { return };
        let reach = REACH_PX * ws.dpr / ws.view.zoom;
        let close = draft.nodes.len() >= 4 && draft.nodes.first().is_some_and(|&first| dist2(at, first) <= reach * reach);
        if close {
            draft.nodes.pop();
        }
        // A stroke that never got further than a tap's reach from where it began is no stroke (a double tap
        // on the first node, to cancel).
        let first = draft.nodes.first().copied().unwrap_or([0.0; 2]);
        if draft.nodes.iter().all(|&n| dist2(n, first) <= reach * reach) {
            ws.redraw = true;
            return;
        }
        self.commit_draft(ws, &draft, close);
    }

    /// End the stroke as it is (open): the tool changed, the file was saved, a page was added.
    pub fn finish_draft(&mut self, ws: &mut Workspace) {
        if let Some(draft) = self.draft.take() {
            self.commit_draft(ws, &draft, false);
        }
    }

    /// Make the figure of `draft` (closed, if it can be), one undo step, selected so its anchors show.
    fn commit_draft(&mut self, ws: &mut Workspace, draft: &Draft, closed: bool) {
        ws.redraw = true;
        let build = |closed| if draft.curve { Figure::spline(&draft.nodes, closed) } else { Figure::polyline(&draft.nodes, closed) };
        let Some(figure) = build(closed).or_else(|| closed.then(|| build(false)).flatten()) else { return };
        let (rgba, width) = ws.current_style();
        if let Some(id) = ws.ink.add_figure(draft.page, Style { rgba, width }, figure) {
            ws.selection = vec![id];
            self.anchor = None;
        }
    }

    /// Undo while a stroke is being drawn takes back its last node. `false` if none is.
    pub(super) fn undo_draft(&mut self, ws: &mut Workspace) -> bool {
        let Some(draft) = &mut self.draft else { return false };
        draft.nodes.pop();
        draft.last_tap = None;
        if draft.nodes.is_empty() {
            self.draft = None;
        }
        ws.redraw = true;
        true
    }

    /// What to draw for the stroke being drawn: its outline (with the segment that follows the pen) and its
    /// nodes.
    pub fn draft_preview(&self) -> Option<(Vec<P>, Vec<P>)> {
        let draft = self.draft.as_ref()?;
        let mut through = draft.nodes.clone();
        if let Some(h) = draft.hover.filter(|h| through.last() != Some(h)) {
            through.push(h);
        }
        let figure = if draft.curve { Figure::spline(&through, false) } else { Figure::polyline(&through, false) };
        Some((figure.map_or_else(|| through.clone(), |f| f.points()), draft.nodes.clone()))
    }

    /// The stroke so far, and a square at each node.
    pub(super) fn paint_draft(&self, ws: &Workspace, out: &mut dyn Painter) {
        let Some((outline, nodes)) = self.draft_preview() else { return };
        let px = ws.dpr / ws.view.zoom;
        let (rgba, width) = ws.current_style();
        polyline(out, &outline, width, ws.on_page(rgba));
        for &n in &nodes {
            fill_square(out, n, 5.0 * px, px, ws.look.theme.selection);
            fill_square(out, n, 3.2 * px, px, ws.look.theme.white);
        }
    }
}
