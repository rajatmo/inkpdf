//! The pictures of a document's pages: the pool that renders their tiles behind the frame, what it is asked for, and how it draws them (as the
//! file has them, or on a dark paper). It knows the file it renders from and nothing of the document's pages, which can be edited, moved or
//! copied without it: pictures are asked for and kept by the page of the file (see [`crate::page_table`]).

use std::sync::Arc;

use crate::pdf::{PageLook, PdfBackend, RenderPool, RenderRequest, Rendered};
use crate::tiles::{self, Key};

pub struct Renderer {
    engine: &'static dyn PdfBackend,
    pool: Box<dyn RenderPool>,
    queued: Vec<RenderRequest>,
    /// The size of each page of the file, in points (what its pictures are made for).
    sizes: Vec<[f32; 2]>,
    /// What it draws from: the file without the ink this program keeps itself.
    bytes: Arc<Vec<u8>>,
    look: PageLook,
}

impl Renderer {
    pub fn new(engine: &'static dyn PdfBackend, bytes: Arc<Vec<u8>>, sizes: Vec<[f32; 2]>) -> Self {
        let pool = engine.render_pool(bytes.clone(), render_threads(), PageLook::Plain);
        Self { engine, pool, queued: Vec::new(), sizes, bytes, look: PageLook::Plain }
    }

    /// Whether it has a page for `key` (whose page is one of the file's).
    pub fn renders(&self, key: Key) -> bool {
        (key.page() as usize) < self.sizes.len()
    }

    /// Replace the work the pool is doing with exactly `keys` (pages of the file), most urgent first. Unchanged requests keep running.
    pub fn set_queue(&mut self, keys: &[Key]) {
        let requests: Vec<RenderRequest> = keys
            .iter()
            .filter_map(|&k| {
                let size = self.sizes.get(k.page() as usize)?;
                Some(RenderRequest { page: k.page() as usize, ..tiles::request(k, *size) })
            })
            .collect();
        if requests != self.queued {
            self.pool.set_queue(requests.clone());
            self.queued = requests;
        }
    }

    /// The file as it renders it, which is also what the text is read from.
    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.bytes.clone()
    }

    /// How the pages are drawn now.
    #[cfg(test)]
    pub fn look(&self) -> PageLook {
        self.look
    }

    /// Draw the pages in another look: a new pool takes over (what the old one had under way is dropped with it) and everything must be asked
    /// for again. `false` if it already is that look.
    pub fn set_look(&mut self, look: PageLook) -> bool {
        if self.look == look {
            return false;
        }
        self.pool = self.engine.render_pool(self.bytes.clone(), render_threads(), look);
        self.queued.clear();
        self.look = look;
        true
    }

    pub fn try_recv(&self) -> Option<Rendered> {
        self.pool.try_recv()
    }
}

/// Worker threads for rendering: leave two logical processors free for the rest of the system.
fn render_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).saturating_sub(2).clamp(2, 8)
}
