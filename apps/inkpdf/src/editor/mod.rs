//! The editor: everything the input devices do to the document, with no window and no GPU, so
//! the whole behaviour can be tested by feeding it events.
//!
//! The shell around it ([`crate::app::State`]) owns the window, the GPU and the file; the editor
//! asks it for the things only it can do (save, close, redraw) through the `want_*` flags and
//! `redraw`.
//!
//! Each device does one kind of thing (see [`crate::input`]); one file per device:
//!
//! - `pen` (the stylus): everything done to the page. It draws; its eraser end erases strokes; its
//!   barrel button draws a lasso. With the shape tool (`shapes`) it drags out shapes, selects them,
//!   and edits them by their anchor points and handles (`anchors`); with the pointer tool
//!   (`laser`) it points instead and draws nothing.
//! - `touch`: pans and zooms; `navigation`: its taps on links and its pull for a new page.
//! - `mouse`: scrolls, and presses the buttons of the toolbars; with the text tool it places, selects
//!   and moves text boxes.
//! - The keyboard types into the text box being edited, and does nothing else.
//!
//! The rest is shared: `dispatch` (who owns the interaction under way), `select` (moving ink and
//! finishing what the pen was doing), `toolbar_ui` (buttons and the tool panel), `commands`
//! (undo and redo).

mod act;
mod commands;
mod dispatch;
mod mouse;
mod navigation;
mod pen;
mod prompt;
mod routing;
mod select;
mod toolbar_ui;
mod touch;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::ink::PageStroke;
use crate::input::{Mode, PalmGuard, TouchGestures};
use crate::toolbar::{self, Toolbar};
use crate::tools::ToolSet;
use crate::view::{Layout, View};

pub use crate::requests::Request;
pub use crate::workspace::Workspace;
pub(crate) use act::Act;

impl std::ops::Deref for Editor {
    type Target = Workspace;

    fn deref(&self) -> &Workspace {
        &self.ws
    }
}

impl std::ops::DerefMut for Editor {
    fn deref_mut(&mut self) -> &mut Workspace {
        &mut self.ws
    }
}

pub struct Editor {
    /// The document state the tools work on (the editor derefs to it: `ed.ink`, `ed.view`, …).
    ws: Workspace,
    pub cursor: [f32; 2],
    /// The toolbar button the mouse is over.
    pub hover: Option<toolbar::Item>,
    /// Whether the window fills the screen (kept in step with the window by the shell).
    pub fullscreen: bool,
    /// The "save your changes?" prompt, while it is up (see [`Editor::ask_unsaved`]).
    pub prompt: Option<crate::toolbar::PromptView>,
    prompt_grab: Option<prompt::Grab>,
    /// The document's links, in document space; only a finger's tap follows one.
    pub links: Vec<crate::links::Link>,
    /// How far a finger has pulled up past the end of the document, in pixels: a long enough pull
    /// adds a blank page.
    pub pull: f32,
    /// A page was already asked for in this pull.
    pull_fired: bool,
    /// The panel under the tool's button, while it is open.
    pub panel: Option<toolbar::PanelState>,
    /// The colour being made in the colour editor (kept as hue, saturation and brightness, so a
    /// grey does not forget its hue).
    pub colour_hsv: crate::colour::Hsv,
    /// The tools that own their interaction (see [`crate::tools`]).
    pub tools: ToolSet,
    pub act: Act,
    /// How the machine is being used: what the mouse and keyboard do depends on it.
    mode: Mode,
    /// The mouse button held on the page, if any.
    mouse_press: Option<mouse::MousePress>,
    pub palm: PalmGuard,
    pub fingers: TouchGestures,
    /// The last pen sample the screen has not yet shown, for latency measurement.
    pub pending_sample_ms: Option<f64>,

    /// Touch through winit's own events: only when the pointer layer is unavailable.
    basic_touches: HashMap<u64, [f32; 2]>,
    basic_pinch: Option<(f32, [f32; 2])>,
}

impl Editor {
    pub fn new(viewport: [f32; 2], dpr: f32) -> Self {
        Self {
            ws: Workspace::new(viewport, dpr),
            cursor: [0.0; 2],
            hover: None,
            fullscreen: false,
            prompt: None,
            prompt_grab: None,
            links: Vec::new(),
            pull: 0.0,
            pull_fired: false,
            panel: None,
            colour_hsv: [0.0; 3],
            tools: ToolSet::new(),
            act: Act::Idle,
            mode: Mode::default(),
            mouse_press: None,
            palm: PalmGuard::default(),
            fingers: TouchGestures::default(),
            pending_sample_ms: None,
            basic_touches: HashMap::new(),
            basic_pinch: None,
        }
    }

    /// Start editing a new document: its pages and the ink already in it.
    pub fn load(&mut self, layout: Layout, strokes: Vec<PageStroke>) {
        self.layout = layout;
        self.has_doc = true;
        self.view = View::new(self.view.viewport);
        self.ink.load(strokes);
        self.selection.clear();
        self.tools = ToolSet::new();
        self.erasing.clear();
        self.act = Act::Idle;
        self.mouse_press = None;
        self.prompt = None;
        self.requests.clear_answer();
        self.prompt_grab = None;
        self.links.clear();
        self.pull = 0.0;
        self.fingers = TouchGestures::default();
        self.fit_mode = true;
        self.fit_width();
        self.redraw = true;
    }

    /// A blank page was inserted as page `at` (at the end, or among the others): take the new
    /// layout, carry the ink on the pages that moved down with them, mark the page changed (so
    /// saving writes it) and glide to it.
    pub fn page_inserted(&mut self, layout: Layout, at: usize) {
        // Whatever the pen was in the middle of was in the old layout.
        if !matches!(self.act, Act::Idle) {
            self.end_act();
        }
        self.tools.end_all(&mut self.ws);
        self.settle();
        let shift = match (self.layout.page(at), layout.page(at + 1)) {
            (Some(was), Some(now)) => now.y - was.y,
            _ => 0.0,
        };
        self.ink.insert_page_at(at as u32, shift);
        self.ink.mark_dirty(at as u32);
        self.layout = layout;
        self.tools.pointer.laser.clear();
        self.scroll_to_page(at);
        self.redraw = true;
    }

    /// End what is being typed or drawn by taps, so that it is part of the document (before the tool
    /// changes, the file is saved, a page is added).
    pub fn settle(&mut self) {
        self.tools.settle_all(&mut self.ws);
    }

    pub fn toolbar(&self) -> Toolbar {
        Toolbar::layout(self.view.viewport, self.dpr)
    }

    /// Everything the toolbars' look depends on.
    pub fn toolbar_state(&self) -> toolbar::State {
        toolbar::State {
            can_undo: self.ink.can_undo(),
            can_redo: self.ink.can_redo(),
            unsaved: self.ink.is_dirty(),
            fullscreen: self.fullscreen,
            tool: self.tool,
            pen_colour: self.colour_of(self.pen),
            shape_colour: self.colour_of(self.shape_style),
            text_colour: self.colour_of(self.text_style),
            hover: self.hover,
        }
    }

    /// Everything the tool panel's look depends on, while it is open.
    pub fn panel_view(&self) -> Option<toolbar::PanelView> {
        self.panel.map(|p| toolbar::PanelView {
            tool: self.tool,
            editing: p.editing,
            width: self.style().width,
            smoothing: self.smoothing,
            shape: self.shape,
            colour: self.style().colour,
            palette: self.palette,
            hsv: self.colour_hsv,
            zoom: self.view.zoom,
        })
    }

    /// Take the saved settings: the tool, and each tool's colours, thickness and options.
    pub fn apply_settings(&mut self, settings: &crate::settings::Settings) {
        self.palette = settings.palette;
        self.tool = settings.tool;
        self.pen = settings.pen;
        self.smoothing = settings.smoothing;
        self.shape = settings.shape;
        self.shape_style = settings.shape_style;
        self.text_style = settings.text_style;
        self.tools = ToolSet::new();
        self.panel = None;
        self.redraw = true;
    }

    /// What is worth remembering for the next session.
    pub fn settings(&self) -> crate::settings::Settings {
        crate::settings::Settings {
            palette: self.palette,
            tool: self.tool,
            pen: self.pen,
            smoothing: self.smoothing,
            shape: self.shape,
            shape_style: self.shape_style,
            text_style: self.text_style,
        }
    }

    pub fn viewport_centre(&self) -> [f32; 2] {
        [self.view.viewport[0] / 2.0, self.view.viewport[1] / 2.0]
    }

    /// 100% is 96 device-independent pixels per inch.
    pub fn actual_size_zoom(&self) -> f32 {
        self.dpr * 96.0 / 72.0
    }
}
