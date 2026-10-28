//! Gestures several tools share, kept as small values the tools hold (composition, not inheritance): the
//! drag of a selection, the eraser, the lasso.

use crate::ink::{self, P, dist2};
use crate::input::{Device, Sample};
use crate::workspace::Workspace;

use super::paint::{ACCENT, Painter, polyline};

/// The eraser's radius, in logical pixels.
pub const ERASER_RADIUS_PX: f32 = 9.0;

/// Dragging the selection. It follows the pointer without touching the stored ink; when the pointer lets
/// go it is one undo step, if it went far enough to be a drag and not a tap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mover {
    pub dev: Device,
    pub id: u32,
    start: P,
    pub delta: [f32; 2],
    /// It has gone far enough (3 pixels) to be a drag.
    pub moved: bool,
}

impl Mover {
    pub fn begin(dev: Device, id: u32, start: P) -> Self {
        Self { dev, id, start, delta: [0.0; 2], moved: false }
    }

    /// The pointer is now at screen position `pos`.
    pub fn drag(&mut self, ws: &Workspace, pos: [f32; 2]) {
        let p = ws.to_doc(pos);
        self.delta = [p[0] - self.start[0], p[1] - self.start[1]];
        if (self.delta[0].powi(2) + self.delta[1].powi(2)).sqrt() * ws.view.zoom > 3.0 {
            self.moved = true;
        }
    }

    /// How far the selection is being dragged: nothing until it is a drag, not a tap.
    pub fn offset(&self) -> [f32; 2] {
        if self.moved { self.delta } else { [0.0; 2] }
    }

    /// The pointer let go: if it was a drag, the selection moves (to another page if its middle was dropped
    /// there). Returns whether it was a drag.
    pub fn finish(self, ws: &mut Workspace) -> bool {
        if !self.moved {
            return false;
        }
        let ids = ws.selection.clone();
        let (dx, dy) = (self.delta[0], self.delta[1]);
        // Dropped with its middle on another page, the ink belongs to that page.
        let to_page = ws.ink.bounds_of(&ids).and_then(|b| ws.page_hit([(b[0] + b[2]) / 2.0 + dx, (b[1] + b[3]) / 2.0 + dy]));
        ws.ink.move_by(&ids, dx, dy, to_page);
        true
    }
}

/// The eraser end of the pen: it marks every stroke it passes over, and they are all removed, as one undo
/// step, when it lifts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eraser {
    pub id: u32,
    last: Option<P>,
    /// Where it is, in screen pixels, for the ring drawn around it.
    pos: [f32; 2],
}

impl Eraser {
    pub fn begin(id: u32) -> Self {
        Self { id, last: None, pos: [0.0; 2] }
    }

    pub fn drag(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        let radius = ERASER_RADIUS_PX * ws.dpr / ws.view.zoom;
        for s in samples {
            let p = ws.to_doc(s.pos);
            self.pos = s.pos;
            let from = self.last.unwrap_or(p);
            self.last = Some(p);
            for id in ws.ink.touched_by_segment(from, p, radius) {
                ws.erasing.insert(id);
                ws.selection.retain(|&sel| sel != id);
            }
        }
    }

    /// Remove what was touched.
    pub fn finish(ws: &mut Workspace) {
        let ids: Vec<u64> = ws.erasing.drain().collect();
        ws.ink.remove(&ids);
        // Something may have been selected while the eraser was still working (`--select-all`).
        ws.prune_selection();
    }

    /// A ring the size of the eraser.
    pub fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        const STEPS: usize = 24;
        let r = ERASER_RADIUS_PX * ws.dpr;
        let at = |i: usize| {
            let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
            [self.pos[0] + r * a.cos(), self.pos[1] + r * a.sin()]
        };
        for i in 0..STEPS {
            out.screen_line(at(i), at(i + 1), 1.5 * ws.dpr, [90, 90, 96, 230]);
        }
    }
}

/// The barrel button's loop: what it encloses is selected when the pen lifts.
#[derive(Clone, Debug, PartialEq)]
pub struct Lasso {
    pub id: u32,
    pub poly: Vec<P>,
}

impl Lasso {
    pub fn begin(id: u32, at: P) -> Self {
        Self { id, poly: vec![at] }
    }

    pub fn drag(&mut self, ws: &Workspace, samples: &[Sample]) {
        let gap2 = (1.5 * ws.dpr / ws.view.zoom).powi(2);
        for s in samples {
            let p = ws.to_doc(s.pos);
            if self.poly.last().is_none_or(|&l| dist2(l, p) >= gap2) {
                self.poly.push(p);
            }
        }
    }

    /// Select what `poly` encloses.
    pub fn select(ws: &mut Workspace, poly: &[P]) {
        let poly = ink::simplify(poly, 0.5 / ws.view.zoom);
        ws.selection = if poly.len() >= 3 { ws.ink.in_polygon(&poly) } else { Vec::new() };
    }

    pub fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        let px = ws.dpr / ws.view.zoom;
        polyline(out, &self.poly, 1.5 * px, ACCENT);
        if let (Some(first), Some(last)) = (self.poly.first(), self.poly.last()) {
            out.doc_line(*last, *first, 1.0 * px, [ACCENT[0], ACCENT[1], ACCENT[2], 110]);
        }
    }
}
