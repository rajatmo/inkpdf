//! What the editor asks of the shell: opening and saving files, the context menu, settings.

use std::path::{Path, PathBuf};

use winit::window::Fullscreen;

use super::config;
use crate::doc::{Doc, UNTITLED};
use crate::editor::Request;
use crate::ink::PageStroke;
use crate::pages::PageEdit;
use crate::requests::Choice;
use crate::store::DocId;

use super::{InkStamp, State, folders};

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
        // (The Save As dialog is a prompt too: what is asked while it is up waits for the person to finish with it.)
        if self.ed.save_as_open() {
            return;
        }
        self.ed.settle();
        if !self.ed.ink.is_dirty() {
            self.run(then);
        } else if self.session.then.is_none() {
            self.session.then = Some(then);
            self.ed.ask_unsaved();
            self.window.request_redraw();
        }
    }

    fn run(&mut self, then: Then) {
        match then {
            Then::Exit => self.timing.exit = true,
            Then::Open(path) => self.open_now(path),
            Then::Blank => self.open_blank(),
        }
    }

    /// The prompt was answered.
    pub(super) fn answered(&mut self, choice: Choice) {
        let Some(then) = self.session.then.take() else { return };
        // A document with no file yet is asked where to go: what was waiting follows when the file has been written.
        if choice == Choice::Save && self.ed.ink.is_dirty() && self.session.doc.as_ref().is_some_and(|d| d.id.is_none()) {
            self.begin_save_as(Some(then));
            return;
        }
        if proceeds(choice, || self.save_now()) {
            self.run(then);
        }
    }
    /// Make `doc` the document being edited, with the ink it came with.
    fn install(&mut self, mut doc: Doc, strokes: Vec<PageStroke>) {
        eprintln!("opened {} — {} pages, {} ink strokes, {} ms", doc.name, doc.page_count(), strokes.len(), doc.open_millis);
        self.gpu.clear_textures();
        self.session.failed.clear();
        self.ed.load(doc.layout(), strokes);
        self.ed.links = doc.take_links();
        self.ed.outline = doc.take_outline();
        self.session.doc = Some(doc);
        self.session.save_then = None;
        self.layers.ink_built = InkStamp::NEVER;
        self.timing.last_zoom = self.ed.view.zoom;
        self.title.clear();
        self.window.request_redraw();
    }

    /// Open a PDF, after dealing with unsaved work.
    pub(super) fn open(&mut self, path: PathBuf) {
        self.after_settling(Then::Open(path));
    }

    fn open_now(&mut self, path: PathBuf) {
        match Doc::open_from(&*self.session.files, &DocId::Path(path.clone())) {
            Ok((doc, strokes)) => self.install(doc, strokes),
            Err(e) => {
                eprintln!("could not open {}: {e}", path.display());
                self.platform.error_box(&*self.window, &format!("Could not open {}:\n{e}", path.display()));
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

    /// Put the Save As dialog up, and let `then` (if there is one) wait for it. It starts in the folder the document is in, or in
    /// the Documents folder for a document that has no file yet, offering the document's name, or `Untitled.pdf`.
    pub(super) fn begin_save_as(&mut self, then: Option<Then>) {
        if self.ed.save_as_open() {
            return;
        }
        self.ed.settle();
        self.session.save_then = then;
        let file = self.session.doc.as_ref().and_then(|d| d.path().map(Path::to_path_buf));
        let dir = file.as_deref().and_then(Path::parent).filter(|p| p.is_dir()).map_or_else(|| self.platform.documents_dir(), Path::to_path_buf);
        let name = file.as_deref().and_then(Path::file_name).map_or_else(|| format!("{UNTITLED}.pdf"), |n| n.to_string_lossy().into_owned());
        self.ed.ask_save_as(dir, &name, self.platform.places());
        self.window.request_redraw();
    }

    /// The Save As dialog was confirmed with the file `id`: the document is written there, and becomes that file.
    fn save_to(&mut self, id: DocId) {
        let result = match &mut self.session.doc {
            Some(doc) => doc.save_as_to(&*self.session.files, &id, &self.ed.ink),
            None => Err("there is no document to save".to_string()),
        };
        match result {
            Ok(()) => {
                self.ed.document_saved();
                self.ed.save_as_done(Ok(()));
                self.title.clear();
                self.window.request_redraw();
                if let Some(then) = self.session.save_then.take() {
                    self.run(then);
                }
            }
            Err(e) => {
                eprintln!("save failed: {e}");
                self.ed.save_as_done(Err(format!("Could not save: {e}")));
            }
        }
    }

    /// The folders the dialog asked for that have been read: handed to the editor.
    fn take_listings(&mut self) {
        while let Ok(listed) = self.session.listings.1.try_recv() {
            self.ed.folder_listed(&listed.dir, listed.result);
        }
    }

    /// Write the ink into the PDF; a document with no file yet asks where to save it first.
    /// `true` on success.
    fn save_now(&mut self) -> bool {
        self.ed.settle();
        if !self.ed.ink.is_dirty() {
            return true;
        }
        let Some(has_file) = self.session.doc.as_ref().map(|d| d.id.is_some()) else { return true };
        // A document with no file yet is saved where the person says: the dialog asks, and the write follows from it
        // (see `save_to`), so nothing has been saved yet.
        if !has_file {
            self.begin_save_as(None);
            return false;
        }
        let Some(doc) = &mut self.session.doc else { return true };
        match doc.save_to(&*self.session.files, &self.ed.ink) {
            Ok(()) => {
                self.ed.document_saved();
                self.title.clear();
                self.window.request_redraw();
                true
            }
            Err(e) => {
                eprintln!("save failed: {e}");
                self.platform.error_box(&*self.window, &format!("Could not save:\n{e}"));
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

    /// The editor edited the pages: the file's page table follows, and what was drawn for the pages is out of date.
    fn pages_edited(&mut self, edit: &PageEdit) {
        let Some(doc) = &mut self.session.doc else { return };
        if !doc.apply(edit) {
            eprintln!("the document could not follow the edit of its pages: {edit:?}");
            return;
        }
        // (What is on the GPU is kept by the page of the file that draws it: a change of the pages leaves it as it is.)
        self.layers.ink_built = InkStamp::NEVER;
    }

    pub(super) fn request_close(&mut self) {
        // Closing the window puts the Save As dialog away first (what it was for is not going to happen).
        if self.ed.save_as_open() {
            self.ed.cancel_save_as();
            self.session.save_then = None;
        }
        self.after_settling(Then::Exit);
    }

    /// Fill the whole screen, borderless and over the taskbar, as games do (or go back to the window).
    /// The swap chain is already a flip-model one, so this costs no extra latency; there is no mode
    /// switch, so it is instant and alt-tabbing is clean.
    pub(super) fn toggle_fullscreen(&mut self) {
        let on = self.window.fullscreen().is_none();
        self.window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        self.ed.chrome.fullscreen = on;
        self.ed.want_redraw();
    }

    fn save_settings(&self) {
        if let Err(e) = config::save(self.platform.config_dir(), &self.ed.settings()) {
            eprintln!("could not save settings: {e}");
        }
    }

    /// Do what the editor asked for after an input event, in the order it asked.
    pub(super) fn service(&mut self) {
        self.take_listings();
        while let Some(request) = self.ed.requests().pop() {
            match request {
                Request::Answer(choice) => self.answered(choice),
                Request::SettingsChanged => {
                    self.save_settings();
                    self.sync_window_theme();
                }
                Request::Save => {
                    self.save_now();
                }
                Request::SaveAs => self.begin_save_as(None),
                Request::ListFolder(dir) => folders::spawn(dir, self.session.files.clone(), self.window.clone(), self.session.listings.0.clone()),
                Request::SaveTo(id) => self.save_to(id),
                Request::SaveAsCancelled => {
                    self.session.save_then = None;
                    self.window.request_redraw();
                }
                Request::Fullscreen => self.toggle_fullscreen(),
                Request::Close => self.close_document(),
                Request::PagesEdited(edit) => self.pages_edited(&edit),
                Request::GravesForgotten(graves) => {
                    if let Some(doc) = &mut self.session.doc {
                        doc.forget_graves(&graves);
                    }
                }
                // (A PDF can ask for any address; only a plain web or mail one is handed to the system.)
                Request::OpenUrl(url) => {
                    if let Some(url) = crate::links::safe_url(&url) {
                        self.platform.open_url(&url);
                    }
                }
            }
        }
        // The on-screen keyboard (a phone or tablet has none) is there while something wants typed text.
        let typing = self.ed.is_typing();
        if typing != self.keyboard_shown {
            self.keyboard_shown = typing;
            self.platform.set_text_input(typing);
        }
        if self.ed.take_redraw() {
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
