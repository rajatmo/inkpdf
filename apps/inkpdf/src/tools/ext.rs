//! Extensions' tools in the program's [`ToolSet`]: each is held as the object the extension made (an `inkpdf_api::Tool`) and spoken to through
//! the API's own types. This is the one place that turns the program's events into the API's and the program's workspace into the API's
//! [`Context`](inkpdf_api::Context); an extension never sees either.

use inkpdf_api as api;

use crate::input::{Device, Dir, Mode, Phase};
use crate::tool::Tool as ToolId;
use crate::workspace::Workspace;

use super::paint::Painter;
use super::{InputPolicy, Key, MouseUse, PageEvent, Tool};

/// What an extension is handed to ask of the document and the window, for one call (it can change the workspace).
struct Cx<'a>(&'a mut Workspace);

/// The same, for a call that only looks (painting): asking for a frame does nothing then.
struct CxRef<'a>(&'a Workspace);

fn theme_of(ws: &Workspace) -> api::Theme {
    let t = &ws.theme;
    api::Theme { ink: t.ink, accent: t.accent, selection: t.selection, danger: t.danger, white: t.white }
}

macro_rules! context_reading {
    ($t:ty) => {
        fn to_doc(&self, screen: api::P) -> api::P {
            self.0.to_doc(screen)
        }
        fn doc_to_screen(&self, doc: api::P) -> api::P {
            self.0.view.doc_to_screen(doc)
        }
        fn zoom(&self) -> f32 {
            self.0.view.zoom
        }
        fn dpr(&self) -> f32 {
            self.0.dpr
        }
        fn has_document(&self) -> bool {
            self.0.has_doc
        }
        fn theme(&self) -> api::Theme {
            theme_of(self.0)
        }
    };
}

impl api::Context for Cx<'_> {
    context_reading!(Cx);
    fn request_redraw(&mut self) {
        self.0.redraw = true;
    }
}

impl api::Context for CxRef<'_> {
    context_reading!(CxRef);
    fn request_redraw(&mut self) {}
}

/// The program's painter, as the API's.
struct Out<'a>(&'a mut dyn Painter);

impl api::Painter for Out<'_> {
    fn doc_line(&mut self, a: api::P, b: api::P, width: f32, rgba: api::Rgba) {
        self.0.doc_line(a, b, width, rgba);
    }
    fn screen_line(&mut self, a: api::P, b: api::P, width: f32, rgba: api::Rgba) {
        self.0.screen_line(a, b, width, rgba);
    }
    fn now_ms(&self) -> f64 {
        self.0.now_ms()
    }
}

fn device_in(d: Device) -> api::Device {
    match d {
        Device::Mouse => api::Device::Mouse,
        Device::Touch => api::Device::Touch,
        Device::Pen => api::Device::Pen,
    }
}

fn device_out(d: api::Device) -> Device {
    match d {
        api::Device::Mouse => Device::Mouse,
        api::Device::Touch => Device::Touch,
        api::Device::Pen => Device::Pen,
    }
}

fn phase_in(p: Phase) -> api::Phase {
    match p {
        Phase::Down => api::Phase::Down,
        Phase::Move => api::Phase::Move,
        Phase::Up => api::Phase::Up,
        Phase::Cancel => api::Phase::Cancel,
        Phase::Hover => api::Phase::Hover,
        Phase::Leave => api::Phase::Leave,
    }
}

fn key_in(k: Key<'_>) -> api::Key<'_> {
    let dir = |d: Dir| match d {
        Dir::Left => api::Dir::Left,
        Dir::Right => api::Dir::Right,
        Dir::Up => api::Dir::Up,
        Dir::Down => api::Dir::Down,
    };
    match k {
        Key::Text(t) => api::Key::Text(t),
        Key::Backspace => api::Key::Backspace,
        Key::Delete => api::Key::Delete,
        Key::Escape => api::Key::Escape,
        Key::Arrow(d, shift) => api::Key::Arrow(dir(d), shift),
        Key::Home(shift) => api::Key::Home(shift),
        Key::End(shift) => api::Key::End(shift),
        Key::Chord(c) => api::Key::Chord(c),
    }
}

/// One extension's tool, held by the [`ToolSet`](super::ToolSet).
pub struct ExtTool {
    id: ToolId,
    inner: Box<dyn api::Tool>,
    /// The samples of the event being passed on, kept so that a stream of events takes no allocation.
    samples: Vec<api::Sample>,
}

impl ExtTool {
    /// Make the tool of `extension`.
    pub fn new(extension: &api::Extension) -> ExtTool {
        ExtTool { id: ToolId::named(extension.id), inner: (extension.create)(), samples: Vec::new() }
    }

    pub fn id(&self) -> ToolId {
        self.id
    }

    /// Time has passed: whether the tool is still moving by itself.
    pub fn tick(&mut self, now_ms: f64) -> bool {
        self.inner.tick(now_ms)
    }

    pub fn pages_edited(&mut self, ws: &mut Workspace) {
        self.inner.pages_edited(&mut Cx(ws));
    }
}

impl Tool for ExtTool {
    fn policy(&self, mode: Mode) -> InputPolicy {
        let p = self.inner.policy(mode == Mode::Desktop);
        let mouse = match p.mouse {
            api::MouseUse::Scrolls => MouseUse::Scrolls,
            api::MouseUse::Itself => MouseUse::Itself,
            api::MouseUse::AsStylus => MouseUse::AsStylus,
        };
        InputPolicy { stylus: p.stylus, mouse, keyboard: p.keyboard }
    }

    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>) {
        self.samples.clear();
        self.samples.extend(ev.samples.iter().map(|s| api::Sample { pos: s.pos, pressure: s.pressure, t_ms: s.t_ms }));
        let pen = api::PenButtons { in_range: ev.pen.in_range, contact: ev.pen.contact, eraser: ev.pen.eraser, barrel: ev.pen.barrel };
        let event = api::PointerEvent { device: device_in(ev.dev), id: ev.id, phase: phase_in(ev.phase), samples: &self.samples, pen };
        self.inner.pointer(&mut Cx(ws), &event);
    }

    fn owns(&self, dev: Device, id: u32) -> bool {
        self.inner.owns(device_in(dev), id)
    }

    fn busy(&self) -> bool {
        self.inner.busy()
    }

    fn end(&mut self, ws: &mut Workspace) {
        self.inner.end(&mut Cx(ws));
    }

    fn leave(&mut self, ws: &mut Workspace) {
        self.inner.leave(&mut Cx(ws));
    }

    fn cancel(&mut self, ws: &mut Workspace) {
        self.inner.cancel(&mut Cx(ws));
    }

    fn key(&mut self, ws: &mut Workspace, key: Key<'_>) -> bool {
        self.inner.key(&mut Cx(ws), key_in(key))
    }

    fn paint(&self, ws: &Workspace, out: &mut dyn Painter) {
        self.inner.paint(&CxRef(ws), &mut Out(out));
    }
}

#[allow(dead_code, reason = "the conversion the other way is for the tests of an extension through the program")]
pub fn device_of(d: api::Device) -> Device {
    device_out(d)
}
