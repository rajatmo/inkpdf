//! The shape tool: a drag from one corner to the other makes a line, a curve, a rectangle, an ellipse or a
//! set of axes when the pen lifts, with the shape following the pen while it is down. The new shape is
//! selected, with its anchor points and handles showing, ready to be edited (see [`anchors`]); a tap on a
//! shape's outline selects it; with the line or curve, taps draw a polyline or spline (see [`draft`]).
//!
//! A shape lies on the page the drag began on and is kept inside it. Its ends snap to the anchor points of
//! other shapes. It is one ordinary stroke (see [`crate::figure`]), so it is erased, selected, moved,
//! undone and saved like handwriting. The stylus does all of it; the mouse scrolls, a finger pans and
//! zooms, the keyboard does nothing.

mod anchors;
mod draft;

use crate::figure::{Control, Figure};
use crate::geometry::{self, Shape, clamp_into};
use crate::ink::{P, Style, dist2};
use crate::input::{Device, Phase, Role, Sample, role};
use crate::workspace::Workspace;

use super::gestures::{Eraser, Lasso, Mover};
use super::paint::{Painter, polyline};
use super::{InputPolicy, PageEvent, TAP_SELECT_PX, Tool};

pub use draft::Draft;

/// A drag shorter than this, in logical pixels, makes no shape: it was a tap.
const MIN_DRAG_PX: f32 = 4.0;
/// How near a drag must come to another shape's anchor to snap to it, in logical pixels.
const SNAP_PX: f32 = 10.0;

#[derive(Default)]
enum Gesture {
    #[default]
    None,
    /// A shape being dragged out from `start` to `end` (both inside `page`).
    Drag {
        id: u32,
        page: u32,
        shape: Shape,
        start: P,
        end: P,
    },
    /// An anchor or handle of the selected shape being moved; `figure` is the shape as it is being reshaped
    /// (the stored one is replaced when the pen lifts), `independent` breaks a handle off from its partner.
    Control {
        id: u32,
        stroke: u64,
        control: Control,
        figure: Figure,
        independent: bool,
    },
    /// The pen is down while a stroke is being drawn by taps: it is at `last` now (a tap if it has not
    /// moved from `down`), at time `t_ms`.
    DraftTap {
        id: u32,
        down: P,
        last: P,
        t_ms: f64,
    },
    Erase(Eraser),
    Lasso(Lasso),
    Move(Mover),
    /// A press that did its work at once (an anchor taken away, a selection let go of); nothing follows.
    Swallow(u32),
}

#[derive(Default)]
pub struct ShapeTool {
    gesture: Gesture,
    /// The polyline or spline being drawn by taps, if any.
    pub(crate) draft: Option<Draft>,
    /// The anchor of the selected shape that was pressed last (the stroke and which anchor), for deleting it
    /// or converting it.
    anchor: Option<(u64, usize)>,
    /// Where a drag has snapped to an anchor of another shape, for the marker shown there.
    snap: Option<P>,
    /// The last press on a shape's anchor or outline, for telling a double tap.
    tap: Option<anchors::TapMark>,
}

impl ShapeTool {
    fn id(&self) -> Option<u32> {
        match &self.gesture {
            Gesture::None => None,
            Gesture::Drag { id, .. } | Gesture::Control { id, .. } | Gesture::DraftTap { id, .. } | Gesture::Swallow(id) => Some(*id),
            Gesture::Erase(e) => Some(e.id),
            Gesture::Lasso(l) => Some(l.id),
            Gesture::Move(m) => Some(m.id),
        }
    }

    /// The shape being dragged out: where it starts and where the pen is.
    #[cfg(test)]
    pub fn dragging(&self) -> Option<(P, P)> {
        match self.gesture {
            Gesture::Drag { start, end, .. } => Some((start, end)),
            _ => None,
        }
    }

    /// An anchor or handle being moved: which, and the shape as it is becoming.
    pub fn controlling(&self) -> Option<(u64, Control, &Figure)> {
        match &self.gesture {
            Gesture::Control { stroke, control, figure, .. } => Some((*stroke, *control, figure)),
            _ => None,
        }
    }

    /// Where a drag has snapped to another shape's anchor.
    #[cfg(test)]
    pub fn snap(&self) -> Option<P> {
        self.snap
    }

    fn down(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        let Some(first) = ev.samples.first() else { return };
        let p = ws.to_doc(first.pos);
        let what = role(&ev.pen);
        // The eraser end and the barrel button end a stroke being drawn by taps.
        if what != Role::Draw {
            self.finish_draft(ws);
        }
        // A selected shape's anchors and handles are grabbed before anything else (the barrel button breaks
        // a handle off from its partner).
        if what != Role::Erase && self.control_down(ws, ev.id, first.pos, first.t_ms, ev.pen.barrel) {
            return;
        }
        match what {
            Role::Draw => {
                if !ws.selection.is_empty() {
                    // Something selected: the pen drags it by its outline; touching elsewhere lets go and
                    // starts a new shape.
                    if self.on_selected_outline(ws, p) {
                        self.gesture = Gesture::Move(Mover::begin(Device::Pen, ev.id, p));
                        return;
                    }
                    ws.selection.clear();
                }
                self.shape_down(ws, ev.id, ev.samples);
            }
            Role::Erase => {
                self.gesture = Gesture::Erase(Eraser::begin(ev.id));
                self.moved(ws, ev.samples);
            }
            Role::Lasso => {
                self.gesture = Gesture::Lasso(Lasso::begin(ev.id, p));
                self.moved(ws, ev.samples);
            }
        }
    }

    /// The pen pressed with the shape tool, and nothing selected.
    fn shape_down(&mut self, ws: &mut Workspace, id: u32, samples: &[Sample]) {
        let Some(first) = samples.first() else { return };
        let p = ws.to_doc(first.pos);
        // Drawing a polyline or spline by taps: this press is another node (or the end of it).
        if self.draft.is_some() {
            self.gesture = Gesture::DraftTap { id, down: p, last: p, t_ms: first.t_ms };
            return;
        }
        self.gesture = match ws.page_hit(p) {
            Some(page) => {
                let start = self.snapped(ws, p, None);
                Gesture::Drag { id, page, shape: ws.shape, start, end: start }
            }
            None => Gesture::Swallow(id),
        };
        self.moved(ws, samples.get(1..).unwrap_or_default());
    }

    fn moved(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        match &mut self.gesture {
            Gesture::Erase(e) => e.drag(ws, samples),
            Gesture::Lasso(l) => l.drag(ws, samples),
            Gesture::Move(m) => {
                if let Some(s) = samples.last() {
                    m.drag(ws, s.pos);
                }
            }
            Gesture::Drag { .. } => self.drag_shape(ws, samples),
            Gesture::Control { .. } => self.drag_control(ws, samples),
            Gesture::DraftTap { .. } => self.drag_draft(ws, samples),
            Gesture::None | Gesture::Swallow(_) => {}
        }
        ws.redraw = true;
    }

    /// The pen moved while a shape is being dragged out.
    fn drag_shape(&mut self, ws: &Workspace, samples: &[Sample]) {
        let Gesture::Drag { page, .. } = self.gesture else { return };
        let Some(rect) = ws.page_rect(page) else { return };
        for s in samples {
            let p = self.snapped(ws, clamp_into(rect, ws.to_doc(s.pos)), None);
            if let Gesture::Drag { end, .. } = &mut self.gesture {
                *end = clamp_into(rect, p);
            }
        }
    }

    /// The pen lifted at the end of a drag. A tap makes no shape: it selects the stroke under it, or lets
    /// go. Anything else is drawn, and selected so its anchors show.
    fn finish_shape(&mut self, ws: &mut Workspace, page: u32, shape: Shape, start: P, end: P) {
        self.snap = None;
        if dist2(start, end).sqrt() * ws.view.zoom < MIN_DRAG_PX * ws.dpr {
            let radius = TAP_SELECT_PX * ws.dpr / ws.view.zoom;
            let hit = ws.ink.hit_test(start, radius);
            // With the line or curve tool a tap that is not on a shape (or is on the end of one, to join it)
            // starts a polyline or spline drawn by taps; otherwise it selects what it is on.
            if matches!(shape, Shape::Line | Shape::Curve) && (hit.is_none() || on_anchor(ws, start)) {
                self.begin_draft(ws, page, start);
                return;
            }
            ws.selection = hit.into_iter().collect();
            return;
        }
        if let Some(figure) = Figure::from_shape(shape, start, end) {
            let (rgba, width) = ws.current_style();
            if let Some(id) = ws.ink.add_figure(page, Style { rgba, width }, figure) {
                ws.selection = vec![id];
                self.anchor = None;
            }
        }
    }

    /// Whether `p` is on the outline of a selected stroke.
    fn on_selected_outline(&self, ws: &Workspace, p: P) -> bool {
        let radius = TAP_SELECT_PX * ws.dpr / ws.view.zoom;
        ws.ink.hit_test(p, radius).is_some_and(|id| ws.selection.contains(&id))
    }

    /// `p`, or the nearest anchor of another shape if one is close enough (and the marker for it).
    fn snapped(&mut self, ws: &Workspace, p: P, skip: Option<u64>) -> P {
        let reach = SNAP_PX * ws.dpr / ws.view.zoom;
        let mut best: Option<(f32, P)> = None;
        for s in ws.ink.strokes() {
            let Some(figure) = &s.figure else { continue };
            let b = s.bbox();
            if Some(s.id) == skip || p[0] < b[0] - reach || p[0] > b[2] + reach || p[1] < b[1] - reach || p[1] > b[3] + reach {
                continue;
            }
            for a in (0..figure.anchor_count()).filter_map(|i| figure.anchor_pos(i)) {
                let d = dist2(a, p);
                if d <= reach * reach && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, a));
                }
            }
        }
        self.snap = best.map(|(_, a)| a);
        best.map_or(p, |(_, a)| a)
    }
}

/// Whether `p` is exactly on an anchor of a shape (a point snapped to one is).
fn on_anchor(ws: &Workspace, p: P) -> bool {
    ws.ink.strokes().iter().filter_map(|s| s.figure.as_ref()).any(|f| (0..f.anchor_count()).filter_map(|i| f.anchor_pos(i)).any(|a| a == p))
}

impl Tool for ShapeTool {
    fn policy(&self) -> InputPolicy {
        InputPolicy { stylus: true, mouse: false, keyboard: false }
    }

    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        match ev.phase {
            Phase::Down => {
                self.end(ws);
                self.down(ws, ev);
            }
            Phase::Move if self.owns(ev.dev, ev.id) => self.moved(ws, ev.samples),
            Phase::Up | Phase::Cancel if self.owns(ev.dev, ev.id) => {
                self.moved(ws, ev.samples);
                self.end(ws);
            }
            Phase::Hover => self.hover_draft(ws, ev.samples.last().map(|s| s.pos)),
            Phase::Leave => self.hover_draft(ws, None),
            _ => {}
        }
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        dev == Device::Pen && self.id() == Some(id)
    }

    fn busy(&self) -> bool {
        self.id().is_some() || self.draft.is_some()
    }

    fn end(&mut self, ws: &mut Workspace) {
        match std::mem::take(&mut self.gesture) {
            Gesture::None | Gesture::Swallow(_) => return,
            Gesture::Drag { page, shape, start, end, .. } => self.finish_shape(ws, page, shape, start, end),
            Gesture::Control { stroke, figure, control, .. } => self.finish_control(ws, stroke, figure, control),
            Gesture::DraftTap { down, last, t_ms, .. } => self.draft_tap(ws, down, last, t_ms),
            Gesture::Erase(_) => Eraser::finish(ws),
            Gesture::Lasso(l) => {
                // (A tap of the barrel button on empty page may be adding a node to a line.)
                if !self.lasso_tap(ws, &l.poly) {
                    Lasso::select(ws, &l.poly);
                }
            }
            Gesture::Move(m) => {
                m.finish(ws);
            }
        }
        ws.redraw = true;
    }

    /// A stroke being drawn by taps becomes part of the document (as it is, open).
    fn settle(&mut self, ws: &mut Workspace) {
        self.finish_draft(ws);
    }

    fn cancel(&mut self, ws: &mut Workspace) {
        ws.erasing.clear();
        self.gesture = Gesture::None;
        self.snap = None;
    }

    fn undo(&mut self, ws: &mut Workspace) -> bool {
        self.undo_draft(ws)
    }

    /// While a stroke is being drawn there is nothing to redo.
    fn redo(&mut self, _ws: &mut Workspace) -> bool {
        self.draft.is_some()
    }

    fn drag_offset(&self) -> [f32; 2] {
        match &self.gesture {
            Gesture::Move(m) => m.offset(),
            _ => [0.0; 2],
        }
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        self.paint_draft(ws, out);
        match &self.gesture {
            Gesture::Drag { shape, start, end, .. } => {
                // Exactly what will be stored, drawn the way it will be drawn.
                let (rgba, width) = ws.current_style();
                polyline(out, &geometry::points(*shape, *start, *end), width, rgba);
            }
            Gesture::Lasso(l) => l.paint(ws, out),
            Gesture::Erase(e) => e.paint(ws, out),
            _ => {}
        }
        self.paint_anchors(ws, out);
    }
}
