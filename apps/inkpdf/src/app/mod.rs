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
mod files;
mod frame;
mod ink_layer;
mod pages;
mod text_layer;

use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key as Typed, NamedKey};
use winit::window::{Icon, Window, WindowLevel};

use crate::doc::Doc;
use crate::editor::Editor;
use crate::gpu::{Gpu, GpuChoice};
use crate::settings;
use crate::tiles::Key;
use crate::win_pointer;
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
}

pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    doc: Option<Doc>,
    ed: Editor,
    frame: u64,
    last_frame: Instant,
    last_zoom: f32,
    last_zoom_change: Instant,
    failed: HashSet<Key>,
    /// The sharp tiles the current view needs, from the last plan.
    visible: Vec<Key>,
    title: String,
    wake_at: Option<Instant>,
    bench: Option<Bench>,
    exit_at: Option<Instant>,
    pub exit: bool,
    pointer_layer: bool,
    print_stats: bool,
    /// Milliseconds from a pen sample's timestamp to the frame carrying it being handed to the GPU.
    latency_ms: Vec<f32>,
    /// Milliseconds from a pen sample's timestamp to the moment the frame read it (the input queue's share).
    read_age_ms: Vec<f32>,
    /// What the GPU's committed-ink buffer was last built from, and which pages own which part.
    ink_built: InkStamp,
    ink_pages: Vec<(u32, Range<u32>)>,
    /// What the toolbar and pop-ups on the GPU were last built from.
    ui_built: Option<UiStamp>,
    /// The line work over the ink, rebuilt each frame into the same buffers (their room is kept).
    overlay_buf: ink_layer::Overlays,
    /// The text boxes to draw this frame, and where the ink's own are.
    text_draws: Vec<text_layer::TextDraw>,
    text_index: text_layer::TextIndex,
    /// Whether a frame is being drawn right now (see `redraw`).
    drawing: bool,
    /// What is waiting on the unsaved-changes prompt.
    then: Option<files::Then>,
}

impl State {
    pub fn new(el: &ActiveEventLoop, opts: Options) -> Result<State, String> {
        let mut attrs = Window::default_attributes().with_title("InkPDF").with_inner_size(LogicalSize::new(1100.0, 900.0)).with_maximized(true);
        if opts.topmost {
            attrs = attrs.with_window_level(WindowLevel::AlwaysOnTop);
        }
        attrs = attrs.with_window_icon(window_icon());
        let window = Arc::new(el.create_window(attrs).map_err(|e| format!("could not create a window: {e}"))?);
        let gpu = Gpu::new(window.clone(), &opts.gpu)?;
        eprintln!("GPU: {} ({}, {})", gpu.adapter_name, gpu.backend, gpu.device_type);
        let pointer_layer = match win_pointer::install(&window) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("pen and touch fall back to basic input: {e}");
                false
            }
        };
        let size = window.inner_size();
        let now = Instant::now();
        let mut state = State {
            ed: Editor::new([size.width.max(1) as f32, size.height.max(1) as f32], window.scale_factor() as f32),
            window,
            gpu,
            doc: None,
            frame: 0,
            last_frame: now,
            last_zoom: 1.0,
            last_zoom_change: now,
            failed: HashSet::new(),
            visible: Vec::new(),
            title: String::new(),
            wake_at: None,
            bench: opts.bench.then(|| Bench::new(now)),
            exit_at: opts.exit_after.map(|d| now + d),
            exit: false,
            pointer_layer,
            print_stats: opts.stats,
            latency_ms: Vec::new(),
            read_age_ms: Vec::new(),
            ink_built: InkStamp::NEVER,
            ink_pages: Vec::new(),
            ui_built: None,
            overlay_buf: ink_layer::Overlays::default(),
            text_draws: Vec::new(),
            text_index: text_layer::TextIndex::default(),
            drawing: false,
            then: None,
        };
        state.ed.apply_settings(&settings::load());
        if let Some(smoothing) = opts.smoothing {
            state.ed.smoothing = smoothing.clamp(0.0, 1.0);
        }
        if let Some(path) = opts.path {
            state.open(path);
            if state.doc.is_none() {
                state.open_blank();
            }
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
                self.ed.set_viewport([size.width as f32, size.height as f32]);
                self.ed.fullscreen = self.window.fullscreen().is_some();
            }
            WindowEvent::CursorLeft { .. } => self.ed.mouse_left(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.ed.dpr = scale_factor as f32,
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::DroppedFile(path) => self.open(path),
            WindowEvent::CursorMoved { position, .. } => self.ed.mouse_moved([position.x as f32, position.y as f32]),
            WindowEvent::MouseInput { state, button, .. } => self.ed.mouse_button(state, button),
            WindowEvent::MouseWheel { delta, .. } => self.ed.wheel(delta),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => self.key_pressed(&event),
            // With the pointer layer installed pen and touch never reach winit; this is only the
            // fallback if it could not be installed.
            WindowEvent::Touch(t) if !self.pointer_layer => self.ed.basic_touch(t.phase, t.id, [t.location.x as f32, t.location.y as f32]),
            _ => {}
        }
        self.service();
    }

    /// The keyboard types into the text box being edited, and does nothing else.
    fn key_pressed(&mut self, event: &KeyEvent) {
        if !self.ed.is_typing() {
            return;
        }
        match &event.logical_key {
            Typed::Named(NamedKey::Backspace) => self.ed.backspace(),
            Typed::Named(NamedKey::Enter) => self.ed.type_text("\n"),
            Typed::Named(NamedKey::Escape) => self.ed.escape(),
            _ => {
                if let Some(text) = &event.text {
                    self.ed.type_text(text);
                }
            }
        }
    }

    pub fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Pen and touch messages queued by the window procedure: handle them now and draw at
        // once, without waiting for the next paint message, so ink follows the pen as closely as
        // the display allows.
        if self.pointer_layer && win_pointer::pending() {
            self.redraw();
            self.service();
        }
        // `--exit-after` must fire even when the window is idle and nothing is redrawing.
        if self.exit_at.is_some_and(|t| Instant::now() >= t) {
            self.exit = true;
        }
        if self.exit {
            if std::mem::take(&mut self.print_stats) {
                self.report_latency();
            }
            win_pointer::uninstall(&self.window);
            el.exit();
            return;
        }
        let mut wake = self.exit_at;
        if let Some(t) = self.wake_at {
            if Instant::now() >= t {
                self.wake_at = None;
                self.window.request_redraw();
            } else {
                wake = Some(wake.map_or(t, |w| w.min(t)));
            }
        }
        el.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
