//! Gestures several tools share, kept as small values the tools hold (composition, not inheritance): the
//! drag of a selection, the eraser, the lasso.

use crate::ink::{self, P, dist2};
use crate::input::{Device, Intent, Role, Sample};
use crate::workspace::Workspace;

use super::PageEvent;
use super::paint::{Painter, polyline};

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
            for id in ws.keep_own(ws.ink.touched_by_segment(from, p, radius)) {
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
            out.screen_line(at(i), at(i + 1), 1.5 * ws.dpr, ws.theme.guide);
        }
    }
}

/// A loop drawn with the pen: what it encloses is selected when the pen lifts. A tap, which makes no loop, selects
/// the item under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Lasso {
    pub id: u32,
    pub poly: Vec<P>,
    /// It was begun with the stylus's barrel button (not by choosing to select on screen).
    pub barrel: bool,
}

impl Lasso {
    pub fn begin(id: u32, at: P, barrel: bool) -> Self {
        Self { id, poly: vec![at], barrel }
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

    /// Whether a loop that stayed this near where it began is a tap.
    pub fn is_tap(ws: &Workspace, poly: &[P]) -> bool {
        let reach = super::TAP_SELECT_PX * ws.dpr / ws.view.zoom;
        poly.first().is_some_and(|&first| poly.iter().all(|&q| dist2(q, first) <= reach * reach))
    }

    /// Select what `poly` encloses (of the tool's own items), or, if it was a tap, the one item under it.
    pub fn select(ws: &mut Workspace, poly: &[P]) {
        if Self::is_tap(ws, poly) {
            let radius = super::TAP_SELECT_PX * ws.dpr / ws.view.zoom;
            ws.selection = poly.first().and_then(|&at| ws.hit_own(at, radius)).into_iter().collect();
            return;
        }
        let poly = ink::simplify(poly, 0.5 / ws.view.zoom);
        ws.selection = if poly.len() >= 3 { ws.keep_own(ws.ink.in_polygon(&poly)) } else { Vec::new() };
    }

    pub fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        let px = ws.dpr / ws.view.zoom;
        polyline(out, &self.poly, 1.5 * px, ws.theme.selection);
        if let (Some(first), Some(last)) = (self.poly.first(), self.poly.last()) {
            out.doc_line(*last, *first, 1.0 * px, [ws.theme.selection[0], ws.theme.selection[1], ws.theme.selection[2], 110]);
        }
    }
}

/// What a press does when it is not making something, for every tool that makes items: erase what the tip passes
/// over (the eraser end, or erase chosen on screen), loop or tap to select (the barrel button, or select chosen), and,
/// with select chosen, drag what is selected where the press lands on it. A tool holds one in its own gesture and
/// hands it the pointer, so that pen, shape and text do not each carry the same three gestures.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Erase { dev: Device, eraser: Eraser },
    Lasso { dev: Device, lasso: Lasso },
    Move(Mover),
}

impl Pick {
    /// What a press at `p` starts, if it is not one that makes something (`None` for that). `on_selection` is whether
    /// `p` is on what is selected, as the tool judges it (a shape by its outline, ink and text by their box).
    pub fn begin(ws: &mut Workspace, ev: &PageEvent<'_>, p: P, on_selection: bool) -> Option<Pick> {
        // Select was chosen on screen, rather than asked for with the barrel button.
        let chosen = ev.pen.role() == Role::Draw && ws.intent == Intent::Select;
        match ws.role(&ev.pen) {
            Role::Draw => None,
            Role::Erase => {
                let mut eraser = Eraser::begin(ev.id);
                eraser.drag(ws, ev.samples);
                Some(Pick::Erase { dev: ev.dev, eraser })
            }
            Role::Lasso if chosen && on_selection => Some(Pick::grab(ev.dev, ev.id, p)),
            Role::Lasso => {
                let mut lasso = Lasso::begin(ev.id, p, !chosen);
                lasso.drag(ws, ev.samples);
                Some(Pick::Lasso { dev: ev.dev, lasso })
            }
        }
    }

    /// Take hold of the selection at `p`: a drag moves it.
    pub fn grab(dev: Device, id: u32, p: P) -> Pick {
        Pick::Move(Mover::begin(dev, id, p))
    }

    pub fn owns(&self, dev: Device, id: u32) -> bool {
        match self {
            Pick::Erase { dev: d, eraser } => *d == dev && eraser.id == id,
            Pick::Lasso { dev: d, lasso } => *d == dev && lasso.id == id,
            Pick::Move(m) => m.dev == dev && m.id == id,
        }
    }

    pub fn id(&self) -> u32 {
        match self {
            Pick::Erase { eraser, .. } => eraser.id,
            Pick::Lasso { lasso, .. } => lasso.id,
            Pick::Move(m) => m.id,
        }
    }

    /// The pointer moved through `samples`.
    pub fn moved(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        match self {
            Pick::Erase { eraser, .. } => eraser.drag(ws, samples),
            Pick::Lasso { lasso, .. } => lasso.drag(ws, samples),
            Pick::Move(m) => {
                if let Some(s) = samples.last() {
                    m.drag(ws, s.pos);
                }
            }
        }
    }

    /// The pointer lifted: what was erased goes, what was looped is selected, what was dragged moves. A tool that does
    /// something else with a loop (a tap that adds a point) says so in `taken`, which is given the loop first and
    /// returns whether it dealt with it.
    pub fn end(self, ws: &mut Workspace, taken: impl FnOnce(&mut Workspace, &Lasso) -> bool) {
        match self {
            Pick::Erase { .. } => Eraser::finish(ws),
            Pick::Lasso { lasso, .. } => {
                if !taken(ws, &lasso) {
                    Lasso::select(ws, &lasso.poly);
                }
            }
            Pick::Move(m) => {
                m.finish(ws);
            }
        }
    }

    /// How far the selection is being dragged.
    pub fn offset(&self) -> [f32; 2] {
        match self {
            Pick::Move(m) => m.offset(),
            _ => [0.0; 2],
        }
    }

    pub fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        match self {
            Pick::Erase { eraser, .. } => eraser.paint(ws, out),
            Pick::Lasso { lasso, .. } => lasso.paint(ws, out),
            Pick::Move(_) => {}
        }
    }
}
