//! The system's own file dialogs on a desktop (Windows' and, through the desktop portal, Linux's): Save As and Open. A dialog blocks until it is
//! answered, so it runs on a thread of its own, parented to the window (so that it stays in front of it), and what it answered waits to be
//! taken by the shell, which is woken to take it. Android's picker is an `Intent` instead (see `saf`); both come back as a [`Picked`].

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{Pick, Picked, Waker, WindowHandles};
use crate::store::DocId;

#[derive(Default)]
pub struct Dialogs {
    answered: Arc<Mutex<VecDeque<Picked>>>,
    wake: Arc<Mutex<Option<Waker>>>,
    /// A dialog is up: another is not opened over it.
    open: Arc<AtomicBool>,
}

impl Dialogs {
    /// How to wake the event loop when a dialog has been answered.
    pub fn set_waker(&self, wake: Waker) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    /// Open the dialog for `kind` in `dir`, over `window`; `name` is the file offered when saving.
    pub fn pick(&self, window: &dyn WindowHandles, kind: Pick, name: &str, dir: &Path) {
        if self.open.swap(true, Ordering::SeqCst) {
            return;
        }
        let dialog = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_parent(window);
        let dialog = if dir.is_dir() { dialog.set_directory(dir) } else { dialog };
        let dialog = match kind {
            Pick::Open => dialog.set_title("Open a PDF"),
            Pick::Save => dialog.set_title("Save as").set_file_name(name),
        };
        let (answered, wake, open) = (self.answered.clone(), self.wake.clone(), self.open.clone());
        let spawned = std::thread::Builder::new().name("file-dialog".into()).spawn(move || {
            let path = match kind {
                Pick::Open => dialog.pick_file(),
                Pick::Save => dialog.save_file().map(with_pdf_extension),
            };
            if let Ok(mut queue) = answered.lock() {
                queue.push_back(Picked { kind, id: path.map(DocId::Path) });
            }
            open.store(false, Ordering::SeqCst);
            if let Some(wake) = wake.lock().ok().and_then(|w| w.clone()) {
                wake();
            }
        });
        if let Err(e) = spawned {
            eprintln!("could not open a file dialog: {e}");
            self.open.store(false, Ordering::SeqCst);
        }
    }

    /// What a dialog answered since this was last asked.
    pub fn take(&self) -> Option<Picked> {
        self.answered.lock().ok()?.pop_front()
    }
}

/// A name typed without its extension is a PDF.
fn with_pdf_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() { path } else { path.with_extension("pdf") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_without_an_extension_is_a_pdf_and_one_with_it_is_left_alone() {
        assert_eq!(with_pdf_extension("/a/b/notes".into()), PathBuf::from("/a/b/notes.pdf"));
        assert_eq!(with_pdf_extension("/a/b/notes.pdf".into()), PathBuf::from("/a/b/notes.pdf"));
        assert_eq!(with_pdf_extension("/a/b/notes.PDF".into()), PathBuf::from("/a/b/notes.PDF"));
    }

    #[test]
    fn an_answer_is_taken_once_and_in_order() {
        let dialogs = Dialogs::default();
        assert_eq!(dialogs.take(), None);
        dialogs.answered.lock().unwrap().push_back(Picked { kind: Pick::Save, id: None });
        dialogs.answered.lock().unwrap().push_back(Picked { kind: Pick::Open, id: Some(DocId::Path("/x.pdf".into())) });
        assert_eq!(dialogs.take().map(|p| p.kind), Some(Pick::Save));
        assert_eq!(dialogs.take().and_then(|p| p.id), Some(DocId::Path("/x.pdf".into())));
        assert_eq!(dialogs.take(), None);
    }
}
