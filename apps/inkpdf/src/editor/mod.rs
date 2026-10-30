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
//! - The keyboard types into the text box being edited and edits its text; in the desktop mode it also
//!   works on the selection (Delete, Esc, arrows) and has shortcuts (see `routing`).
//!
//! The rest is shared: `dispatch` (who owns the interaction under way), `select` (moving ink and
//! finishing what the pen was doing), `toolbar_ui` (buttons and the tool panel), `context` (the bar
//! under the tools: draw / select / erase and what is done to the selection), `commands` (undo and
//! redo).

mod act;
mod appearance;
mod chrome;
mod commands;
mod context;
mod dispatch;
mod modal;
mod motions;
mod mouse;
mod navigation;
mod overview;
mod pages;
mod pen;
mod prompt;
mod routing;
mod save_as;
mod search;
mod select;
mod text_pages;
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
pub use chrome::Chrome;

impl std::ops::Deref for Editor {
    type Target = Workspace;

    fn deref(&self) -> &Workspace {
        &self.ws
    }
}

#[cfg(test)]
impl std::ops::DerefMut for Editor {
    fn deref_mut(&mut self) -> &mut Workspace {
        &mut self.ws
    }
}

pub struct Editor {
    /// The document state the tools work on (the editor derefs to it: `ed.ink`, `ed.view`, …).
    ws: Workspace,
    pub cursor: [f32; 2],
    /// The document's links, in document space; only a finger's tap follows one.
    pub links: Vec<crate::links::Link>,
    /// The document's table of contents (see [`crate::outline`]); empty if it has none.
    pub outline: Vec<crate::outline::OutlineItem>,
    /// How far a finger has pulled up past the end of the document, in pixels: a long enough pull
    /// adds a blank page.
    pub pull: f32,
    /// A page was already asked for in this pull.
    pull_fired: bool,
    /// The last name given to a place where deleted pages are kept (see [`crate::pages::PageEdit::Delete`]).
    grave: u64,
    /// What was kept by page and went with the pages of each deletion, until it is undone or the file is written.
    graves: HashMap<u64, pages::Grave>,
    /// The bar under the tools as it last was, kept while it goes out (see [`Editor::step_motions`]).
    held_bar: Option<(toolbar::ContextBar, toolbar::ContextState)>,
    /// The search card: what was typed, what it found (see [`search`]).
    pub search: search::Search,
    /// The tools there are, as the toolbar lists them (the program's own and the extensions').
    registry: crate::tool::Registry,
    /// The extensions those tools came from, to make them afresh when a document is loaded.
    extensions: Vec<inkpdf_api::Extension>,
    /// The tools that own their interaction (see [`crate::tools`]).
    pub tools: ToolSet,
    /// The window's own controls (see [`Chrome`]).
    pub chrome: Chrome,
    /// Every animation of the interface (see [`motions`]).
    pub motions: motions::Motions,
    /// How the machine is being used: what the mouse and keyboard do depends on it.
    mode: Mode,
    /// Milliseconds now, on the clock the pointer messages are stamped with (see [`crate::clock`]); a field so that tests can give another.
    clock: fn() -> f64,
    /// The mouse button held on the page, if any.
    mouse_press: Option<mouse::MousePress>,
    pub palm: PalmGuard,
    pub fingers: TouchGestures,
    /// The last pen sample the screen has not yet shown, for latency measurement.
    pub pending_sample_ms: Option<f64>,
}

impl Editor {
    /// An editor for a window of `viewport` pixels at `dpr`, moving as `prefs` say (what the user asked of motion).
    pub fn new(viewport: [f32; 2], dpr: f32, prefs: crate::motion::Prefs) -> Self {
        Self::with_extensions(viewport, dpr, prefs, &crate::tools::extensions())
    }

    /// An editor for tests: the program's tools and the reference extension (see [`crate::tool::reference_extensions`]).
    #[cfg(test)]
    pub fn reference(viewport: [f32; 2], dpr: f32, prefs: crate::motion::Prefs) -> Self {
        Self::with_extensions(viewport, dpr, prefs, &crate::tool::reference_extensions())
    }

    /// The same, with the tools of `extensions` instead of the ones this build has (a test's own, or a fork's).
    pub fn with_extensions(viewport: [f32; 2], dpr: f32, prefs: crate::motion::Prefs, extensions: &[inkpdf_api::Extension]) -> Self {
        let registry = extensions.iter().fold(crate::tool::Registry::builtin(), |r, e| r.with(crate::tool::Tool::named(e.id), e.icon_svg));
        Self {
            registry,
            extensions: extensions.to_vec(),
            ws: Workspace::new(viewport, dpr),
            cursor: [0.0; 2],
            links: Vec::new(),
            outline: Vec::new(),
            pull: 0.0,
            pull_fired: false,
            grave: 0,
            graves: HashMap::new(),
            held_bar: None,
            search: search::Search::default(),
            tools: ToolSet::with_extensions(extensions),
            chrome: Chrome::default(),
            motions: motions::Motions::new(prefs),
            mode: Mode::default(),
            clock: crate::clock::now_ms,
            mouse_press: None,
            palm: PalmGuard::default(),
            fingers: TouchGestures::default(),
            pending_sample_ms: None,
        }
    }

    /// Start editing a new document: its pages and the ink already in it.
    pub fn load(&mut self, layout: Layout, strokes: Vec<PageStroke>) {
        self.ws.layout = layout;
        self.ws.has_doc = true;
        self.ws.view = View::new(self.ws.view.viewport);
        self.ws.ink.load(strokes);
        self.ws.selection.clear();
        self.ws.intent = crate::input::Intent::Draw;
        self.tools = ToolSet::with_extensions(&self.extensions);
        self.ws.erasing.clear();
        self.search.close();
        self.chrome.clear_press_and_prompt();
        self.mouse_press = None;
        self.ws.requests.clear_answer();
        self.links.clear();
        self.outline.clear();
        self.graves.clear();
        self.pull = 0.0;
        self.fingers = TouchGestures::default();
        self.ws.fit_mode = true;
        self.ws.fit_width();
        self.ws.redraw = true;
    }

    /// End what is being typed or drawn by taps, so that it is part of the document (before the tool
    /// changes, the file is saved, a page is added).
    pub fn settle(&mut self) {
        self.tools.settle_all(&mut self.ws);
    }

    pub fn toolbar(&self) -> Toolbar {
        Toolbar::layout_for(&self.registry, self.ws.view.viewport, self.ws.dpr)
    }

    /// Everything the toolbars' look depends on.
    pub fn toolbar_state(&self) -> toolbar::State {
        toolbar::State {
            can_undo: self.ws.ink.can_undo(),
            can_redo: self.ws.ink.can_redo(),
            can_delete: self.ws.layout.len() > 1,
            unsaved: self.ws.ink.is_dirty(),
            fullscreen: self.chrome.fullscreen,
            mode: self.mode,
            appearance: self.ws.appearance,
            dark: self.scheme() == crate::palette::Scheme::Dark,
            dark_pages: self.ws.dark_pages,
            search: self.search.open,
            tool: self.ws.tool,
            pen_colour: self.colour_of(self.ws.pen),
            shape_colour: self.colour_of(self.ws.shape_style),
            text_colour: self.colour_of(self.ws.text_style),
            hover: self.chrome.hover,
            pressed: match self.chrome.act {
                Act::Toolbar { item, .. } => item,
                _ => None,
            },
        }
    }

    /// The toolbar as it looks now, in the middle of whatever is moving in it.
    pub fn toolbar_look(&self) -> toolbar::Look {
        let rest = toolbar::Look::resting(&self.toolbar_state(), &self.toolbar());
        self.motions.toolbar_look(&rest)
    }

    /// The prompt as it is drawn now, and how much of it: it comes in and goes out over a moment.
    pub fn prompt_look(&self) -> Option<(toolbar::PromptView, f32)> {
        self.motions.prompt_look(self.chrome.prompt)
    }

    /// The tool's panel as it is drawn now, and how much of it.
    pub fn panel_look(&self) -> Option<(toolbar::PanelView, f32)> {
        self.motions.panel_look(self.panel_view())
    }

    /// Everything the tool panel's look depends on, while it is open.
    pub fn panel_view(&self) -> Option<toolbar::PanelView> {
        self.chrome.panel.map(|p| toolbar::PanelView {
            tool: self.ws.tool,
            editing: p.editing,
            width: self.style().width,
            smoothing: self.ws.smoothing,
            shape: self.ws.shape,
            colour: self.style().colour,
            palette: self.ws.palette,
            hsv: self.chrome.colour_hsv,
            zoom: self.ws.view.zoom,
        })
    }

    /// Take the saved settings: the tool, and each tool's colours, thickness and options.
    pub fn apply_settings(&mut self, settings: &crate::settings::Settings) {
        self.ws.appearance = settings.appearance;
        self.ws.dark_pages = settings.dark_pages;
        self.search.case = settings.search_case;
        self.search.words = settings.search_words;
        self.sync_theme();
        self.ws.palette = settings.palette;
        self.ws.tool = settings.tool;
        self.ws.pen = settings.pen;
        self.ws.smoothing = settings.smoothing;
        self.ws.shape = settings.shape;
        self.ws.shape_style = settings.shape_style;
        self.ws.text_style = settings.text_style;
        self.tools = ToolSet::with_extensions(&self.extensions);
        self.chrome.panel = None;
        self.ws.redraw = true;
    }

    /// What is worth remembering for the next session.
    pub fn settings(&self) -> crate::settings::Settings {
        crate::settings::Settings {
            appearance: self.ws.appearance,
            dark_pages: self.ws.dark_pages,
            search_case: self.search.case,
            search_words: self.search.words,
            palette: self.ws.palette,
            tool: self.ws.tool,
            pen: self.ws.pen,
            smoothing: self.ws.smoothing,
            shape: self.ws.shape,
            shape_style: self.ws.shape_style,
            text_style: self.ws.text_style,
        }
    }

    /// Time has passed: whether any extension's tool is still moving by itself (a trail fading), which keeps the frames coming.
    pub fn tick_tools(&mut self, now_ms: f64) -> bool {
        self.tools.tick(now_ms)
    }

    /// Take the time from clock instead (a test's own, that it can set).
    #[cfg(test)]
    pub fn set_clock(&mut self, clock: fn() -> f64) {
        self.clock = clock;
    }

    // What the shell may do to the document state. The rest of it is the editor's own: the shell reads it (through
    // `Deref`) and asks for changes in the editor's terms, so that nothing outside the editor and its tools edits the
    // drawing by reaching into a field.

    /// Scroll the view by `(dx, dy)` pixels.
    pub fn pan_px(&mut self, dx: f32, dy: f32) {
        self.ws.pan_px(dx, dy);
    }

    /// One frame of the view's glide and zoom animation: whether it is still moving.
    pub fn step_view(&mut self, dt: f32) -> bool {
        let overview = self.step_overview(dt);
        let interface = self.step_motions(dt);
        self.refresh_search();
        self.ws.theme = self.palette();
        self.ws.step_view(dt) || overview || interface
    }

    /// One frame of the interface's own animations: they are told where the toolbar is heading, and move there.
    fn step_motions(&mut self, dt: f32) -> bool {
        let (state, bar) = (self.toolbar_state(), self.toolbar());
        let rest = toolbar::Look::resting(&state, &bar);
        self.motions.retarget_toolbar(&rest, state.pressed.and_then(|item| bar.slot(item)));
        let context = self.context_bar().zip(self.context_view());
        if context.is_some() {
            self.held_bar = context;
        }
        self.motions.retarget_drawers(self.chrome.prompt, self.panel_view(), context.is_some());
        self.motions.retarget_theme(self.scheme() == crate::palette::Scheme::Dark);
        let moving = self.motions.step(dt);
        // (Once it is all the way out there is nothing of it to draw.)
        if context.is_none() && self.motions.bar_amount() <= 0.0 {
            self.held_bar = None;
        }
        moving
    }

    /// The window changed size: the view follows it.
    pub fn set_viewport(&mut self, size: [f32; 2]) {
        self.ws.set_viewport(size);
        // (The pages overview is laid out by the window: what it keeps is brought back into range.)
        self.overview_settled();
    }

    /// The view itself, for the shell's benchmark to drive.
    pub fn view_mut(&mut self) -> &mut View {
        &mut self.ws.view
    }

    /// The window's scale changed.
    pub fn set_dpr(&mut self, dpr: f32) {
        self.ws.dpr = dpr;
        self.overview_settled();
    }

    /// How much the pen is steadied while drawing, from the command line.
    pub fn set_smoothing(&mut self, amount: f32) {
        self.ws.smoothing = amount.clamp(0.0, 1.0);
    }

    /// Where text is copied to and pasted from (the system's clipboard).
    pub fn set_text_clipboard(&mut self, clipboard: Box<dyn crate::clipboard::TextClipboard>) {
        self.ws.text_clipboard = clipboard;
    }

    /// The document was written to its file: nothing is unsaved.
    pub fn document_saved(&mut self) {
        self.ws.ink.mark_clean();
        // The file has lost the pages that were deleted: they cannot be put back into it.
        self.ws.ink.forget_deleted_pages();
        self.graves.clear();
    }

    /// What the editor asks of the shell (see [`crate::requests`]).
    pub fn requests(&mut self) -> &mut crate::requests::Requests {
        &mut self.ws.requests
    }

    /// Whether a frame is wanted, and no longer.
    pub fn take_redraw(&mut self) -> bool {
        std::mem::take(&mut self.ws.redraw)
    }

    /// A frame is wanted (something outside the editor changed what is shown).
    pub fn want_redraw(&mut self) {
        self.ws.redraw = true;
    }
    pub fn viewport_centre(&self) -> [f32; 2] {
        [self.ws.view.viewport[0] / 2.0, self.ws.view.viewport[1] / 2.0]
    }

    /// 100% is 96 device-independent pixels per inch.
    pub fn actual_size_zoom(&self) -> f32 {
        self.ws.dpr * 96.0 / 72.0
    }
}
