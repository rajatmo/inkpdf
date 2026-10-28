//! The pointer tool: the stylus points at the page, a dot follows it, and a trail that fades is left behind
//! while it is pressed. Nothing is drawn into the document. Nothing else points: a finger pans and zooms,
//! the mouse scrolls, the keyboard does nothing.

use crate::input::{Device, Phase, Sample};
use crate::laser::Laser;
use crate::workspace::Workspace;

use super::paint::Painter;
use super::{InputPolicy, PageEvent, Tool};

const RED: [u8; 3] = [236, 36, 48];

#[derive(Default)]
pub struct PointerTool {
    pub laser: Laser,
    /// The pointer that is pressed, while one is.
    down: Option<(Device, u32)>,
}

impl PointerTool {
    fn drag(&mut self, ws: &mut Workspace, samples: &[Sample]) {
        for s in samples {
            self.laser.drag(ws.to_doc(s.pos), s.t_ms);
        }
        ws.redraw = true;
    }

    /// The pen is hovering at `pos` (screen), or has gone (`None`): the dot follows it.
    pub fn hover(&mut self, ws: &mut Workspace, pos: Option<[f32; 2]>) {
        let at = pos.filter(|_| ws.has_doc).map(|p| ws.to_doc(p));
        if at != self.laser.head() {
            self.laser.show(at);
            ws.redraw = true;
        }
    }

    /// One frame of the trail fading: whether it still is.
    pub fn prune(&mut self, now_ms: f64) -> bool {
        self.laser.prune(now_ms)
    }
}

impl Tool for PointerTool {
    fn policy(&self) -> InputPolicy {
        InputPolicy { stylus: true, mouse: false, keyboard: false }
    }

    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        match ev.phase {
            Phase::Down => {
                // Whichever end or button of the pen touches, it points.
                self.down = Some((ev.dev, ev.id));
                if let Some(first) = ev.samples.first() {
                    self.laser.press(ws.to_doc(first.pos), first.t_ms);
                }
                self.drag(ws, ev.samples.get(1..).unwrap_or_default());
            }
            Phase::Move if self.owns(ev.dev, ev.id) => self.drag(ws, ev.samples),
            Phase::Up | Phase::Cancel if self.owns(ev.dev, ev.id) => {
                self.drag(ws, ev.samples);
                self.end(ws);
            }
            Phase::Hover => self.hover(ws, ev.samples.last().map(|s| s.pos)),
            Phase::Leave => self.hover(ws, None),
            _ => {}
        }
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        self.down == Some((dev, id))
    }

    fn busy(&self) -> bool {
        self.down.is_some()
    }

    fn end(&mut self, ws: &mut Workspace) {
        if self.down.take().is_some() {
            self.laser.release();
            ws.redraw = true;
        }
    }

    fn leave(&mut self, _ws: &mut Workspace) {
        self.down = None;
        self.laser.clear();
    }

    fn cancel(&mut self, _ws: &mut Workspace) {
        self.down = None;
        self.laser.clear();
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        let px = ws.dpr / ws.view.zoom;
        let now = out.now_ms();
        for (a, b, life) in self.laser.pieces(now) {
            // Opaque, so the overlap between pieces leaves no beads; it fades by thinning.
            out.doc_line(a, b, px * (0.8 + 8.2 * life.powf(1.4)), [RED[0], RED[1], RED[2], ((life * 4.0).min(1.0) * 255.0) as u8]);
        }
        if let Some(head) = self.laser.head() {
            let at = ws.view.doc_to_screen(head);
            let dpr = ws.dpr;
            out.screen_line(at, at, 30.0 * dpr, [RED[0], RED[1], RED[2], 56]);
            out.screen_line(at, at, 18.0 * dpr, [RED[0], RED[1], RED[2], 110]);
            out.screen_line(at, at, 10.0 * dpr, [RED[0], RED[1], RED[2], 255]);
            out.screen_line(at, at, 4.0 * dpr, [255, 255, 255, 235]);
        }
    }
}
