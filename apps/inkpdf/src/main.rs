//! InkPDF: a fast, pen-first PDF annotator for Windows.
//!
//! `inkpdf [file.pdf] [--gpu integrated|discrete|auto|NAME] [--list-gpus] [--bench] [--stats]
//! [--exit-after SECONDS] [--topmost] [--fullscreen]`

// The release build has no console window of its own (it joins the terminal it was started from,
// if any, in `main`). Debug builds keep the console for logging.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(test)]
mod architecture;
mod clipboard;
mod clock;
mod colour;
mod coords;
mod doc;
mod editor;
mod figure;
mod geometry;
mod gpu;
mod ink;
mod ink_io;
mod input;
mod laser;
mod links;
mod native;
mod outline;
mod pages;
mod plane;
mod posture;
mod requests;
mod scene;
mod selection;
mod settings;
mod smooth;
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
mod win_pointer;
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
        if self.state.is_some() {
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

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(state) = &mut self.state {
            state.window_event(event);
            if state.exit {
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

const USAGE: &str = "usage: inkpdf [file.pdf] [--gpu integrated|discrete|auto|NAME] [--list-gpus] [--smoothing 0..1] [--mode desktop|tablet|auto] [--bench] [--stats] [--exit-after SECONDS] [--topmost] [--fullscreen]";

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

fn main() {
    // Explorer's thumbnail handler runs this program as a helper: no window, no terminal.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--thumbnail") {
        std::process::exit(thumbnail::run_child(args.next().and_then(|n| n.parse().ok()).unwrap_or(256)));
    }
    native::attach_parent_console();
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            native::report_fatal(&format!("{e}\n{USAGE}"));
            std::process::exit(2);
        }
    };
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            native::report_fatal(&format!("could not start the event loop: {e}"));
            std::process::exit(1);
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App { options: Some(options), state: None, startup_error: None };
    if let Err(e) = event_loop.run_app(&mut app) {
        native::report_fatal(&format!("InkPDF stopped: {e}"));
        std::process::exit(1);
    }
    if let Some(e) = app.startup_error {
        native::report_fatal(&format!("InkPDF cannot start: {e}"));
        std::process::exit(1);
    }
}
