//! The pen tool: the stylus's tip draws, its eraser end erases whole strokes, its barrel button draws a
//! lasso (or, with select or erase chosen on screen, the tip does that instead). It works on handwriting:
//! what it erases, selects and pastes is ink, not shapes or text. With something selected the tip drags the
//! selection when it lands on it, and lets go of it when it lands elsewhere (that press does nothing else).
//! The mouse scrolls in the tablet mode, a finger pans and zooms, and at a desk the keyboard works on the
//! selection (Delete, Esc, the arrows).

use crate::ink::{self, P, Style, dist2};
use crate::input::{Device, Mode, Phase, Sample, predict};
use crate::smooth::{PenFilter, curve_segments};
use crate::workspace::Workspace;

use super::gestures::Pick;
use super::paint::{CURVE_STEP, Painter, polyline};
use super::shortcuts::selection_key;
use super::{InputPolicy, Key, PageEvent, Tool};

/// Closest together, in device pixels, two points of a live stroke are kept.
const MIN_POINT_PX: f32 = 0.35;
/// How far ahead of the last pen sample the stroke being drawn is extended, in milliseconds.
const PREDICT_MS: f64 = 12.0;
/// How many recent samples feed the prediction.
const RECENT: usize = 6;

/// A stroke being drawn. `page` is `None` while the pen is off any page. `pts` are the steadied points;
/// `recent` (steadied samples, in pixels) feeds the prediction; `raw_end` is where the pen really was, so a
/// finished stroke ends exactly there.
struct Draw {
    id: u32,
    page: Option<u32>,
    pts: Vec<P>,
    recent: Vec<Sample>,
    filter: PenFilter,
    raw_end: Option<P>,
    /// The smooth curve through the first stable segments of pts, kept as it is made (a segment is final
    /// once the point after its end is not the last), so a frame redoes only the end of a long stroke.
    curve: Vec<P>,
    stable: usize,
}

impl Draw {
    fn new(id: u32, smoothing: f32) -> Self {
        Self {
            id,
            page: None,
            pts: Vec::new(),
            recent: Vec::with_capacity(RECENT + 1),
            filter: PenFilter::new(smoothing),
            raw_end: None,
            curve: Vec::new(),
            stable: 0,
        }
    }

    fn drag(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        let min_gap2 = (MIN_POINT_PX / ws.view.zoom).powi(2);
        for raw in samples {
            // Steady the pen first (in screen pixels, so the feel does not depend on the zoom).
            let steadied = Sample { pos: self.filter.filter(raw.pos, raw.t_ms), ..*raw };
            let p = ws.to_doc(steadied.pos);
            self.raw_end = Some(ws.to_doc(raw.pos));
            if self.recent.len() == RECENT {
                self.recent.remove(0);
            }
            self.recent.push(steadied);
            match (self.page, ws.page_hit(p)) {
                (Some(cur), Some(hit)) if cur == hit => {
                    if self.pts.last().is_none_or(|&last| dist2(last, p) >= min_gap2) {
                        self.pts.push(p);
                    }
                }
                // Left the page, moved onto another, or just started: what there is so far is a stroke of
                // its own, and a new one begins where the pen is.
                (was, hit) => {
                    let done = std::mem::take(&mut self.pts);
                    self.curve.clear();
                    self.stable = 0;
                    if let Some(was) = was {
                        commit_stroke(ws, was, done);
                    }
                    self.page = hit;
                    if hit.is_some() {
                        self.pts.push(p);
                    }
                }
            }
        }
        self.extend_curve();
    }

    /// Make the curve through the segments that have become final.
    fn extend_curve(&mut self) {
        if self.curve.is_empty() {
            self.curve.extend(self.pts.first());
        }
        let upto = self.pts.len().saturating_sub(2);
        if upto > self.stable {
            curve_segments(&self.pts, self.stable..upto, CURVE_STEP, &mut self.curve);
            self.stable = upto;
        }
    }

    /// The stroke so far: what is final, then the end of it (with where the pen is going) made afresh.
    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        if self.pts.is_empty() {
            return;
        }
        let (rgba, width) = ws.current_style();
        let rgba = ws.on_page(rgba);
        if self.stable > 0 {
            polyline(out, &self.curve, width, rgba);
        }
        let from = self.stable.saturating_sub(1);
        let mut window: Vec<P> = self.pts.get(from..).unwrap_or_default().to_vec();
        if let Some(ahead) = predict(&self.recent, PREDICT_MS) {
            window.push(ws.view.screen_to_doc(ahead));
        }
        let skip = self.stable - from;
        let mut tail: Vec<P> = window.get(skip).copied().into_iter().collect();
        if window.len() >= 3 {
            curve_segments(&window, skip..window.len() - 1, CURVE_STEP, &mut tail);
        } else {
            tail.extend(window.get(skip + 1..).unwrap_or_default());
        }
        polyline(out, &tail, width, rgba);
    }

    fn finish(mut self, ws: &mut Workspace) {
        // The steadied path trails the pen a little, and the last sample may have been closer than the
        // spacing: end the stroke exactly where the pen did.
        if let Some(end) = self.raw_end
            && self.pts.last().is_some_and(|&l| l != end)
            && self.page == ws.page_hit(end)
        {
            self.pts.push(end);
        }
        if let Some(page) = self.page {
            commit_stroke(ws, page, self.pts);
        }
    }
}

/// Store a finished stroke: simplified so files stay small and strokes cheap to draw.
fn commit_stroke(ws: &mut Workspace, page: u32, pts: Vec<P>) {
    let zoom = ws.view.zoom;
    // Under a quarter of a device pixel at this zoom, and never coarser than 0.15 point: fine enough that
    // the curve drawn through the points keeps the pen's shape.
    let tolerance = (0.25 / zoom).min(0.15);
    let pts = ink::simplify(&ink::thin(&pts, 0.12 / zoom), tolerance);
    let (rgba, width) = ws.current_style();
    ws.ink.add(page, Style { rgba, width }, pts);
}

#[derive(Default)]
enum Gesture {
    #[default]
    None,
    Draw(Draw),
    /// Erasing, selecting, or dragging the selection (see [`Pick`]).
    Pick(Pick),
    /// A press that only let go of the selection; it does nothing else.
    Swallow(u32),
}

#[derive(Default)]
pub struct PenTool {
    gesture: Gesture,
}

impl PenTool {
    fn id(&self) -> Option<u32> {
        match &self.gesture {
            Gesture::None => None,
            Gesture::Draw(d) => Some(d.id),
            Gesture::Pick(p) => Some(p.id()),
            Gesture::Swallow(id) => Some(*id),
        }
    }

    /// The stroke being drawn, as the points it has and the recent samples that predict where it goes.
    #[cfg(test)]
    pub fn live(&self) -> Option<(&[P], &[Sample])> {
        match &self.gesture {
            Gesture::Draw(d) => Some((&d.pts, &d.recent)),
            _ => None,
        }
    }

    /// Whether a stroke is being drawn.
    #[cfg(test)]
    pub fn is_drawing(&self) -> bool {
        matches!(self.gesture, Gesture::Draw(_))
    }

    fn down(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        let Some(first) = ev.samples.first() else { return };
        let p = ws.to_doc(first.pos);
        let on_selection = ws.selection_hit(p);
        self.gesture = if let Some(pick) = Pick::begin(ws, ev, p, on_selection) {
            Gesture::Pick(pick)
        } else if ws.selection.is_empty() {
            Gesture::Draw(Draw::new(ev.id, ws.smoothing))
        } else if on_selection {
            // With something selected, the pen drags it; touching elsewhere lets go.
            Gesture::Pick(Pick::grab(Device::Pen, ev.id, p))
        } else {
            ws.selection.clear();
            Gesture::Swallow(ev.id)
        };
        // (The other gestures took in the first samples as they began.)
        if matches!(self.gesture, Gesture::Draw(_)) {
            self.moved(ws, ev.samples);
        }
    }
    fn moved(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        match &mut self.gesture {
            Gesture::Draw(d) => d.drag(ws, samples),
            Gesture::Pick(p) => p.moved(ws, samples),
            Gesture::None | Gesture::Swallow(_) => {}
        }
        ws.redraw = true;
    }
}

impl Tool for PenTool {
    fn policy(&self, mode: Mode) -> InputPolicy {
        InputPolicy::editing_tool(mode)
    }

    fn key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        selection_key(ws, key)
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
            _ => {}
        }
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        dev == Device::Pen && self.id() == Some(id)
    }

    fn busy(&self) -> bool {
        self.id().is_some()
    }

    fn end(&mut self, ws: &mut Workspace) {
        match std::mem::take(&mut self.gesture) {
            Gesture::None | Gesture::Swallow(_) => return,
            Gesture::Draw(d) => d.finish(ws),
            Gesture::Pick(p) => p.end(ws, |_, _| false),
        }
        ws.redraw = true;
    }

    fn cancel(&mut self, ws: &mut Workspace) {
        // What the eraser had touched is not erased.
        ws.erasing.clear();
        self.gesture = Gesture::None;
    }

    fn drag_offset(&self) -> [f32; 2] {
        match &self.gesture {
            Gesture::Pick(p) => p.offset(),
            _ => [0.0; 2],
        }
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        match &self.gesture {
            Gesture::Draw(d) => d.paint(ws, out),
            Gesture::Pick(p) => p.paint(ws, out),
            Gesture::Swallow(_) | Gesture::None => {}
        }
    }
}
