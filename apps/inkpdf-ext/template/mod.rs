//! TEMPLATE_LABEL: a template of an extension (copied by `tools/new-extension.sh`; this is what it starts from).
//!
//! It is a tool that rings the place the stylus (or the mouse, at a desk) is over, and leaves a ring where it is pressed, which stays until
//! the tool is put away. Change `Marker` into what your tool does; keep to `inkpdf-api` (a test fails if you do not).

use inkpdf_api::{API_VERSION, Context, Device, Extension, P, Painter, Phase, PointerEvent, Policy, Tool};

/// The extension, for the list of the program's (`build.rs` finds this function by the folder it is in).
pub fn extension() -> Extension {
    Extension {
        api: API_VERSION,
        id: "TEMPLATE_ID",
        label: "TEMPLATE_LABEL",
        icon_svg: include_str!("icon.svg"),
        create: || Box::new(Marker::default()),
    }
}

#[derive(Default)]
struct Marker {
    /// Where the pointer is over the page, in document space.
    over: Option<P>,
    /// The rings left by presses, in document space.
    rings: Vec<P>,
    /// The pointer that is pressed, while one is.
    down: Option<(Device, u32)>,
}

impl Tool for Marker {
    fn policy(&self, desktop: bool) -> Policy {
        Policy::stylus_tool(desktop)
    }

    fn pointer(&mut self, cx: &mut dyn Context, ev: &PointerEvent<'_>) {
        let at = ev.samples.last().map(|s| cx.to_doc(s.pos));
        match ev.phase {
            Phase::Down => {
                self.down = Some((ev.device, ev.id));
                self.rings.extend(at);
            }
            Phase::Up | Phase::Cancel => self.down = None,
            Phase::Leave => self.over = None,
            Phase::Hover | Phase::Move => {}
        }
        if ev.phase != Phase::Leave {
            self.over = at.filter(|_| cx.has_document());
        }
        cx.request_redraw();
    }

    fn owns(&self, device: Device, id: u32) -> bool {
        self.down == Some((device, id))
    }

    fn busy(&self) -> bool {
        self.down.is_some()
    }

    fn end(&mut self, _cx: &mut dyn Context) {
        self.down = None;
    }

    fn cancel(&mut self, _cx: &mut dyn Context) {
        self.down = None;
    }

    /// Put away: nothing is left to surface later.
    fn leave(&mut self, _cx: &mut dyn Context) {
        *self = Marker::default();
    }

    fn pages_edited(&mut self, _cx: &mut dyn Context) {
        self.rings.clear();
    }

    fn paint(&self, cx: &dyn Context, out: &mut dyn Painter) {
        let px = cx.dpr() / cx.zoom();
        let colour = cx.theme().accent;
        for ring in &self.rings {
            out.doc_line(*ring, *ring, 14.0 * px, colour);
            out.doc_line(*ring, *ring, 10.0 * px, [255, 255, 255, 255]);
        }
        if let Some(at) = self.over.map(|p| cx.doc_to_screen(p)) {
            out.screen_line(at, at, 18.0 * cx.dpr(), [colour[0], colour[1], colour[2], 90]);
        }
    }
}
