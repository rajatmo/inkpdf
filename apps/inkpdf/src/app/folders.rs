//! Listing a folder for the Save As dialog: read off the main thread (a network drive can take seconds to answer, and a frame must
//! not wait for it) and handed back through a channel the shell empties as it serves the editor's requests.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use winit::window::Window;

use crate::browse::Entry;
use crate::store::{DocId, FileStore};

/// What was found in `dir`, or why it could not be read.
pub(super) struct Listed {
    pub dir: PathBuf,
    pub result: Result<Vec<Entry>, String>,
}

/// Read `dir` on a thread of its own; the answer goes through `tx` and the window is woken to take it.
pub(super) fn spawn(dir: PathBuf, files: Arc<dyn FileStore>, window: Arc<Window>, tx: Sender<Listed>) {
    let spawned = std::thread::Builder::new().name("inkpdf-list-folder".into()).spawn(move || {
        let result = files.list(&DocId::Path(dir.clone()));
        if tx.send(Listed { dir, result }).is_ok() {
            window.request_redraw();
        }
    });
    if let Err(e) = spawned {
        eprintln!("could not start a thread to read a folder: {e}");
    }
}
