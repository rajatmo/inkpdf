//! The tools: what a press on the page does. Each tool is an object that owns its own interaction state
//! (a drag in progress, a text box being typed into, a trail) and works on the shared [`Workspace`]; none
//! knows about another tool, the toolbar, the prompt, the window or the GPU. The session routes input to the
//! tool in use according to its [`InputPolicy`], and asks it to paint its own overlay through a
//! [`Painter`].
//!
//! Adding a tool is one file implementing [`Tool`] and one line in [`ToolSet`].

mod ext;
pub mod gestures;
pub mod items;
pub mod paint;
mod pen;
mod picker;
mod select;
mod shape;
mod shortcuts;
mod text;

use crate::input::{Device, Mode, PenButtons, Phase, Sample};
use crate::tool::{Tool as ToolId, ToolAction};
use crate::workspace::Workspace;

pub use ext::ExtTool;
pub use paint::Painter;
pub use pen::PenTool;
pub use picker::PickTool;
pub use select::SelectTool;
#[cfg(test)]
pub use shape::Draft;
pub use shape::ShapeTool;
pub use text::{TextEdit, TextTool};

/// How near, in logical pixels, a tap must be to a shape or a text box to select it.
pub const TAP_SELECT_PX: f32 = 8.0;

/// What the mouse's buttons do on the page. (The wheel always scrolls.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseUse {
    /// Nothing: the mouse only scrolls.
    Scrolls,
    /// Its presses reach the tool as a mouse's (the text tool: click to place, drag to move).
    Itself,
    /// It stands in for a stylus: the left button is the tip, the right the eraser end, the middle the barrel
    /// button. (The tool is handed a stylus; it never knows.)
    AsStylus,
}

/// Which devices a tool listens to on the page. (A finger always pans and zooms and never reaches a tool;
/// every device can still press the buttons of the toolbars, which are the window's, not the page's.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputPolicy {
    /// The stylus: tip, eraser end and barrel button.
    pub stylus: bool,
    pub mouse: MouseUse,
    /// The keyboard.
    pub keyboard: bool,
}

impl InputPolicy {
    /// A tool the stylus drives (pen, shape, pointer): in the desktop the mouse can too, in the tablet it
    /// only scrolls.
    pub fn stylus_tool(mode: Mode) -> Self {
        let mouse = if mode == Mode::Desktop { MouseUse::AsStylus } else { MouseUse::Scrolls };
        Self { stylus: true, mouse, keyboard: false }
    }

    /// A stylus tool that makes items (pen, shape): in the desktop mode the keyboard also works on its selection
    /// (delete it, let go of it, nudge it); a tablet has no keyboard to rely on.
    pub fn editing_tool(mode: Mode) -> Self {
        Self { keyboard: mode == Mode::Desktop, ..Self::stylus_tool(mode) }
    }
}

/// A pointer message for a tool: a stylus or mouse on the page.
#[derive(Clone, Copy, Debug)]
pub struct PageEvent<'a> {
    pub dev: Device,
    pub id: u32,
    pub phase: Phase,
    pub samples: &'a [Sample],
    pub pen: PenButtons,
}

// (The keyboard's vocabulary is the program's input vocabulary: see `input::Key`.)
pub use crate::input::{Dir, Key};

pub trait Tool {
    /// Which devices the tool listens to in `mode`.
    fn policy(&self, mode: Mode) -> InputPolicy;

    /// A stylus or mouse message on the page (only ones the policy lets through).
    fn pointer(&mut self, ws: &mut Workspace, ev: &PageEvent<'_>);

    /// Whether a gesture of this pointer is under way (so that its moves and release are the tool's).
    fn owns(&self, dev: Device, id: u32) -> bool;

    /// Whether any gesture, or work not yet made part of the document, is under way.
    #[cfg_attr(not(test), allow(dead_code))]
    fn busy(&self) -> bool;

    /// A press elsewhere, or the pen lifting: finish the gesture under way, committing it.
    fn end(&mut self, ws: &mut Workspace);

    /// Make whatever is pending part of the document (typing, a polyline being drawn): before the tool
    /// changes, the file is saved, a page is added.
    fn settle(&mut self, ws: &mut Workspace) {
        self.end(ws);
    }

    /// The tool is being put away: end the gesture under way (a press may still be down) and settle what is
    /// pending, so that nothing of its own is left to surface later, when something else ends gestures.
    fn leave(&mut self, ws: &mut Workspace) {
        self.end(ws);
        self.settle(ws);
    }

    /// The gesture under way is called off without being committed (a prompt came up).
    fn cancel(&mut self, ws: &mut Workspace);

    /// The keyboard, if the policy lets it through. `true` if it was used.
    fn key(&mut self, _ws: &mut Workspace, _key: Key<'_>) -> bool {
        false
    }

    /// Undo and redo ask the tool first: it takes back its own pending work (the last node, what was
    /// typed) and returns `true`, or leaves them to the document's history.
    fn undo(&mut self, _ws: &mut Workspace) -> bool {
        false
    }

    fn redo(&mut self, _ws: &mut Workspace) -> bool {
        false
    }

    /// The colour or size in the tool's panel changed.
    fn restyled(&mut self, _ws: &mut Workspace) {}

    /// The actions of its own the tool offers now, as buttons of the bar (what it is editing decides which): the bar
    /// shows them without knowing what they are.
    fn actions(&self, _ws: &Workspace) -> &'static [ToolAction] {
        &[]
    }

    /// Whether `action` can be done now (a button that cannot is dimmed).
    fn action_enabled(&self, _ws: &Workspace, _action: ToolAction) -> bool {
        false
    }

    /// Do `action`, as one undo step. `true` if it did something.
    fn act(&mut self, _ws: &mut Workspace, _action: ToolAction) -> bool {
        false
    }

    /// How far the selection is being dragged, in document space.
    fn drag_offset(&self) -> [f32; 2] {
        [0.0, 0.0]
    }

    /// What the tool shows over the page: its drafts, trails, frames.
    fn paint(&self, _ws: &Workspace, _out: &mut dyn Painter) {}
}

/// The extensions this build has, each already checked (one that the program cannot serve is left out, and said on the terminal).
pub fn extensions() -> Vec<inkpdf_api::Extension> {
    inkpdf_ext::all()
        .into_iter()
        .filter(|e| match e.problem() {
            Some(why) => {
                eprintln!("extension left out: {why}");
                false
            }
            None => true,
        })
        .collect()
}

/// The tools, by name: the one place that knows which there are. The program's own are fields; an extension's are held in a list and spoken
/// to through the API.
pub struct ToolSet {
    pub pen: PenTool,
    pub shape: ShapeTool,
    pub text: TextTool,
    pub select: SelectTool,
    pub lasso: PickTool,
    pub eraser: PickTool,
    ext: Vec<ExtTool>,
}

impl ToolSet {
    /// The program's tools and those of the extensions this build has.
    pub fn new() -> Self {
        Self::with_extensions(&extensions())
    }

    /// The program's tools and those of `extensions` (a test's own).
    pub fn with_extensions(extensions: &[inkpdf_api::Extension]) -> Self {
        Self {
            pen: PenTool::default(),
            shape: ShapeTool::default(),
            text: TextTool::default(),
            select: SelectTool::default(),
            lasso: PickTool::default(),
            eraser: PickTool::default(),
            ext: extensions.iter().map(ExtTool::new).collect(),
        }
    }

    /// The tool, if there is one of that name (the selection's if not: a name from a settings file that an extension has left).
    pub fn get(&self, id: ToolId) -> &dyn Tool {
        match id {
            ToolId::Pen => &self.pen,
            ToolId::Geometry => &self.shape,
            ToolId::Text => &self.text,
            ToolId::Select => &self.select,
            ToolId::Lasso => &self.lasso,
            ToolId::Eraser => &self.eraser,
            other => match self.ext.iter().find(|t| t.id() == other) {
                Some(t) => t,
                None => &self.select,
            },
        }
    }

    pub fn get_mut(&mut self, id: ToolId) -> &mut dyn Tool {
        match id {
            ToolId::Pen => &mut self.pen,
            ToolId::Geometry => &mut self.shape,
            ToolId::Text => &mut self.text,
            ToolId::Select => &mut self.select,
            ToolId::Lasso => &mut self.lasso,
            ToolId::Eraser => &mut self.eraser,
            other => match self.ext.iter().position(|t| t.id() == other) {
                Some(at) => &mut self.ext[at],
                None => &mut self.select,
            },
        }
    }

    fn each(&mut self) -> Vec<&mut dyn Tool> {
        let mut all: Vec<&mut dyn Tool> = vec![&mut self.pen, &mut self.shape, &mut self.text, &mut self.select, &mut self.lasso, &mut self.eraser];
        all.extend(self.ext.iter_mut().map(|t| t as &mut dyn Tool));
        all
    }

    /// Whether any tool has a gesture, or work not yet made part of the document, under way.
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        [&self.pen as &dyn Tool, &self.shape, &self.text, &self.select, &self.lasso, &self.eraser].iter().any(|t| t.busy())
            || self.ext.iter().any(|t| t.busy())
    }

    /// Time has passed: whether any extension's tool is still moving by itself (the frames go on while one is).
    pub fn tick(&mut self, now_ms: f64) -> bool {
        self.ext.iter_mut().fold(false, |moving, t| t.tick(now_ms) | moving)
    }

    /// The pages were edited: the extensions' tools hear of it.
    pub fn pages_edited(&mut self, ws: &mut Workspace) {
        self.ext.iter_mut().for_each(|t| t.pages_edited(ws));
    }

    /// Finish the gesture of every tool (a new press, a new page).
    pub fn end_all(&mut self, ws: &mut Workspace) {
        self.each().into_iter().for_each(|t| t.end(ws));
    }

    /// Make the pending work of every tool part of the document.
    pub fn settle_all(&mut self, ws: &mut Workspace) {
        self.each().into_iter().for_each(|t| t.settle(ws));
    }

    /// Put every tool away.
    pub fn leave_all(&mut self, ws: &mut Workspace) {
        self.each().into_iter().for_each(|t| t.leave(ws));
    }

    /// Call off the gesture of every tool.
    pub fn cancel_all(&mut self, ws: &mut Workspace) {
        self.each().into_iter().for_each(|t| t.cancel(ws));
    }
}

impl Default for ToolSet {
    fn default() -> Self {
        Self::new()
    }
}
