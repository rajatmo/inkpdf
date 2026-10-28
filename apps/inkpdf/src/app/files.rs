//! What the editor asks of the shell: opening and saving files, the context menu, settings.

use std::path::PathBuf;

use winit::window::Fullscreen;

use crate::doc::{Doc, UNTITLED};
use crate::editor::Request;
use crate::ink::PageStroke;
use crate::native;
use crate::settings;
use crate::toolbar::Choice;

use super::{InkStamp, State};

/// What to do once any unsaved work has been dealt with.
#[derive(Debug, PartialEq)]
pub(super) enum Then {
    Exit,
    Open(PathBuf),
    Blank,
}

/// Whether to go ahead with the action that was waiting, given the answer. Saving comes first and
/// the action only follows if the save worked.
fn proceeds(choice: Choice, save: impl FnOnce() -> bool) -> bool {
    match choice {
        Choice::Save => save(),
        Choice::Discard => true,
        Choice::Cancel => false,
    }
}

impl State {
    /// Do `then` now if nothing is unsaved; otherwise put up the app's own prompt and let `then`
    /// wait for the answer (see [`Self::answered`]). While a prompt is up, further requests are
    /// ignored.
    pub(super) fn after_settling(&mut self, then: Then) {
        self.ed.settle();
        if !self.ed.ink.is_dirty() {
            self.run(then);
        } else if self.then.is_none() {
            self.then = Some(then);
            self.ed.ask_unsaved();
            self.window.request_redraw();
        }
    }

    fn run(&mut self, then: Then) {
        match then {
            Then::Exit => self.exit = true,
            Then::Open(path) => self.open_now(path),
            Then::Blank => self.open_blank(),
        }
    }

    /// The prompt was answered.
    pub(super) fn answered(&mut self, choice: Choice) {
        let Some(then) = self.then.take() else { return };
        if proceeds(choice, || self.save_now()) {
            self.run(then);
        }
    }
    /// Make `doc` the document being edited, with the ink it came with.
    fn install(&mut self, doc: Doc, strokes: Vec<PageStroke>) {
        eprintln!("opened {} — {} pages, {} ink strokes, {} ms", doc.name, doc.page_count(), strokes.len(), doc.open_millis);
        self.gpu.clear_textures();
        self.failed.clear();
        self.ed.load(doc.layout.clone(), strokes);
        self.ed.links.clone_from(&doc.links);
        self.doc = Some(doc);
        self.ink_built = InkStamp::NEVER;
        self.last_zoom = self.ed.view.zoom;
        self.title.clear();
        self.window.request_redraw();
    }

    /// Open a PDF, after dealing with unsaved work.
    pub(super) fn open(&mut self, path: PathBuf) {
        self.after_settling(Then::Open(path));
    }

    fn open_now(&mut self, path: PathBuf) {
        match Doc::open(&path) {
            Ok((doc, strokes)) => self.install(doc, strokes),
            Err(e) => {
                eprintln!("could not open {}: {e}", path.display());
                native::error_box(&self.window, &format!("Could not open {}:\n{e}", path.display()));
            }
        }
    }

    /// Start a new document: one blank page, ready for writing. The work in progress is settled
    /// first by the callers that need it ([`Self::close_document`]).
    pub(super) fn open_blank(&mut self) {
        match Doc::blank(self.display_page()) {
            Ok(doc) => self.install(doc, Vec::new()),
            Err(e) => {
                eprintln!("could not make a blank document: {e}");
                self.set_title(format!("InkPDF — {e}"));
            }
        }
    }

    /// Close the document and open a blank one, after asking about unsaved ink.
    fn close_document(&mut self) {
        self.after_settling(Then::Blank);
    }

    /// Where to save a new document, asked for with the system's save window. `None` if cancelled.
    fn save_dialog(&self) -> Option<PathBuf> {
        let mut path = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{UNTITLED}.pdf")).save_file()?;
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        Some(path)
    }

    /// Write the ink into the PDF; a document with no file yet asks where to save it first.
    /// `true` on success.
    fn save_now(&mut self) -> bool {
        self.ed.settle();
        if !self.ed.ink.is_dirty() {
            return true;
        }
        let Some(has_file) = self.doc.as_ref().map(|d| d.path.is_some()) else { return true };
        // A document with no file yet is saved where the person says, or not at all.
        let new_path = if has_file {
            None
        } else if let Some(path) = self.save_dialog() {
            Some(path)
        } else {
            return false;
        };
        let Some(doc) = &mut self.doc else { return true };
        let result = match &new_path {
            Some(path) => doc.save_as(path, &self.ed.ink),
            None => doc.save(&self.ed.ink),
        };
        match result {
            Ok(()) => {
                self.ed.ink.mark_clean();
                self.title.clear();
                self.window.request_redraw();
                true
            }
            Err(e) => {
                eprintln!("save failed: {e}");
                native::error_box(&self.window, &format!("Could not save:\n{e}"));
                false
            }
        }
    }

    /// The size of a new blank page, in points: the display, so that at 100% the page is exactly as
    /// big as the screen (and as sharp). A4 if the display is not known.
    fn display_page(&self) -> [f32; 2] {
        let monitor = self.window.current_monitor().or_else(|| self.window.primary_monitor());
        let scale = self.window.scale_factor() as f32;
        match monitor.map(|m| m.size()) {
            // 100% is 96 pixels to the inch, and a point is 1/72 inch.
            Some(size) if size.width > 0 && size.height > 0 && scale > 0.0 => [size.width as f32 * 0.75 / scale, size.height as f32 * 0.75 / scale],
            _ => crate::doc::BLANK_PAGE,
        }
    }

    /// Insert a blank page as page `at`, the size of page `like`, and glide to it.
    fn insert_page(&mut self, at: usize, like: usize) {
        let Some(doc) = &mut self.doc else { return };
        let old_count = doc.page_count();
        if doc.insert_blank_page(at, like).is_none() {
            return;
        }
        self.ed.page_inserted(doc.layout.clone(), at);
        self.ed.links.clone_from(&doc.links);
        if at < old_count {
            // Pages after it have new numbers: what is on the GPU for them is out of date.
            self.gpu.clear_textures();
            self.failed.clear();
        }
        self.ink_built = InkStamp::NEVER;
    }

    /// A finger pulled past the end of the document: add a blank page, the size of the last, and
    /// glide to it.
    fn add_page(&mut self) {
        let Some(count) = self.doc.as_ref().map(Doc::page_count) else { return };
        self.insert_page(count, count.saturating_sub(1));
    }

    pub(super) fn request_close(&mut self) {
        self.after_settling(Then::Exit);
    }

    /// Fill the whole screen, borderless and over the taskbar, as games do (or go back to the window).
    /// The swap chain is already a flip-model one, so this costs no extra latency; there is no mode
    /// switch, so it is instant and alt-tabbing is clean.
    pub(super) fn toggle_fullscreen(&mut self) {
        let on = self.window.fullscreen().is_none();
        self.window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        self.ed.fullscreen = on;
        self.ed.redraw = true;
    }

    fn save_settings(&self) {
        if let Err(e) = settings::save(&self.ed.settings()) {
            eprintln!("could not save settings: {e}");
        }
    }

    /// Do what the editor asked for after an input event, in the order it asked.
    pub(super) fn service(&mut self) {
        while let Some(request) = self.ed.requests.pop() {
            match request {
                Request::Answer(choice) => self.answered(choice),
                Request::SettingsChanged => self.save_settings(),
                Request::Save => {
                    self.save_now();
                }
                Request::Fullscreen => self.toggle_fullscreen(),
                Request::Close => self.close_document(),
                Request::AddPage => self.add_page(),
                Request::InsertPage(after) => self.insert_page(after + 1, after),
                Request::OpenUrl(url) => native::open_url(&url),
            }
        }
        if std::mem::take(&mut self.ed.redraw) {
            self.window.request_redraw();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_comes_first_and_the_action_only_follows_if_it_worked() {
        assert!(proceeds(Choice::Save, || true));
        assert!(!proceeds(Choice::Save, || false), "a save that failed or was cancelled stops everything");
    }

    #[test]
    fn discarding_goes_ahead_without_saving_and_cancelling_stops_without_saving() {
        assert!(proceeds(Choice::Discard, || panic!("must not save")));
        assert!(!proceeds(Choice::Cancel, || panic!("must not save")));
    }
}
