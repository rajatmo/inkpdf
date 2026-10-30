//! The shell: the window, the GPU, the file, and the frame loop. Everything the input devices do
//! to the document is in [`crate::editor::Editor`] (and `editor/`); this module hands
//! it events, does what it asks (save, open, show the menu), and draws what it holds.
//!
//! - `files`: opening, saving, the context menu, settings.
//! - `frame`: one frame, from acquiring the swap-chain image to presenting.
//! - `pages`: laying out the page tiles as quads.
//! - `ink_layer`: committed ink, overlays and the toolbar on the GPU.
//! - `text_layer`: typed text, as bitmaps drawn over their pages.
//! - `bench`: `--bench` and `--stats`.

mod bench;
mod config;
mod files;
mod folders;
mod frame;
mod ink_layer;
mod overview;
mod pages;
mod replay;
mod text_layer;
mod translate;

use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key as Typed, ModifiersState};
use winit::window::{Icon, Window, WindowLevel};

use crate::doc::Doc;
use crate::editor::Editor;
use crate::gpu::{Gpu, GpuChoice};
use crate::input::{InputEvent, Mode};
use crate::platform::{self, Platform, PointerSource};
use crate::store::FileStore;
use crate::text;
use crate::tiles::Key;
use bench::Bench;
use ink_layer::{InkStamp, UiStamp};

/// The icon in the title bar and taskbar: the same drawing as the one embedded in the exe.
fn window_icon() -> Option<Icon> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(include_bytes!("../../installer/app-64.png").as_slice())).read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut pixels).ok()?;
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return None;
    }
    pixels.truncate(frame.buffer_size());
    Icon::from_rgba(pixels, frame.width, frame.height).ok()
}

pub struct Options {
    pub path: Option<PathBuf>,
    pub bench: bool,
    pub exit_after: Option<Duration>,
    pub topmost: bool,
    /// Start filling the whole screen.
    pub fullscreen: bool,
    pub gpu: GpuChoice,
    /// Print pen-to-screen latency on exit.
    pub stats: bool,
    /// Start with all the ink on the first page selected, for screenshots.
    pub select_all: bool,
    /// How much the pen is steadied, 0 to 1, instead of what was saved.
    pub smoothing: Option<f32>,
    /// The mode to keep whatever the machine's posture is (--mode desktop|tablet); None follows the posture.
    pub mode: Option<Mode>,
    /// Save the input given to the editor to this file when the program ends (--record).
    pub record: Option<PathBuf>,
    /// Play this recorded input, then close (--replay).
    pub replay: Option<PathBuf>,
}

/// The frame loop's own business: its clock, what it is waiting for, how long things took.
struct FrameLoop {
    frame: u64,
    last_frame: Instant,
    last_zoom: f32,
    last_zoom_change: Instant,
    wake_at: Option<Instant>,
    bench: Option<Bench>,
    exit_at: Option<Instant>,
    exit: bool,
    print_stats: bool,
    /// Milliseconds from a pen sample's timestamp to the frame carrying it being handed to the GPU.
    latency_ms: Vec<f32>,
    /// Milliseconds from a pen sample's timestamp to the moment the frame read it (the input queue's share).
    read_age_ms: Vec<f32>,
    /// Whether a frame is being drawn right now (see `redraw`).
    drawing: bool,
    /// The app is in the background and has no window to draw to (Android): nothing is drawn, and nothing asks to be.
    suspended: bool,
    /// The events taken from the pointer source each frame, kept so that their room is.
    pointer_buf: Vec<crate::input::PointerEvent>,
    other_buf: Vec<InputEvent>,
}

/// The open document and what waits on it: the file, the store it lives in, and the questions asked about unsaved work.
struct DocumentSession {
    doc: Option<Doc>,
    /// Reads the text of the document's pages behind the frame.
    indexer: Option<crate::indexer::Indexer>,
    /// Where files are read and written (see `store`); the platform's.
    files: Arc<dyn FileStore>,
    /// Tiles the renderer could not draw, so they are not asked for again.
    failed: HashSet<Key>,
    /// What is waiting on the unsaved-changes prompt.
    then: Option<files::Then>,
    /// What is waiting on the Save As dialog: it follows when the file is written, and is dropped if the dialog is cancelled.
    save_then: Option<files::Then>,
    /// The folders read for the Save As dialog arrive here, from threads of their own (see [`folders`]).
    listings: (Sender<folders::Listed>, Receiver<folders::Listed>),
}

/// What is on the GPU for the window, and what it was last built from.
struct UiLayers {
    /// The sharp tiles the current view needs, from the last plan.
    visible: Vec<Key>,
    /// What the GPU's committed-ink buffer was last built from, and which pages own which part.
    ink_built: InkStamp,
    ink_pages: Vec<(u32, Range<u32>)>,
    /// What the toolbar and pop-ups on the GPU were last built from.
    ui_built: Option<UiStamp>,
    /// The pages overview's pictures and text, while it is up.
    overview: overview::Shown,
    /// The line work over the ink, rebuilt each frame into the same buffers (their room is kept).
    overlay_buf: ink_layer::Overlays,
    /// The text boxes to draw this frame, and where the ink's own are.
    text_draws: Vec<text_layer::TextDraw>,
    text_index: text_layer::TextIndex,
}

/// What the presenters (the methods of [`UiLayers`]) read and write besides the layers themselves: the editor to show, the GPU to put it on,
/// the document whose pictures are used, and the number of the frame being made. Built from the fields of [`State`] with
/// [`paint!`], so the borrow is of those fields alone and `layers` stays free to be borrowed mutably beside it.
pub(super) struct Paint<'a> {
    pub ed: &'a Editor,
    pub gpu: &'a mut Gpu,
    pub doc: Option<&'a Doc>,
    pub frame: u64,
}

macro_rules! paint {
    ($state:ident) => {
        $crate::app::Paint { ed: &$state.ed, gpu: &mut $state.gpu, doc: $state.session.doc.as_ref(), frame: $state.timing.frame }
    };
}
pub(super) use paint;

/// The shell: the window and the GPU, the editor, and three plain groups of what the shell keeps (no indirection on the frame
/// path: they are fields, not objects).
pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    ed: Editor,
    timing: FrameLoop,
    session: DocumentSession,
    layers: UiLayers,
    title: String,
    /// The OS's own pen and touch reader (see `platform`); `None` when it could not be installed, and winit's basic input is used.
    pointer: Option<Box<dyn PointerSource>>,
    /// What this operating system provides (folders, clipboard, error boxes, the pen reader); built once at start.
    platform: Box<dyn Platform>,
    /// Whether the on-screen keyboard was last asked for (see `platform::set_text_input`).
    keyboard_shown: bool,
    /// The mode was chosen on the command line, so a change of posture does not change it.
    mode_forced: bool,
    /// Which of Ctrl, Shift, … are held (for the desktop mode's shortcuts).
    modifiers: ModifiersState,
    /// The look the window's title bar was last told to have.
    last_appearance: crate::settings::Appearance,
    /// The input given to the editor, being copied to a file (`--record`).
    recorder: Option<replay::Recorder>,
    /// A recording being played (`--replay`).
    player: Option<replay::Player>,
}

impl State {
    /// Whether the app has been asked to end (by its window, a command-line limit, or the benchmark finishing).
    pub fn exiting(&self) -> bool {
        self.timing.exit
    }

    pub fn new(el: &ActiveEventLoop, opts: Options) -> Result<State, String> {
        let mut attrs = Window::default_attributes().with_title("InkPDF").with_inner_size(LogicalSize::new(1100.0, 900.0)).with_maximized(true);
        if opts.topmost {
            attrs = attrs.with_window_level(WindowLevel::AlwaysOnTop);
        }
        attrs = attrs.with_window_icon(window_icon());
        let window = Arc::new(el.create_window(attrs).map_err(|e| format!("could not create a window: {e}"))?);
        let gpu = Gpu::new(window.clone(), &opts.gpu)?;
        eprintln!("GPU: {} ({}, {})", gpu.adapter_name, gpu.backend, gpu.device_type);
        let platform = platform::native();
        if let Some(font) = platform.system_font() {
            text::set_font(font);
        }
        let wake = {
            let window = window.clone();
            Arc::new(move || window.request_redraw())
        };
        let pointer = match platform.install_pointer(&*window, wake) {
            Ok(pointer) => Some(pointer),
            Err(e) => {
                eprintln!("pen and touch fall back to basic input: {e}");
                None
            }
        };
        let size = window.inner_size();
        let window_scale = window.scale_factor() as f32;
        let now = Instant::now();
        let mut state = State {
            ed: Editor::new([size.width.max(1) as f32, size.height.max(1) as f32], window_scale, platform.motion_prefs()),
            window,
            gpu,
            timing: FrameLoop {
                frame: 0,
                last_frame: now,
                last_zoom: 1.0,
                last_zoom_change: now,
                wake_at: None,
                bench: opts.bench.then(|| Bench::new(now)),
                exit_at: opts.exit_after.map(|d| now + d),
                exit: false,
                print_stats: opts.stats,
                latency_ms: Vec::new(),
                read_age_ms: Vec::new(),
                drawing: false,
                suspended: false,
                pointer_buf: Vec::new(),
                other_buf: Vec::new(),
            },
            session: DocumentSession {
                doc: None,
                files: platform.file_store(),
                indexer: None,
                failed: HashSet::new(),
                then: None,
                save_then: None,
                listings: std::sync::mpsc::channel(),
            },
            layers: UiLayers {
                visible: Vec::new(),
                ink_built: InkStamp::NEVER,
                ink_pages: Vec::new(),
                ui_built: None,
                overview: overview::Shown::default(),
                overlay_buf: ink_layer::Overlays::default(),
                text_draws: Vec::new(),
                text_index: text_layer::TextIndex::default(),
            },
            title: String::new(),
            pointer,
            platform,
            keyboard_shown: false,
            mode_forced: opts.mode.is_some(),
            modifiers: ModifiersState::empty(),
            last_appearance: crate::settings::Appearance::default(),
            recorder: opts.record.map(replay::Recorder::new),
            player: opts.replay.as_deref().map(replay::Player::load).transpose()?,
        };
        // (A recording starts from the window as it is, so that it replays from the same.)
        let (size, dpr) = ([size.width.max(1) as f32, size.height.max(1) as f32], window_scale);
        state.feed(InputEvent::Resized(size));
        state.feed(InputEvent::ScaleChanged(dpr));
        state.ed.apply_settings(&config::load(state.platform.config_dir()));
        state.refresh_system_scheme();
        state.sync_window_theme();
        // Text copied from a box can be pasted anywhere, and text copied anywhere into a box.
        if let Some(clipboard) = state.platform.clipboard(&*state.window) {
            state.ed.set_text_clipboard(clipboard);
        }
        state.ed.set_can_open(state.platform.has_picker());
        state.set_mode(opts.mode.unwrap_or_else(|| state.platform.detect_mode()));
        if let Some(smoothing) = opts.smoothing {
            state.ed.set_smoothing(smoothing);
        }
        if let Some(path) = opts.path {
            state.open(path);
            if state.session.doc.is_none() {
                state.open_blank();
            }
            // (A document another app handed this one is opened in its place.)
            state.take_picked();
            if opts.select_all {
                state.ed.select_all_on_page();
            }
        } else {
            state.open_blank();
        }
        // After the surface exists: asking for full screen while the window is being created
        // leaves the swap chain with an invalid surface.
        if opts.fullscreen {
            state.toggle_fullscreen();
        }
        state.window.request_redraw();
        Ok(state)
    }

    /// Let the OS's own reader take the input that is waiting before the toolkit does (see `PointerSource::pump_early`).
    pub fn early_pump(&mut self) {
        if let Some(pointer) = &mut self.pointer {
            pointer.pump_early();
        }
    }

    /// The window was taken away (the app went to the background): no drawing until [`Self::resumed`].
    pub fn suspended(&mut self) {
        self.timing.suspended = true;
        self.gpu.pause();
    }

    /// A window is back (the app came to the foreground): the surface is made again at the window's size.
    pub fn resumed(&mut self) {
        let size = self.window.inner_size();
        self.gpu.resume(size.width, size.height);
        self.ed.set_viewport([size.width.max(1) as f32, size.height.max(1) as f32]);
        self.timing.suspended = false;
        self.take_picked();
        self.window.request_redraw();
    }

    fn set_title(&mut self, title: String) {
        if title != self.title {
            self.window.set_title(&title);
            self.title = title;
        }
    }

    pub fn window_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.request_close(),
            WindowEvent::Resized(size) => {
                self.gpu.resize(size.width, size.height);
                self.feed(InputEvent::Resized([size.width as f32, size.height as f32]));
                self.ed.chrome.fullscreen = self.window.fullscreen().is_some();
            }
            WindowEvent::CursorLeft { .. } => self.feed(InputEvent::MouseLeft),
            WindowEvent::Focused(false) => self.feed(InputEvent::FocusLost),
            // (What was asked of motion in Windows' settings is read again when the window is come back to.)
            WindowEvent::Focused(true) => {
                self.ed.motions.set_prefs(self.platform.motion_prefs());
                self.refresh_system_scheme();
                // (Back from the system's file picker: what it answered is taken.)
                self.take_picked();
                self.window.request_redraw();
            }
            WindowEvent::ThemeChanged(_) => self.refresh_system_scheme(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.feed(InputEvent::ScaleChanged(scale_factor as f32)),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::DroppedFile(path) => self.open(path),
            WindowEvent::CursorMoved { position, .. } => self.feed(InputEvent::MouseMoved([position.x as f32, position.y as f32])),
            WindowEvent::MouseInput { state, button, .. } => {
                self.feed(InputEvent::MouseButton { state: translate::button_state(state), button: translate::button(button) })
            }
            WindowEvent::MouseWheel { delta, .. } => self.feed(InputEvent::Wheel(translate::wheel(delta))),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => self.key_pressed(&event),
            // With the pointer layer installed pen and touch never reach winit; this is only the
            // fallback if it could not be installed.
            // A touch the toolkit reports. With the OS's own reader installed it has taken pen and touch before the toolkit saw them
            // (a few arrive in the moment between: the reader makes them what they already were); without one, it is a finger.
            WindowEvent::Touch(t) => {
                let ev = translate::touch(&t, crate::clock::now_ms());
                match &mut self.pointer {
                    Some(pointer) => pointer.late_touch(ev),
                    None => self.feed(InputEvent::Pointer(ev)),
                }
            }
            _ => {}
        }
        self.service();
    }

    /// Give the editor an event, keeping a copy if the input is being recorded.
    fn feed(&mut self, event: InputEvent) {
        if let Some(recorder) = &mut self.recorder {
            recorder.push(&event);
        }
        self.ed.handle(event);
    }

    /// Ask what the system's look is: the platform first (it knows `INKPDF_THEME`, and the desktop's settings where the toolkit cannot), then the
    /// window (Windows says it). Light when nobody knows.
    fn refresh_system_scheme(&mut self) {
        use crate::palette::Scheme;
        let scheme = self
            .platform
            .color_scheme()
            .or_else(|| self.window.theme().map(|t| if t == winit::window::Theme::Dark { Scheme::Dark } else { Scheme::Light }))
            .unwrap_or(Scheme::Light);
        self.ed.set_system_scheme(scheme);
    }

    /// The title bar follows the look the user chose (and the system's when they left it to the system), where the toolkit can do it.
    fn sync_window_theme(&mut self) {
        use crate::settings::Appearance;
        let appearance = self.ed.appearance();
        self.window.set_theme(match appearance {
            Appearance::System => None,
            Appearance::Light => Some(winit::window::Theme::Light),
            Appearance::Dark => Some(winit::window::Theme::Dark),
        });
        self.last_appearance = appearance;
    }

    /// Change how the devices are used (the posture changed, or it was asked for on the command line).
    fn set_mode(&mut self, mode: Mode) {
        if std::env::var_os("INKPDF_TRACE").is_some() {
            eprintln!("mode: {mode:?}");
        }
        self.ed.set_mode(mode);
        self.window.request_redraw();
    }

    /// The keyboard types into the text box being edited; with Ctrl held, in the desktop mode, it has a few
    /// shortcuts; nothing else hears it.
    fn key_pressed(&mut self, event: &KeyEvent) {
        if self.modifiers.control_key() {
            if let Typed::Character(letter) = &event.logical_key {
                self.feed(InputEvent::Shortcut { letter: letter.to_string(), shift: self.modifiers.shift_key() });
            }
            return;
        }
        // The tool in use says which of these it hears (see `InputPolicy`): a text box being typed into takes them
        // all; with something selected, Delete, Escape and the arrows work on that.
        if let Some(key) = translate::key(&event.logical_key, event.text.as_deref(), self.modifiers.shift_key(), self.ed.is_typing()) {
            self.feed(InputEvent::Key(key));
        }
    }

    pub fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Pen and touch messages queued by the window procedure: handle them now and draw at
        // once, without waiting for the next paint message, so ink follows the pen as closely as
        // the display allows.
        if self.pointer.as_ref().is_some_and(|p| p.pending()) {
            self.redraw();
            self.service();
        }
        // (Where nothing says the system's look changed, look again: a change is one call, and nothing else.)
        if self.platform.watches_color_scheme() {
            let before = self.ed.scheme();
            self.refresh_system_scheme();
            if self.ed.scheme() != before {
                self.window.request_redraw();
            }
        }
        // A convertible was turned into a tablet, or back.
        if self.pointer.as_mut().is_some_and(|p| p.take_posture_change()) && !self.mode_forced {
            self.set_mode(self.platform.detect_mode());
        }
        // `--exit-after` must fire even when the window is idle and nothing is redrawing.
        if self.timing.exit_at.is_some_and(|t| Instant::now() >= t) {
            self.timing.exit = true;
        }
        if self.timing.exit {
            if std::mem::take(&mut self.timing.print_stats) {
                self.report_latency();
            }
            if let Some(pointer) = &mut self.pointer {
                pointer.uninstall();
            }
            el.exit();
            return;
        }
        let mut wake = self.timing.exit_at;
        if let Some(t) = self.timing.wake_at {
            if Instant::now() >= t {
                self.timing.wake_at = None;
                self.window.request_redraw();
            } else {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
        }
        el.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
