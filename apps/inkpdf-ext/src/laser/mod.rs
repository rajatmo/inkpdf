//! The laser pointer, as an extension: the stylus points at the page, a dot follows it, and a trail that fades is left behind while it is
//! pressed. Nothing is drawn into the document. Nothing else points: a finger pans and zooms, the mouse (in the tablet mode) scrolls, the
//! keyboard does nothing.
//!
//! It is written against `inkpdf-api` alone: it is the proof that a tool can be.

use inkpdf_api::{Context, Device, Extension, P, Painter, Phase, PointerEvent, Policy, Sample, Tool, API_VERSION};

mod model;

use model::Laser;

/// The laser's red: the same in the light and the dark look.
const RED: [u8; 3] = [236, 36, 48];

/// The extension, for the list of the program's.
pub fn extension() -> Extension {
    Extension { api: API_VERSION, id: "laser", label: "Laser pointer", icon_svg: include_str!("icon.svg"), create: || Box::new(Pointer::default()) }
}

/// The tool.
#[derive(Default)]
struct Pointer {
    laser: Laser,
    /// The pointer that is pressed, while one is.
    down: Option<(Device, u32)>,
}

impl Pointer {
    fn drag(&mut self, cx: &mut dyn Context, samples: &[Sample]) {
        for s in samples {
            self.laser.drag(cx.to_doc(s.pos), s.t_ms);
        }
        cx.request_redraw();
    }

    /// The pen is hovering at `pos` (screen), or has gone (`None`): the dot follows it.
    fn hover(&mut self, cx: &mut dyn Context, pos: Option<P>) {
        let at = pos.filter(|_| cx.has_document()).map(|p| cx.to_doc(p));
        if at != self.laser.head() {
            self.laser.show(at);
            cx.request_redraw();
        }
    }
}

impl Tool for Pointer {
    fn edits(&self) -> bool {
        false
    }

    fn policy(&self, desktop: bool) -> Policy {
        Policy::stylus_tool(desktop)
    }

    fn pointer(&mut self, cx: &mut dyn Context, ev: &PointerEvent<'_>) {
        match ev.phase {
            Phase::Down => {
                // Whichever end or button of the pen touches, it points.
                self.down = Some((ev.device, ev.id));
                if let Some(first) = ev.samples.first() {
                    self.laser.press(cx.to_doc(first.pos), first.t_ms);
                }
                self.drag(cx, ev.samples.get(1..).unwrap_or_default());
            }
            Phase::Move if self.owns(ev.device, ev.id) => self.drag(cx, ev.samples),
            Phase::Up | Phase::Cancel if self.owns(ev.device, ev.id) => {
                self.drag(cx, ev.samples);
                self.end(cx);
            }
            Phase::Hover => self.hover(cx, ev.samples.last().map(|s| s.pos)),
            Phase::Leave => self.hover(cx, None),
            _ => {}
        }
    }

    fn owns(&self, device: Device, id: u32) -> bool {
        self.down == Some((device, id))
    }

    fn busy(&self) -> bool {
        self.down.is_some()
    }

    fn end(&mut self, cx: &mut dyn Context) {
        if self.down.take().is_some() {
            self.laser.release();
            cx.request_redraw();
        }
    }

    fn leave(&mut self, _cx: &mut dyn Context) {
        self.down = None;
        self.laser.clear();
    }

    fn cancel(&mut self, _cx: &mut dyn Context) {
        self.down = None;
        self.laser.clear();
    }

    /// The trail fades with the clock: the frames go on until it is gone.
    fn tick(&mut self, now_ms: f64) -> bool {
        self.laser.prune(now_ms)
    }

    /// A trail is on a page: when the pages move it is let go.
    fn pages_edited(&mut self, _cx: &mut dyn Context) {
        self.laser.clear();
    }

    fn paint(&self, cx: &dyn Context, out: &mut dyn Painter) {
        let px = cx.dpr() / cx.zoom();
        let now = out.now_ms();
        let colour = |alpha: u8| [RED[0], RED[1], RED[2], alpha];
        for (a, b, life) in self.laser.pieces(now) {
            // Opaque, so the overlap between pieces leaves no beads; it fades by thinning.
            out.doc_line(a, b, px * (0.8 + 8.2 * life.powf(1.4)), colour(((life * 4.0).min(1.0) * 255.0) as u8));
        }
        if let Some(head) = self.laser.head() {
            let at = cx.doc_to_screen(head);
            let dpr = cx.dpr();
            out.screen_line(at, at, 30.0 * dpr, colour(56));
            out.screen_line(at, at, 18.0 * dpr, colour(110));
            out.screen_line(at, at, 10.0 * dpr, colour(255));
            out.screen_line(at, at, 4.0 * dpr, [255, 255, 255, 235]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkpdf_api::{Theme, Rgba};

    /// A window with a document at zoom 2, its origin at the screen's (10, 20): the API's `Context`, made by hand.
    struct Window {
        has_document: bool,
        redraws: usize,
    }

    impl Context for Window {
        fn to_doc(&self, s: P) -> P {
            [(s[0] - 10.0) / 2.0, (s[1] - 20.0) / 2.0]
        }
        fn doc_to_screen(&self, d: P) -> P {
            [d[0] * 2.0 + 10.0, d[1] * 2.0 + 20.0]
        }
        fn zoom(&self) -> f32 {
            2.0
        }
        fn dpr(&self) -> f32 {
            1.0
        }
        fn has_document(&self) -> bool {
            self.has_document
        }
        fn request_redraw(&mut self) {
            self.redraws += 1;
        }
        fn theme(&self) -> Theme {
            Theme { ink: [0; 4], accent: [0; 4], selection: [0; 4], danger: [0; 4], white: [255; 4] }
        }
    }

    #[derive(Default)]
    struct Lines {
        doc: Vec<(P, P, f32, Rgba)>,
        screen: Vec<(P, P, f32, Rgba)>,
        now: f64,
    }

    impl Painter for Lines {
        fn doc_line(&mut self, a: P, b: P, width: f32, rgba: Rgba) {
            self.doc.push((a, b, width, rgba));
        }
        fn screen_line(&mut self, a: P, b: P, width: f32, rgba: Rgba) {
            self.screen.push((a, b, width, rgba));
        }
        fn now_ms(&self) -> f64 {
            self.now
        }
    }

    fn event(device: Device, phase: Phase, at: &[Sample]) -> PointerEvent<'_> {
        PointerEvent { device, id: 1, phase, samples: at, pen: Default::default() }
    }

    fn at(x: f32, y: f32, t: f64) -> Sample {
        Sample { pos: [x, y], pressure: 0.5, t_ms: t }
    }

    fn painted(tool: &Pointer, w: &Window, now: f64) -> Lines {
        let mut out = Lines { now, ..Lines::default() };
        tool.paint(w, &mut out);
        out
    }

    #[test]
    fn it_is_a_well_formed_extension_a_stylus_tool_with_no_keyboard() {
        let e = extension();
        assert_eq!(e.problem(), None);
        assert_eq!(e.id, "laser");
        let tool = (e.create)();
        for desktop in [true, false] {
            let p = tool.policy(desktop);
            assert!(p.stylus && !p.keyboard);
            assert_eq!(p.mouse, if desktop { inkpdf_api::MouseUse::AsStylus } else { inkpdf_api::MouseUse::Scrolls });
        }
    }

    #[test]
    fn a_hovering_pen_has_a_dot_where_it_is_and_it_goes_when_the_pen_does_or_when_there_is_no_document() {
        let (mut tool, mut w) = (Pointer::default(), Window { has_document: true, redraws: 0 });
        tool.pointer(&mut w, &event(Device::Pen, Phase::Hover, &[at(110.0, 120.0, 0.0)]));
        let out = painted(&tool, &w, 0.0);
        assert_eq!(out.screen.len(), 4, "a halo, a ring, a dot and its highlight");
        assert_eq!(out.screen[0].0, [110.0, 120.0], "on the screen where the pen is");
        assert!(out.doc.is_empty(), "hovering leaves no trail");
        assert!(w.redraws > 0, "a frame was asked for");
        tool.pointer(&mut w, &event(Device::Pen, Phase::Leave, &[]));
        assert!(painted(&tool, &w, 0.0).screen.is_empty());
        w.has_document = false;
        tool.pointer(&mut w, &event(Device::Pen, Phase::Hover, &[at(110.0, 120.0, 0.0)]));
        assert!(painted(&tool, &w, 0.0).screen.is_empty(), "with no page there is nothing to point at");
    }

    #[test]
    fn a_press_leaves_a_trail_in_the_document_that_thins_fades_and_is_gone_and_the_tool_says_when() {
        let (mut tool, mut w) = (Pointer::default(), Window { has_document: true, redraws: 0 });
        let path: Vec<Sample> = (0..10).map(|i| at(10.0 + i as f32 * 20.0, 20.0 + i as f32 * 5.0, 1000.0 + i as f64)).collect();
        tool.pointer(&mut w, &event(Device::Pen, Phase::Down, &path[..1]));
        assert!(tool.owns(Device::Pen, 1) && tool.busy());
        tool.pointer(&mut w, &event(Device::Pen, Phase::Move, &path[1..]));
        tool.pointer(&mut w, &event(Device::Pen, Phase::Up, &path[9..]));
        assert!(!tool.busy(), "the stroke is over");
        let fresh = painted(&tool, &w, 1010.0);
        assert!(fresh.doc.len() >= 9, "a piece between each pair of points");
        assert_eq!(fresh.doc[0].0, [0.0, 0.0], "in the document's own space");
        let old = painted(&tool, &w, 1010.0 + 800.0);
        assert!(old.doc[0].2 < fresh.doc[0].2, "it thins as it ages");
        assert!(tool.tick(1010.0), "still fading: frames go on");
        assert!(!tool.tick(1010.0 + 5000.0), "gone: an idle window asks for none");
        assert!(painted(&tool, &w, 1010.0 + 5000.0).doc.is_empty());
    }

    #[test]
    fn a_finger_cancelled_stroke_the_tool_being_put_away_and_the_pages_being_edited_leave_nothing() {
        let (mut tool, mut w) = (Pointer::default(), Window { has_document: true, redraws: 0 });
        let path = [at(10.0, 20.0, 1.0), at(50.0, 60.0, 2.0), at(90.0, 100.0, 3.0)];
        for ending in 0..3 {
            tool.pointer(&mut w, &event(Device::Pen, Phase::Down, &path[..1]));
            tool.pointer(&mut w, &event(Device::Pen, Phase::Move, &path[1..]));
            assert!(!painted(&tool, &w, 4.0).doc.is_empty());
            match ending {
                0 => tool.cancel(&mut w),
                1 => tool.leave(&mut w),
                _ => tool.pages_edited(&mut w),
            }
            assert!(painted(&tool, &w, 4.0).doc.is_empty(), "ending {ending}");
            tool.leave(&mut w);
            assert!(!tool.busy());
        }
    }

    #[test]
    fn hostile_events_make_no_panic_and_nothing_that_is_not_a_number() {
        let (mut tool, mut w) = (Pointer::default(), Window { has_document: true, redraws: 0 });
        let bad = [at(f32::NAN, 0.0, 0.0), at(f32::INFINITY, f32::NEG_INFINITY, f64::NAN), at(1e30, -1e30, -1.0), at(0.0, 0.0, f64::INFINITY)];
        for phase in [Phase::Hover, Phase::Down, Phase::Move, Phase::Up, Phase::Cancel, Phase::Leave, Phase::Move, Phase::Up] {
            tool.pointer(&mut w, &event(Device::Mouse, phase, &bad));
            tool.pointer(&mut w, &event(Device::Pen, phase, &[]));
            tool.tick(f64::NAN);
        }
        let out = painted(&tool, &w, 1.0e6);
        assert!(out.doc.iter().chain(&out.screen).all(|l| l.0.iter().chain(&l.1).chain(&[l.2]).all(|v| v.is_finite())));
    }
}
