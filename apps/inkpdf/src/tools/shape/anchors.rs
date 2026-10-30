//! Editing a selected shape by its anchor points and handles, the way an illustration program does. Only the
//! stylus does this, and only with the shape tool.
//!
//! What an anchor does depends on the kind of shape (see [`crate::figure`]):
//!
//! - **Straight lines** (a line, a broken line, a polygon): an anchor for every node. Drag one and the
//!   segments follow, always straight. A node snaps to the anchors of other shapes, and an end dragged onto
//!   the other end of a broken line closes it into a polygon. Double tap the outline to add a node; double
//!   tap an end node to close the line into a polygon, or any node of a polygon to open it there; press a
//!   node with the barrel button to take it away; tap empty page with the barrel button to add a node at
//!   the end that grows (the far end, or the first node if that was pressed last).
//! - **Splines**: anchors with handles (tangents). Drag a handle to bend the segments either side of its
//!   anchor; on a smooth anchor the other handle swings to stay in line, and with the barrel button held the
//!   handle moves alone and the anchor becomes a corner. Double tap an anchor to flip it between a corner
//!   and a smooth point.
//! - **Rectangles**: the two ends of the diagonal. **Ellipses**: the four corners of the bounding
//!   rectangle. **Axes**: the origin, and the end of each axis, which slides along its axis.
//!
//! Each edit is one undo step, made when the pen lifts; while it is down the shape follows the pen without
//! touching the stored ink.

use crate::figure::NodeAction;
use crate::figure::{Control, Figure};
use crate::ink::{P, dist2};
use crate::input::Sample;
use crate::plane::clamp_into;
use crate::workspace::Workspace;

use super::super::paint::{Painter, dashed, fill_square, polyline};
use super::{Gesture, SNAP_PX, ShapeTool};
use crate::tools::{TAP_SELECT_PX, Tool};

/// How near a press must be to an anchor or handle to take hold of it, in logical pixels.
const CONTROL_PX: f32 = 14.0;
/// How near a press must be to an outline to count as on it, in logical pixels.
const PATH_PX: f32 = 8.0;
/// Two presses this close in time (milliseconds) are a double tap.
const DOUBLE_TAP_MS: f64 = 450.0;

/// The stretch of `figure` to put a new point on: the one after the point pressed last, if it has one, else the
/// longest.
fn segment_to_split(figure: &Figure, selected: Option<usize>) -> usize {
    let n = figure.anchor_count();
    let closed = matches!(figure, Figure::Poly { closed: true, .. } | Figure::Path { closed: true, .. });
    let segments = if closed { n } else { n.saturating_sub(1) };
    if let Some(i) = selected.filter(|&i| i < segments) {
        return i;
    }
    let length = |s: usize| match (figure.anchor_pos(s), figure.anchor_pos((s + 1) % n.max(1))) {
        (Some(a), Some(b)) => dist2(a, b),
        _ => 0.0,
    };
    (0..segments).max_by(|&a, &b| length(a).total_cmp(&length(b))).unwrap_or(0)
}

/// What a press landed on, for telling a double tap.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TapOn {
    Control(Control),
    Outline,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TapMark {
    t_ms: f64,
    stroke: u64,
    at: P,
    on: TapOn,
}

impl ShapeTool {
    /// The single selected shape, and the figure to show for it (the one being edited, while an edit is
    /// under way).
    pub fn editing<'a>(&'a self, ws: &'a Workspace) -> Option<(u64, &'a Figure)> {
        let [id] = ws.selection[..] else { return None };
        if let Gesture::Control { stroke, figure, .. } = &self.gesture
            && *stroke == id
        {
            return Some((id, figure));
        }
        ws.ink.get(id)?.figure.as_ref().map(|f| (id, f))
    }

    /// The anchor of the shape being edited that was pressed last, if it is still there.
    pub fn selected_anchor(&self, ws: &Workspace) -> Option<usize> {
        let (id, figure) = self.editing(ws)?;
        self.anchor.filter(|&(s, i)| s == id && i < figure.anchor_count()).map(|(_, i)| i)
    }

    /// Whether the shape being edited has points that can be added, taken away or smoothed (a run of lines or
    /// a spline; a rectangle's corners and an ellipse's are fixed in number).
    pub fn has_nodes(&self, ws: &Workspace) -> bool {
        self.editing(ws).is_some_and(|(_, f)| matches!(f, Figure::Poly { .. } | Figure::Path { .. }))
    }

    /// Take away, add or smooth a point of the shape being edited: the one pressed last, or (to add one) the
    /// longest stretch after it. As one undo step, like the stylus's own way of doing it. `true` if it did.
    pub fn node_action(&mut self, ws: &mut Workspace, action: NodeAction) -> bool {
        let Some((stroke, figure)) = self.editing(ws).map(|(s, f)| (s, f.clone())) else { return false };
        let selected = self.selected_anchor(ws);
        let mut changed = figure;
        let anchor = match action {
            NodeAction::Remove => selected.filter(|&i| changed.remove_anchor(i)).map(|_| None),
            NodeAction::Smooth => selected.filter(|&i| changed.toggle_smooth(i) || changed.toggle_closed(i)).map(Some),
            NodeAction::Add => {
                let segment = segment_to_split(&changed, selected);
                changed.insert_anchor(segment, 0.5).map(Some)
            }
        };
        let Some(anchor) = anchor.filter(|_| changed.valid()) else { return false };
        ws.ink.reshape(stroke, changed);
        self.anchor = anchor.map(|i| (stroke, i));
        ws.redraw = true;
        true
    }

    /// A press at `pos` (screen) at time `t_ms`: if it is on an anchor or handle of the selected shape, take
    /// hold of it (or, on a double tap, convert the anchor or add one). With `barrel` (the barrel button
    /// held) a handle is broken off from its partner and an anchor is taken away. Returns whether the press
    /// was used.
    pub(super) fn control_down(&mut self, ws: &mut Workspace, id: u32, pos: [f32; 2], t_ms: f64, barrel: bool) -> bool {
        let Some((stroke, figure)) = self.editing(ws).map(|(s, f)| (s, f.clone())) else { return false };
        let p = ws.to_doc(pos);
        let radius = CONTROL_PX * ws.dpr / ws.view.zoom;
        if let Some(control) = figure.control_near(p, radius) {
            let double = self.is_double_tap(stroke, TapOn::Control(control), p, t_ms, radius);
            self.tap = Some(TapMark { t_ms, stroke, at: p, on: TapOn::Control(control) });
            let (Control::Anchor(i) | Control::Back(i) | Control::Fwd(i)) = control;
            self.anchor = Some((stroke, i));
            if barrel && let Control::Anchor(i) = control {
                self.tap = None;
                let mut smaller = figure;
                if smaller.remove_anchor(i) {
                    ws.ink.reshape(stroke, smaller);
                    self.anchor = None;
                }
                self.gesture = Gesture::Swallow(id);
            } else if double && let Control::Anchor(i) = control {
                self.tap = None;
                let mut converted = figure;
                // A spline's anchor flips between corner and smooth; the end of a run of straight lines
                // closes it, and a node of a polygon opens it. (Each kind answers to one of them.)
                if converted.toggle_smooth(i) || converted.toggle_closed(i) {
                    ws.ink.reshape(stroke, converted);
                }
                self.gesture = Gesture::Swallow(id);
            } else {
                self.gesture = Gesture::Control { id, stroke, control, figure, independent: barrel };
            }
            ws.redraw = true;
            return true;
        }
        if let Some(on) = figure.nearest_on_path(p).filter(|o| o.distance <= PATH_PX * ws.dpr / ws.view.zoom) {
            if self.is_double_tap(stroke, TapOn::Outline, p, t_ms, radius) {
                self.tap = None;
                let mut split = figure;
                if let Some(i) = split.insert_anchor(on.segment, on.t) {
                    ws.ink.reshape(stroke, split);
                    self.anchor = Some((stroke, i));
                }
                self.gesture = Gesture::Swallow(id);
                ws.redraw = true;
                return true;
            }
            self.tap = Some(TapMark { t_ms, stroke, at: p, on: TapOn::Outline });
        }
        false
    }

    fn is_double_tap(&self, stroke: u64, on: TapOn, p: P, t_ms: f64, radius: f32) -> bool {
        self.tap.is_some_and(|m| {
            m.stroke == stroke && m.on == on && t_ms >= m.t_ms && t_ms - m.t_ms <= DOUBLE_TAP_MS && dist2(m.at, p) <= (1.5 * radius).powi(2)
        })
    }

    /// The pen moved while holding a control.
    pub(super) fn drag_control(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        for s in samples {
            let Gesture::Control { stroke, control, independent, .. } = self.gesture else { return };
            // Whatever is pulled stays on the shape's page.
            let rect = ws.ink.get(stroke).and_then(|s| ws.page_rect(s.page));
            let p = ws.to_doc(s.pos);
            let p = rect.map_or(p, |r| clamp_into(r, p));
            // Only an anchor snaps (to the anchors of other shapes, and an end of a broken line to its
            // other end, which closes it); a handle goes where it is put.
            let mut target = if matches!(control, Control::Anchor(_)) { self.snapped(ws, p, Some(stroke)) } else { p };
            if let Some(other_end) = self.other_end_near(ws, p) {
                self.snap = Some(other_end);
                target = other_end;
            }
            if let Gesture::Control { figure, .. } = &mut self.gesture {
                figure.move_control(control, target, independent);
            }
        }
    }

    /// While an end node of a broken line (four nodes or more) is being dragged: the other end, if `p` is
    /// near it.
    fn other_end_near(&self, ws: &Workspace, p: P) -> Option<P> {
        let Gesture::Control { figure: Figure::Poly { points, closed: false }, control: Control::Anchor(i), .. } = &self.gesture else {
            return None;
        };
        let n = points.len();
        let other = match (n >= 4, *i) {
            (true, 0) => points.get(n - 1),
            (true, i) if i == n - 1 => points.first(),
            _ => None,
        }?;
        let reach = SNAP_PX * ws.dpr / ws.view.zoom;
        (dist2(*other, p) <= reach * reach).then_some(*other)
    }

    /// The barrel button tapped empty page (at `poly`, the little it was drawn over) while a run of
    /// straight lines is selected: add a node there, at the end that was touched last (the far end, unless
    /// the first node was). Returns whether that was what the tap was.
    pub(super) fn lasso_tap(&mut self, ws: &mut Workspace, poly: &[P]) -> bool {
        let Some(&first) = poly.first() else { return false };
        let reach = TAP_SELECT_PX * ws.dpr / ws.view.zoom;
        if poly.iter().any(|&q| dist2(q, first) > reach * reach) {
            return false;
        }
        let Some((stroke, figure)) = self.editing(ws).map(|(s, f)| (s, f.clone())) else { return false };
        if !matches!(figure, Figure::Poly { closed: false, .. }) {
            return false;
        }
        let Some(page) = ws.ink.get(stroke).and_then(|s| ws.page_rect(s.page)) else { return false };
        let at = self.snapped(ws, clamp_into(page, first), Some(stroke));
        self.snap = None;
        let mut longer = figure;
        // The end that grows is the one last touched: the start, if that is the node last pressed.
        let at_start = self.selected_anchor(ws) == Some(0);
        let Some(i) = longer.extend(at, at_start) else { return false };
        ws.ink.reshape(stroke, longer);
        self.anchor = Some((stroke, i));
        ws.redraw = true;
        true
    }

    /// The pen lifted: the edit is made, as one undo step. An end of a broken line brought onto its other
    /// end closes it into a polygon.
    pub(super) fn finish_control(&mut self, ws: &mut Workspace, stroke: u64, mut figure: Figure, control: Control) {
        self.snap = None;
        let changed = ws.ink.get(stroke).and_then(|s| s.figure.as_ref()) != Some(&figure);
        if changed && matches!(control, Control::Anchor(_)) {
            figure.merge_ends();
        }
        if figure.valid() {
            ws.ink.reshape(stroke, figure);
        }
        ws.redraw = true;
    }

    /// What a selected shape shows for editing: its anchor points as small squares (the one pressed last
    /// solid), the handles that exist as circles joined to their anchors, and a marker where a drag has
    /// snapped to another shape's anchor.
    pub(super) fn paint_anchors(&self, ws: &Workspace, out: &mut dyn Painter) {
        let (accent, white) = (ws.look.theme.selection, ws.look.theme.white);
        let px = ws.dpr / ws.view.zoom;
        let offset = self.drag_offset();
        if let Some((_, figure)) = self.editing(ws) {
            let selected = self.selected_anchor(ws);
            let at = |p: P| [p[0] + offset[0], p[1] + offset[1]];
            let guide = [accent[0], accent[1], accent[2], 150];
            let controls = figure.controls();
            // The rectangle's diagonal, the ellipse's bounding box.
            for [a, b] in figure.guides() {
                dashed(out, at(a), at(b), 1.2 * px, guide, 6.0 * px, 4.0 * px);
            }
            // Handles first (their lines, then their dots), so the anchors are drawn over them.
            for &(control, p) in &controls {
                let (Control::Back(i) | Control::Fwd(i)) = control else { continue };
                if let Some(anchor) = figure.anchor_pos(i) {
                    out.doc_line(at(anchor), at(p), 1.2 * px, guide);
                }
            }
            for &(control, p) in &controls {
                match control {
                    Control::Back(_) | Control::Fwd(_) => {
                        out.doc_line(at(p), at(p), 11.0 * px, accent);
                        out.doc_line(at(p), at(p), 7.5 * px, white);
                    }
                    Control::Anchor(i) => {
                        let on = selected == Some(i);
                        fill_square(out, at(p), 5.5 * px, px, accent);
                        fill_square(out, at(p), 3.6 * px, px, if on { accent } else { white });
                    }
                }
            }
        }
        if let Some(target) = self.snap {
            // A ring and a cross where the drag has landed on another shape's anchor.
            let magenta = ws.look.theme.snap;
            let r = 10.0 * px;
            let ring: Vec<P> = (0..=24)
                .map(|i| {
                    let a = i as f32 / 24.0 * std::f32::consts::TAU;
                    [target[0] + r * a.cos(), target[1] + r * a.sin()]
                })
                .collect();
            polyline(out, &ring, 1.6 * px, magenta);
            out.doc_line([target[0] - 5.0 * px, target[1]], [target[0] + 5.0 * px, target[1]], 1.4 * px, magenta);
            out.doc_line([target[0], target[1] - 5.0 * px], [target[0], target[1] + 5.0 * px], 1.4 * px, magenta);
        }
    }
}
