//! InkPDF: a fast, pen-first PDF annotator (Windows first; Linux and Android on the port branch).
//!
//! `inkpdf [file.pdf] [--gpu integrated|discrete|auto|NAME] [--list-gpus] [--bench] [--stats]
//! [--exit-after SECONDS] [--topmost] [--fullscreen]`
//!
//! The program is this library; the `inkpdf` binary only calls [`run`], and the `inkpdf-android` library calls
//! `run_android`, so that each host has its own entry point and nothing else of its own.

mod app;
#[cfg(test)]
mod architecture;
mod browse;
mod clipboard;
mod clock;
mod colour;
mod coords;
mod doc;
mod doctext;
mod editor;
mod figure;
mod geometry;
mod gpu;
mod indexer;
mod ink;
mod ink_io;
mod input;
mod links;
mod motion;
mod outline;
mod page_table;
mod pages;
mod pagetext;
mod palette;
mod path_store;
mod pdf;
mod pdf_read;
mod plane;
mod platform;
mod requests;
mod scene;
mod selection;
mod settings;
mod smooth;
mod store;
#[cfg(test)]
mod stress;
#[cfg(test)]
mod testutil;
mod text;
mod thumbnail;
mod tiles;
mod tool;
mod toolbar;
mod tools;
mod view;
mod workspace;

use std::path::PathBuf;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

struct App {
    options: Option<app::Options>,
    state: Option<app::State>,
    /// Why the window could not be set up, reported once the event loop has ended.
    startup_error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        // (Android takes the window away when the app is put in the background and gives one back: the window object stays, its
        // surface is made again.)
        if let Some(state) = &mut self.state {
            state.resumed();
            return;
        }
        let Some(options) = self.options.take() else { return };
        match app::State::new(el, options) {
            Ok(state) => self.state = Some(state),
            Err(e) => {
                self.startup_error = Some(e);
                el.exit();
            }
        }
    }

    fn new_events(&mut self, _el: &ActiveEventLoop, _cause: winit::event::StartCause) {
        // (The first thing of every turn of the toolkit's loop: before it reads the input queue, a source that shares it reads it.)
        if let Some(state) = &mut self.state {
            state.early_pump();
        }
    }

    fn suspended(&mut self, _el: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            state.suspended();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(state) = &mut self.state {
            state.window_event(event);
            if state.exiting() {
                el.exit();
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            state.about_to_wait(el);
        }
    }
}

const USAGE: &str = "usage: inkpdf [file.pdf] [--gpu integrated|discrete|auto|NAME] [--list-gpus] [--smoothing 0..1] [--mode desktop|tablet|auto] [--bench] [--stats] [--record FILE] [--replay FILE] [--exit-after SECONDS] [--topmost] [--fullscreen]";

fn parse_args() -> Result<app::Options, String> {
    let mut options = app::Options {
        path: None,
        bench: false,
        exit_after: None,
        topmost: false,
        fullscreen: false,
        gpu: gpu::GpuChoice::default(),
        stats: false,
        select_all: false,
        smoothing: None,
        mode: None,
        record: None,
        replay: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bench" => options.bench = true,
            "--topmost" => options.topmost = true,
            "--fullscreen" => options.fullscreen = true,
            "--stats" => options.stats = true,
            "--select-all" => options.select_all = true,
            "--smoothing" => {
                options.smoothing =
                    args.next().and_then(|v| v.parse().ok()).map(Some).ok_or("--smoothing needs a number from 0 (off) to 1 (strong)")?
            }
            "--mode" => {
                options.mode = match args.next().as_deref() {
                    Some("auto") => None,
                    Some(name) => Some(input::Mode::parse(name).ok_or("--mode needs desktop, tablet or auto")?),
                    None => return Err("--mode needs desktop, tablet or auto".into()),
                }
            }
            "--record" => options.record = Some(args.next().ok_or("--record needs a file to write the input to")?.into()),
            "--replay" => options.replay = Some(args.next().ok_or("--replay needs a recorded file")?.into()),
            "--list-gpus" => {
                for gpu in gpu::list_gpus() {
                    println!("{gpu}");
                }
                std::process::exit(0);
            }
            "--gpu" => options.gpu = gpu::GpuChoice::parse(&args.next().ok_or("--gpu needs integrated, discrete, auto or part of a GPU's name")?),
            "--exit-after" => {
                let secs: f32 = args.next().and_then(|v| v.parse().ok()).ok_or("--exit-after needs a number of seconds")?;
                options.exit_after = Some(Duration::from_secs_f32(secs.max(0.0)));
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => options.path = Some(PathBuf::from(path)),
        }
    }
    Ok(options)
}

/// The program: what `main` of the `inkpdf` binary does.
pub fn run() {
    // Explorer's thumbnail handler runs this program as a helper: no window, no terminal.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--thumbnail") {
        std::process::exit(thumbnail::run_child(args.next().and_then(|n| n.parse().ok()).unwrap_or(256)));
    }
    platform::attach_parent_console();
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            platform::report_fatal(&format!("{e}\n{USAGE}"));
            std::process::exit(2);
        }
    };
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            platform::report_fatal(&format!("could not start the event loop: {e}"));
            std::process::exit(1);
        }
    };
    serve(event_loop, options);
}

/// Run the event loop until the window is closed; a failure is reported and ends the process with a status.
fn serve(event_loop: EventLoop<()>, options: app::Options) {
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App { options: Some(options), state: None, startup_error: None };
    if let Err(e) = event_loop.run_app(&mut app) {
        platform::report_fatal(&format!("InkPDF stopped: {e}"));
        std::process::exit(1);
    }
    if let Some(e) = app.startup_error {
        platform::report_fatal(&format!("InkPDF cannot start: {e}"));
        std::process::exit(1);
    }
}

/// The program on Android: the activity (`android_main` of the `inkpdf-android` library) hands over its `AndroidApp`, which is
/// kept for what only it can say (where the app's own folders are, the soft keyboard) and given to winit's event loop.
#[cfg(target_os = "android")]
pub fn run_android(android: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    platform::android_app_started(android.clone());
    let event_loop = match EventLoop::builder().with_android_app(android).build() {
        Ok(el) => el,
        Err(e) => {
            platform::report_fatal(&format!("could not start the event loop: {e}"));
            return;
        }
    };
    let options = app::Options {
        path: platform::welcome_document(),
        bench: false,
        exit_after: None,
        topmost: false,
        fullscreen: false,
        gpu: gpu::GpuChoice::default(),
        stats: false,
        select_all: false,
        smoothing: None,
        mode: None,
        record: None,
        replay: None,
    };
    serve(event_loop, options);
}
