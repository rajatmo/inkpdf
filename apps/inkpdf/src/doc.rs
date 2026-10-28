//! An open PDF: its page layout, the pool that renders its tiles, and saving.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use pdfcraft_cos::{SaveOptions, write_full};
use pdfcraft_render::{RenderConfig, RenderPool, RenderRequest, RenderedPage, inspect};

use crate::coords::PageGeom;
use crate::ink::{InkStore, PageStroke};
use crate::ink_io;
use crate::links::Link;
use crate::tiles::{self, Key};
use crate::view::Layout;

/// The page size a blank document falls back to when the display is not known, in points (A4).
pub const BLANK_PAGE: [f32; 2] = [595.0, 842.0];
/// Smallest and largest side of a page, in points.
const PAGE_SIDE: [f32; 2] = [72.0, 14_400.0];
/// What a document with no file yet is called.
pub const UNTITLED: &str = "Untitled";

/// Where one page of the document comes from.
#[derive(Clone, Copy, Debug)]
struct Slot {
    /// Which page of the renderer's file draws it; `None` for a page that was added, which is blank
    /// and never rendered.
    render: Option<usize>,
    /// Whether the file as last saved has it.
    saved: bool,
}

pub struct Doc {
    /// Where it is saved; `None` for a new document that has not been saved yet.
    pub path: Option<PathBuf>,
    pub name: String,
    /// Displayed size of each page in points.
    pub sizes: Vec<[f32; 2]>,
    pub geoms: Vec<PageGeom>,
    pub layout: Layout,
    /// Links on the pages, in document space.
    pub links: Vec<Link>,
    pub open_millis: u128,
    /// The file as last read or saved: saving appends to it.
    base: Arc<Vec<u8>>,
    /// Each page, in order: where it renders from and whether it is in `base`. Pages can be added
    /// anywhere, so these are kept for every page.
    slots: Vec<Slot>,
    /// What the renderer was given: the file without our ink annotations.
    render_bytes: Arc<Vec<u8>>,
    pool: RenderPool,
    queued: Vec<RenderRequest>,
}

impl Doc {
    /// Open a PDF. Also returns the ink this app saved in it earlier.
    pub fn open(path: &Path) -> Result<(Doc, Vec<PageStroke>), String> {
        let bytes = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        Self::from_bytes(Arc::new(bytes), Some(path.to_path_buf()), name)
    }

    /// A new document of one blank page `size` points across, with no file yet: where you can just
    /// start writing.
    pub fn blank(size: [f32; 2]) -> Result<Doc, String> {
        let size = size.map(|s| if s.is_finite() { s.clamp(PAGE_SIDE[0], PAGE_SIDE[1]) } else { BLANK_PAGE[0] });
        let pdf = pdfcraft_create::blank(f64::from(size[0]), f64::from(size[1]), 1).map_err(|e| e.to_string())?;
        let bytes = write_full(&pdf, &SaveOptions::default()).map_err(|e| e.to_string())?;
        Self::from_bytes(Arc::new(bytes), None, UNTITLED.into()).map(|(doc, _)| doc)
    }

    fn from_bytes(bytes: Arc<Vec<u8>>, path: Option<PathBuf>, name: String) -> Result<(Doc, Vec<PageStroke>), String> {
        let started = Instant::now();
        let info = inspect(bytes.clone(), None).map_err(|e| e.to_string())?;
        if info.pages.is_empty() {
            return Err("the document has no pages".into());
        }
        let pages = info.pages.iter().take(tiles::MAX_PAGES);
        let sizes: Vec<[f32; 2]> = pages.clone().map(|p| [p.width, p.height]).collect();
        let geoms: Vec<PageGeom> = pages.map(PageGeom::from_info).collect();
        let layout = Layout::new(&sizes);
        // A file this app inked earlier shows that ink from the ink layer, not the page raster.
        let (strokes, links, render_bytes) = match ink_io::load(&bytes, &geoms, &layout) {
            Ok(l) => (l.strokes, l.links, l.render_bytes),
            Err(e) => {
                eprintln!("could not read earlier ink: {e}");
                (Vec::new(), Vec::new(), bytes.clone())
            }
        };
        let pool = RenderPool::new(render_bytes.clone(), render_threads(), RenderConfig::default());
        let slots = (0..sizes.len()).map(|i| Slot { render: Some(i), saved: true }).collect();
        let doc = Doc {
            path,
            name,
            sizes,
            geoms,
            layout,
            links,
            open_millis: started.elapsed().as_millis(),
            base: bytes,
            slots,
            render_bytes,
            pool,
            queued: Vec::new(),
        };
        Ok((doc, strokes))
    }

    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    pub fn page_size(&self, page: u32) -> [f32; 2] {
        self.sizes.get(page as usize).copied().unwrap_or(BLANK_PAGE)
    }

    /// Add a blank page after the last one, the same size as it (see [`Doc::insert_blank_page`]).
    #[cfg(test)]
    pub fn add_blank_page(&mut self) -> Option<usize> {
        let last = self.sizes.len().saturating_sub(1);
        self.insert_blank_page(self.sizes.len(), last)
    }

    /// Insert a blank page so that it becomes page `at` (0 to the page count), the same size as page
    /// `like`. Returns its index, or `None` when the document cannot have more pages. The pages
    /// from `at` on move down one, and so do their links; it is written when the document is saved.
    pub fn insert_blank_page(&mut self, at: usize, like: usize) -> Option<usize> {
        if self.sizes.len() >= tiles::MAX_PAGES || at > self.sizes.len() {
            return None;
        }
        let size = self.sizes.get(like).or(self.sizes.last()).copied().unwrap_or(BLANK_PAGE);
        let old_count = self.sizes.len();
        self.sizes.insert(at, size);
        self.geoms.insert(at, PageGeom { width: size[0], height: size[1], crop: [0.0, 0.0, size[0], size[1]], rotation: 0 });
        self.slots.insert(at, Slot { render: None, saved: false });
        let old = std::mem::replace(&mut self.layout, Layout::new(&self.sizes));
        if at < old_count {
            self.shift_links(&old, at);
            // Every page from here on has a new number: what the renderer was doing is out of date.
            self.pool = RenderPool::new(self.render_bytes.clone(), render_threads(), RenderConfig::default());
            self.queued.clear();
        }
        Some(at)
    }

    /// Carry the links along with the pages that moved when a page was inserted at `at`: each link
    /// keeps its place on its page, and a link to a page from `at` on points to its new number.
    fn shift_links(&mut self, old: &Layout, at: usize) {
        let renumber = |p: usize| if p >= at { p + 1 } else { p };
        for link in &mut self.links {
            let from = old.page_at((link.rect[1] + link.rect[3]) / 2.0);
            if let (Some(was), Some(now)) = (old.page(from), self.layout.page(renumber(from))) {
                let (dx, dy) = (now.x - was.x, now.y - was.y);
                link.rect = [link.rect[0] + dx, link.rect[1] + dy, link.rect[2] + dx, link.rect[3] + dy];
            }
            if let crate::links::Target::Page(p) = &mut link.target {
                *p = renumber(*p);
            }
        }
    }

    /// Whether the renderer has something to draw for `key`: not for a page that was added.
    pub fn renders(&self, key: Key) -> bool {
        self.slots.get(key.page() as usize).is_some_and(|s| s.render.is_some())
    }

    /// Replace the work the pool is doing with exactly `keys`, most urgent first. Unchanged
    /// requests keep running. Keys for pages that are blank are ignored.
    pub fn set_queue(&mut self, keys: &[Key]) {
        let requests: Vec<RenderRequest> = keys
            .iter()
            .filter_map(|&k| {
                // The request is tagged with the page's place in the document, and asks the
                // renderer for the page that draws it.
                let render = self.slots.get(k.page() as usize)?.render?;
                Some(RenderRequest { page: render, ..tiles::request(k, self.page_size(k.page())) })
            })
            .collect();
        if requests != self.queued {
            self.pool.set_queue(requests.clone());
            self.queued = requests;
        }
    }

    pub fn try_recv(&self) -> Option<RenderedPage> {
        self.pool.try_recv()
    }

    /// Write the ink (and any added pages) into the file. `Err` for a document with no file yet:
    /// use [`Doc::save_as`].
    pub fn save(&mut self, store: &InkStore) -> Result<(), String> {
        let path = self.path.clone().ok_or("this document has not been saved anywhere yet")?;
        self.write_to(&path, store)
    }

    /// Save to `path`, which becomes where the document lives.
    pub fn save_as(&mut self, path: &Path, store: &InkStore) -> Result<(), String> {
        self.write_to(path, store)?;
        self.name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        self.path = Some(path.to_path_buf());
        Ok(())
    }

    /// The new file is written beside the old one and swapped in, so a failure part-way leaves the
    /// original as it was.
    fn write_to(&mut self, path: &Path, store: &InkStore) -> Result<(), String> {
        let inserts: Vec<(usize, [f32; 2])> = self.slots.iter().enumerate().filter(|(_, s)| !s.saved).map(|(i, _)| (i, self.sizes[i])).collect();
        let bytes = ink_io::save(&self.base, store, &self.geoms, &self.layout, store.dirty_pages(), &inserts)?;
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let file = path.file_name().map_or_else(|| "document.pdf".into(), |n| n.to_string_lossy().into_owned());
        let tmp = dir.join(format!(".{file}.inkpdf-saving"));
        fs::write(&tmp, &bytes).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("could not replace {}: {e}", path.display()));
        }
        self.base = Arc::new(bytes);
        self.slots.iter_mut().for_each(|s| s.saved = true);
        Ok(())
    }
}

/// Worker threads for rendering: leave two logical processors free for the rest of the system.
fn render_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).saturating_sub(2).clamp(2, 8)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::Style;
    use crate::testutil::sample_pdf;

    const RED: Style = Style { rgba: [255, 0, 0, 255], width: 2.0 };

    fn temp_pdf(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkpdf-test-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("doc.pdf");
        fs::write(&path, sample_pdf()).expect("write the sample");
        path
    }

    #[test]
    fn saving_writes_the_file_in_place_and_it_reopens_with_the_ink() {
        let path = temp_pdf("save");
        let original = fs::read(&path).expect("read");
        let (mut doc, strokes) = Doc::open(&path).expect("open");
        assert!(strokes.is_empty());

        let mut store = InkStore::new();
        let o = doc.layout.page(0).expect("page");
        store.add(0, RED, vec![[o.x + 100.0, o.y + 100.0], [o.x + 200.0, o.y + 150.0], [o.x + 300.0, o.y + 100.0]]);
        doc.save(&store).expect("save");
        store.mark_clean();

        let saved = fs::read(&path).expect("read the saved file");
        assert_eq!(&saved[..original.len()], &original[..], "the original bytes are kept at the start");
        let leftovers: Vec<_> = fs::read_dir(path.parent().expect("dir"))
            .expect("dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["doc.pdf".to_string()], "no temporary file is left beside it");

        let (_, reloaded) = Doc::open(&path).expect("reopen");
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].1, RED);

        // A second save, after changing the ink, replaces the first one's annotations.
        let ids: Vec<u64> = store.strokes().iter().map(|s| s.id).collect();
        store.remove(&ids);
        doc.save(&store).expect("second save");
        let (_, after) = Doc::open(&path).expect("reopen again");
        assert!(after.is_empty(), "erased ink stays erased");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false, reason = "restoring a read-only flag on Windows")]
    fn a_save_that_cannot_write_leaves_the_file_as_it_was() {
        let path = temp_pdf("readonly");
        let original = fs::read(&path).expect("read");
        let (mut doc, _) = Doc::open(&path).expect("open");
        let mut perms = fs::metadata(&path).expect("meta").permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).expect("make it read-only");
        let mut store = InkStore::new();
        store.add(0, RED, vec![[50.0, 50.0], [80.0, 80.0]]);
        // Windows will not replace a read-only file: the save must fail cleanly.
        let result = doc.save(&store);
        let mut perms = fs::metadata(&path).expect("meta").permissions();
        perms.set_readonly(false);
        fs::set_permissions(&path, perms).expect("restore");
        assert!(result.is_err(), "replacing a read-only file must be refused");
        assert_eq!(fs::read(&path).expect("read"), original, "the original is untouched");
        let leftovers = fs::read_dir(path.parent().expect("dir")).expect("dir").count();
        assert_eq!(leftovers, 1, "and the temporary file is cleaned up");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn opening_something_that_is_not_a_pdf_is_an_error_not_a_crash() {
        let path = temp_pdf("junk");
        fs::write(&path, b"this is not a pdf").expect("write");
        assert!(Doc::open(&path).is_err());
        assert!(Doc::open(&path.with_file_name("missing.pdf")).is_err());
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkpdf-doc-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_blank_document_is_one_empty_page_with_no_file() {
        let doc = Doc::blank(BLANK_PAGE).expect("blank");
        assert_eq!(doc.page_count(), 1);
        assert_eq!(doc.page_size(0), [595.0, 842.0], "A4");
        assert_eq!((doc.path.as_ref(), doc.name.as_str()), (None, UNTITLED));
        assert!(doc.links.is_empty());
    }

    #[test]
    fn a_blank_document_cannot_be_saved_in_place_but_can_be_saved_as() {
        let dir = temp_dir("saveas");
        let mut doc = Doc::blank(BLANK_PAGE).expect("blank");
        let mut store = InkStore::new();
        let page = doc.layout.page(0).expect("page");
        store.add(0, RED, vec![[page.x + 50.0, page.y + 50.0], [page.x + 120.0, page.y + 90.0]]);
        assert!(doc.save(&store).is_err(), "there is nowhere to write it yet");

        let path = dir.join("notes.pdf");
        doc.save_as(&path, &store).expect("save as");
        assert_eq!((doc.path.as_deref(), doc.name.as_str()), (Some(path.as_path()), "notes.pdf"));
        store.mark_clean();

        let (reopened, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!((reopened.page_count(), strokes.len()), (1, 1));
        // After that, saving works in place.
        store.add(0, RED, vec![[page.x + 10.0, page.y + 10.0], [page.x + 30.0, page.y + 10.0]]);
        doc.save(&store).expect("save in place");
        assert_eq!(Doc::open(&path).expect("again").1.len(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn added_pages_are_saved_with_their_ink_and_the_original_bytes_are_kept() {
        let dir = temp_dir("addpage");
        let path = dir.join("doc.pdf");
        fs::write(&path, sample_pdf()).expect("write");
        let original = fs::read(&path).expect("read");
        let (mut doc, _) = Doc::open(&path).expect("open");
        assert_eq!(doc.add_blank_page(), Some(1));
        assert_eq!(doc.add_blank_page(), Some(2));
        assert_eq!(doc.page_count(), 3);
        assert_eq!(doc.page_size(2), doc.page_size(0), "an added page is as big as the last one");

        let mut store = InkStore::new();
        store.mark_dirty(1);
        store.mark_dirty(2);
        let page = doc.layout.page(2).expect("page");
        store.add(2, RED, vec![[page.x + 40.0, page.y + 40.0], [page.x + 140.0, page.y + 60.0]]);
        doc.save(&store).expect("save");

        let saved = fs::read(&path).expect("read back");
        assert_eq!(&saved[..original.len()], &original[..], "the file only grew");
        let (reopened, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!(reopened.page_count(), 3);
        assert_eq!((strokes.len(), strokes[0].0), (1, 2), "the stroke is on the third page");
        // Saving again without more pages adds none.
        doc.save(&store).expect("save again");
        assert_eq!(Doc::open(&path).expect("again").0.page_count(), 3);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_blank_page_added_to_a_blank_document_survives_save_as() {
        let dir = temp_dir("blankplus");
        let mut doc = Doc::blank(BLANK_PAGE).expect("blank");
        doc.add_blank_page().expect("add");
        let mut store = InkStore::new();
        store.mark_dirty(1);
        let path = dir.join("two.pdf");
        doc.save_as(&path, &store).expect("save as");
        assert_eq!(Doc::open(&path).expect("reopen").0.page_count(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn added_pages_are_never_sent_to_the_renderer() {
        let path = temp_pdf("render");
        let (mut doc, _) = Doc::open(&path).expect("open");
        doc.add_blank_page().expect("add");
        assert!(doc.renders(Key::Preview { page: 0 }));
        assert!(!doc.renders(Key::Preview { page: 1 }), "nothing to draw on a blank page");
        // Asking for it anyway queues nothing and does not fail.
        doc.set_queue(&[Key::Preview { page: 1 }]);
        assert!(doc.try_recv().is_none());
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn links_load_with_the_document() {
        let dir = temp_dir("links");
        let path = dir.join("links.pdf");
        fs::write(&path, crate::testutil::pdf_with_links()).expect("write");
        let (doc, _) = Doc::open(&path).expect("open");
        assert_eq!(doc.links.len(), 4);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn a_blank_document_is_as_big_as_the_display_it_is_given_and_odd_sizes_are_made_sane() {
        let doc = Doc::blank([1280.0, 720.0]).expect("blank");
        assert_eq!(doc.page_size(0), [1280.0, 720.0]);
        let odd = Doc::blank([1.0, f32::NAN]).expect("blank");
        assert_eq!(odd.page_size(0), [PAGE_SIDE[0], BLANK_PAGE[0]], "too small is raised, not a number is replaced");
        assert_eq!(Doc::blank([1.0e9, 1.0e9]).expect("blank").page_size(0), [PAGE_SIDE[1], PAGE_SIDE[1]]);
    }

    #[test]
    fn a_page_inserted_in_the_middle_is_saved_there_and_the_ink_and_links_follow_their_pages() {
        let dir = temp_dir("insert");
        let path = dir.join("links.pdf");
        fs::write(&path, crate::testutil::pdf_with_links()).expect("write");
        let (mut doc, _) = Doc::open(&path).expect("open");
        let n = doc.page_count();
        assert!(n >= 2, "the sample has more than one page");
        let before: Vec<_> = doc.links.iter().map(|l| l.target.clone()).collect();
        let last = (n - 1) as u32;
        let mut store = InkStore::new();
        let at = doc.layout.page(n - 1).expect("page");
        store.add(last, RED, vec![[at.x + 30.0, at.y + 40.0], [at.x + 90.0, at.y + 70.0]]);
        store.mark_clean();
        store.add(last, RED, vec![[at.x + 30.0, at.y + 100.0], [at.x + 90.0, at.y + 130.0]]);

        // Insert a page after the first, the size of the first.
        let old_y = doc.layout.page(1).expect("page").y;
        assert_eq!(doc.insert_blank_page(1, 0), Some(1));
        assert_eq!(doc.page_count(), n + 1);
        assert_eq!(doc.page_size(1), doc.page_size(0));
        store.insert_page_at(1, doc.layout.page(2).expect("page").y - old_y);
        store.mark_dirty(1);
        // Links to a page from the new one on point at its new number.
        for (now, was) in doc.links.iter().zip(&before) {
            if let crate::links::Target::Page(p) = was {
                assert_eq!(now.target, crate::links::Target::Page(if *p >= 1 { p + 1 } else { *p }));
            }
        }
        doc.save(&store).expect("save");

        let (reopened, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!(reopened.page_count(), n + 1, "the page is in the file");
        assert_eq!(reopened.page_size(1), doc.page_size(1));
        assert_eq!(strokes.len(), 2, "both strokes came back");
        assert!(strokes.iter().all(|s| s.0 == last + 1), "on the page they were on, which is one further on");
        // The links as read from the new file are where the shifted ones were.
        assert_eq!(reopened.links.len(), doc.links.len());
        for (a, b) in reopened.links.iter().zip(&doc.links) {
            assert_eq!(a.target, b.target);
            assert!(a.rect.iter().zip(&b.rect).all(|(x, y)| (x - y).abs() < 0.5), "{:?} vs {:?}", a.rect, b.rect);
        }
        // Saving again changes nothing about the pages.
        doc.save(&store).expect("save again");
        assert_eq!(Doc::open(&path).expect("again").0.page_count(), n + 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ink_saved_before_a_page_was_inserted_is_replaced_not_duplicated_when_its_page_moves() {
        let dir = temp_dir("insert-resave");
        let path = dir.join("doc.pdf");
        fs::write(&path, crate::testutil::pdf_with_links()).expect("write");
        let (mut doc, _) = Doc::open(&path).expect("open");
        let last = (doc.page_count() - 1) as u32;
        let mut store = InkStore::new();
        let at = doc.layout.page(last as usize).expect("page");
        store.add(last, RED, vec![[at.x + 30.0, at.y + 40.0], [at.x + 90.0, at.y + 70.0]]);
        doc.save(&store).expect("first save");
        store.mark_clean();
        // Now a page in front of it, and a second stroke on the page that moved.
        let old_y = doc.layout.page(1).expect("page").y;
        doc.insert_blank_page(1, 0).expect("insert");
        store.insert_page_at(1, doc.layout.page(2).expect("page").y - old_y);
        store.mark_dirty(1);
        let moved = doc.layout.page(last as usize + 1).expect("page");
        store.add(last + 1, RED, vec![[moved.x + 30.0, moved.y + 200.0], [moved.x + 90.0, moved.y + 230.0]]);
        doc.save(&store).expect("second save");
        let (_, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!(strokes.len(), 2, "the old stroke and the new one, each once");
        assert!(strokes.iter().all(|s| s.0 == last + 1));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn after_an_insertion_pages_render_from_the_pages_of_the_file_they_came_from() {
        let path = temp_pdf("render-map");
        let (mut doc, _) = Doc::open(&path).expect("open");
        assert_eq!(doc.insert_blank_page(0, 0), Some(0), "in front of the only page");
        assert!(!doc.renders(Key::Preview { page: 0 }), "the new page is blank");
        assert!(doc.renders(Key::Preview { page: 1 }), "the page that was first is second now");
        doc.set_queue(&[Key::Preview { page: 0 }, Key::Preview { page: 1 }]);
        let started = Instant::now();
        let rendered = loop {
            if let Some(page) = doc.try_recv() {
                break page;
            }
            assert!(started.elapsed().as_secs() < 20, "the page was never rendered");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(Key::from_tag(rendered.request.tag), Key::Preview { page: 1 }, "it is tagged with where the page is now");
        assert_eq!(rendered.request.page, 0, "and was drawn from the file's first page");
        assert!(rendered.error.is_none());
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }
}
