//! An open PDF: its pages (which can be edited before the file is written), the renderer that draws them, and saving.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::coords::PageGeom;
use crate::form::Form;
use crate::ink::{InkStore, PageStroke};
use crate::ink_io::{self, Extras};
use crate::links::Link;
use crate::outline::OutlineItem;
pub use crate::page_table::BLANK_PAGE;
use crate::page_table::PageTable;
use crate::pages::PageEdit;
#[cfg(test)]
use crate::path_store::PathStore;
use crate::pdf::{self, PageLook, PdfBackend, Rendered};
use crate::renderer::Renderer;
use crate::store::{DocId, FileStore};
use crate::tiles::{self, Key};
use crate::view::Layout;

/// Smallest and largest side of a page, in points.
const PAGE_SIDE: [f32; 2] = [72.0, 14_400.0];
/// What a document with no file yet is called.
pub const UNTITLED: &str = "Untitled";

/// Why a document could not be opened or saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocError {
    /// The file could not be read.
    Read(String),
    /// The PDF engine could not make sense of it, or could not make what was asked.
    Engine(String),
    /// The file has no pages.
    NoPages,
    /// A save in place of a document that has no file yet.
    NotSaved,
    /// The file could not be written.
    Write(String),
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocError::Read(e) | DocError::Engine(e) | DocError::Write(e) => f.write_str(e),
            DocError::NoPages => f.write_str("the document has no pages"),
            DocError::NotSaved => f.write_str("this document has not been saved anywhere yet"),
        }
    }
}

impl From<String> for DocError {
    fn from(e: String) -> Self {
        DocError::Engine(e)
    }
}

/// What a file holds besides its pages, which the editor takes when the document is opened and keeps from then on: the ink this app saved in
/// it earlier, the links, the form (the boxes of its fields are among the links), and the table of contents.
#[derive(Default)]
pub struct Contents {
    pub strokes: Vec<PageStroke>,
    pub links: Vec<Link>,
    pub form: Form,
    pub outline: Vec<OutlineItem>,
}

pub struct Doc {
    /// Which file it is saved in; `None` for a new document that has not been saved yet.
    pub id: Option<DocId>,
    pub name: String,
    pub open_millis: u128,
    /// The PDF engine the document was opened with (the one the rest of it is made with too).
    engine: &'static dyn PdfBackend,
    /// The file as last read or saved: saving appends to it.
    base: Arc<Vec<u8>>,
    /// Each page: its size, and where it renders from and which page of `base` it is.
    pages: PageTable,
    renderer: Renderer,
}

impl Doc {
    /// Open a PDF from the file system. Also returns what it holds. (The program opens through its platform's store; the tests, which use
    /// the file system, through this.)
    #[cfg(test)]
    pub fn open(path: &Path) -> Result<(Doc, Vec<PageStroke>), DocError> {
        let (doc, contents) = Self::open_contents(path)?;
        Ok((doc, contents.strokes))
    }

    /// [`Doc::open`], with all that the file holds.
    #[cfg(test)]
    pub fn open_contents(path: &Path) -> Result<(Doc, Contents), DocError> {
        Self::open_from(&PathStore, &DocId::Path(path.to_path_buf()))
    }

    /// Open the PDF `id` names in `files`, with the program's PDF engine. Also returns what the file holds.
    pub fn open_from(files: &dyn FileStore, id: &DocId) -> Result<(Doc, Contents), DocError> {
        Self::open_with(pdf::native(), files, id)
    }

    /// [`Doc::open_from`] with another PDF engine.
    pub fn open_with(engine: &'static dyn PdfBackend, files: &dyn FileStore, id: &DocId) -> Result<(Doc, Contents), DocError> {
        let bytes = files.read(id).map_err(DocError::Read)?;
        Self::from_bytes(engine, Arc::new(bytes), Some(id.clone()), files.display_name(id))
    }

    /// The file's path, if it is kept by path and has been saved.
    pub fn path(&self) -> Option<&Path> {
        self.id.as_ref().and_then(DocId::as_path)
    }

    /// A new document of one blank page `size` points across, with no file yet: where you can just
    /// start writing.
    pub fn blank(size: [f32; 2]) -> Result<Doc, DocError> {
        let size = size.map(|s| if s.is_finite() { s.clamp(PAGE_SIDE[0], PAGE_SIDE[1]) } else { BLANK_PAGE[0] });
        let engine = pdf::native();
        let bytes = engine.blank(size)?;
        Self::from_bytes(engine, Arc::new(bytes), None, UNTITLED.into()).map(|(doc, _)| doc)
    }

    fn from_bytes(engine: &'static dyn PdfBackend, bytes: Arc<Vec<u8>>, id: Option<DocId>, name: String) -> Result<(Doc, Contents), DocError> {
        let started = Instant::now();
        let all = engine.inspect(&bytes)?;
        if all.is_empty() {
            return Err(DocError::NoPages);
        }
        let geoms: Vec<PageGeom> = all.into_iter().take(tiles::MAX_PAGES).collect();
        let sizes: Vec<[f32; 2]> = geoms.iter().map(|p| [p.width, p.height]).collect();
        let layout = Layout::new(&sizes);
        // A file this app inked earlier shows that ink from the ink layer, not the page raster.
        let (contents, render_bytes) = match ink_io::load_with(engine, &bytes, &geoms, &layout) {
            Ok(l) => (Contents { strokes: l.strokes, links: l.links, form: l.form, outline: l.outline }, l.render_bytes),
            Err(e) => {
                eprintln!("could not read earlier ink: {e}");
                (Contents::default(), bytes.clone())
            }
        };
        // (In a damaged file the outline and the links can name pages the renderer does not show.)
        let Contents { strokes, mut links, form, mut outline } = contents;
        crate::outline::limit_to(&mut outline, sizes.len());
        crate::links::limit_to(&mut links, sizes.len());
        let renderer = Renderer::new(engine, render_bytes, sizes.clone());
        let doc =
            Doc { id, name, open_millis: started.elapsed().as_millis(), engine, base: bytes, pages: PageTable::of_file(sizes, geoms), renderer };
        Ok((doc, Contents { strokes, links, form, outline }))
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    #[cfg(test)]
    pub fn page_size(&self, page: u32) -> [f32; 2] {
        self.pages.sizes().get(page as usize).copied().unwrap_or(BLANK_PAGE)
    }

    /// Where each page is laid out, as the editor lays it out.
    pub fn layout(&self) -> Layout {
        Layout::new(self.pages.sizes())
    }

    /// Add a blank page after the last one, the same size as it.
    #[cfg(test)]
    pub fn add_blank_page(&mut self) -> Option<usize> {
        let size = self.pages.sizes().last().copied().unwrap_or(BLANK_PAGE);
        let at = self.pages.len();
        self.apply(&PageEdit::Insert { at, size }).then_some(at)
    }

    /// The pages kept for these deletions can go: the deletions can no longer be undone.
    pub fn forget_graves(&mut self, graves: &[u64]) {
        self.pages.forget_graves(graves);
    }

    /// Do an edit of the pages (see [`PageEdit`]): the page table follows, and the file is changed when the document is
    /// saved. `false`, and nothing done, if the edit cannot be done to this document, or puts back pages that are no
    /// longer kept. The editor does the same edit to everything it keeps by page.
    pub fn apply(&mut self, edit: &PageEdit) -> bool {
        self.pages.apply(edit)
    }

    /// The page of the file the renderer has that draws page `page` of the document, if there is one (not for a page that
    /// was added). A picture is kept under this number, not the page's own: it stays the right one when pages are moved,
    /// deleted, copied or brought back, and a copy shares its original's.
    pub fn render_page(&self, page: usize) -> Option<u32> {
        self.pages.render_page(page)
    }

    /// Whether the renderer has a page for `key` (whose page is one of the file's, see [`Doc::render_page`]).
    pub fn renders(&self, key: Key) -> bool {
        self.renderer.renders(key)
    }

    /// Replace the work the renderer is doing with exactly `keys` (pages of the file, see [`Doc::render_page`]), most urgent
    /// first. Unchanged requests keep running, through any change of the pages.
    pub fn set_queue(&mut self, keys: &[Key]) {
        self.renderer.set_queue(keys);
    }

    /// The file the text is read from: as the renderer has it, without the ink and the text boxes this program keeps itself (those are the
    /// editor's own to search, so they are not found twice).
    pub fn text_bytes(&self) -> Arc<Vec<u8>> {
        self.renderer.bytes()
    }

    /// The page of the file that draws each page of the document (`None` for a page that was added).
    pub fn file_pages(&self) -> Vec<Option<u32>> {
        (0..self.pages.len()).map(|p| self.pages.render_page(p)).collect()
    }

    /// How the pages are drawn now.
    #[cfg(test)]
    pub fn look(&self) -> PageLook {
        self.renderer.look()
    }

    /// Draw the pages in another look (see [`Renderer::set_look`]). `false` if it already is that look.
    pub fn set_look(&mut self, look: PageLook) -> bool {
        self.renderer.set_look(look)
    }

    pub fn try_recv(&self) -> Option<Rendered> {
        self.renderer.try_recv()
    }

    /// Write the ink (and any added pages, and `extras`) into the file, in `files`. [`DocError::NotSaved`] for a document with no file yet: use
    /// [`Doc::save_as_to`].
    pub fn save_to(&mut self, files: &dyn FileStore, store: &InkStore, extras: &Extras<'_>) -> Result<(), DocError> {
        let id = self.id.clone().ok_or(DocError::NotSaved)?;
        self.write_to(files, &id, store, extras)
    }

    /// [`Doc::save_to`] on the file system, with nothing extra.
    #[cfg(test)]
    pub fn save(&mut self, store: &InkStore) -> Result<(), DocError> {
        self.save_to(&PathStore, store, &Extras::default())
    }

    /// Save to `path` on the file system, which becomes where the document lives.
    #[cfg(test)]
    pub fn save_as(&mut self, path: &Path, store: &InkStore) -> Result<(), DocError> {
        self.save_as_to(&PathStore, &DocId::Path(path.to_path_buf()), store, &Extras::default())
    }

    /// Save as the file `id` names in `files`, which becomes where the document lives.
    pub fn save_as_to(&mut self, files: &dyn FileStore, id: &DocId, store: &InkStore, extras: &Extras<'_>) -> Result<(), DocError> {
        self.write_to(files, id, store, extras)?;
        self.name = files.display_name(id);
        self.id = Some(id.clone());
        Ok(())
    }

    /// The new file is written whole or not at all (see [`FileStore::write_atomic`]), so a failure part-way leaves the original as
    /// it was.
    fn write_to(&mut self, files: &dyn FileStore, id: &DocId, store: &InkStore, extras: &Extras<'_>) -> Result<(), DocError> {
        let bytes = ink_io::save_with(
            self.engine,
            &self.base,
            store,
            self.pages.geoms(),
            &self.layout(),
            store.dirty_pages(),
            &self.pages.doc_of_file(),
            &self.pages.copies(),
            extras,
        )?;
        files.write_atomic(id, &bytes).map_err(DocError::Write)?;
        self.base = Arc::new(bytes);
        self.pages.written();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::ink::Style;
    use crate::pages::Remap;
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
        let o = doc.layout().page(0).expect("page");
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
    fn a_form_is_read_with_a_box_for_each_widget_and_what_is_put_in_it_is_saved_with_the_file_and_read_back() {
        let dir = temp_dir("form");
        let path = dir.join("form.pdf");
        fs::write(&path, crate::testutil::pdf_with_form()).expect("write");
        let (mut doc, contents) = Doc::open_contents(&path).expect("open");
        let (mut form, boxes) = (contents.form, contents.links);
        let names: Vec<&str> = form.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["name", "agree", "size", "colour"]);
        let places: Vec<_> = boxes
            .iter()
            .filter_map(|l| match l.target {
                crate::links::Target::Field { field, widget } => Some((field, widget, l.rect)),
                _ => None,
            })
            .collect();
        assert_eq!(
            places.iter().map(|&(f, w, _)| (f, w)).collect::<Vec<_>>(),
            [(0, 0), (1, 0), (2, 0), (2, 1), (3, 0)],
            "one box for each widget, in order"
        );
        // The page is 612 x 792 and lies 12 points in; the PDF's y axis points up (the first box is 700 to 720).
        let expected = [12.0 + 100.0, 12.0 + 792.0 - 720.0, 12.0 + 300.0, 12.0 + 792.0 - 700.0];
        assert!(places[0].2.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3), "{:?}", places[0].2);

        form.fields[0].set_text("Ada Lovelace".into());
        form.fields[1].press(0);
        form.fields[2].press(1);
        form.fields[3].press(0);
        let mut store = InkStore::new();
        store.mark_dirty(0);
        let fields = crate::pdf_read::raw_values(form.changes());
        doc.save_to(&PathStore, &store, &Extras { flags: None, fields: &fields }).expect("save");

        let back = Doc::open_contents(&path).expect("reopen").1.form;
        assert_eq!(back.fields[0].shown(), "Ada Lovelace");
        assert!(back.fields[1].is_on(0), "the box is checked");
        assert!(!back.fields[2].is_on(0) && back.fields[2].is_on(1), "the second radio button is on");
        assert_eq!(back.fields[3].shown(), "Red");
        assert!(back.changes().is_empty(), "and what the file has is not a change");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn flags_are_written_with_a_save_and_come_back_apart_from_the_contents_and_a_save_that_names_none_clears_them() {
        let path = temp_pdf("flags");
        let (mut doc, _) = Doc::open(&path).expect("open");
        let mut store = InkStore::new();
        let second = doc.add_blank_page().expect("a second page");
        store.mark_dirty(second as u32);
        let with_flags = |pages: &'static [usize]| Extras { flags: Some(pages), fields: &[] };
        doc.save_to(&PathStore, &store, &with_flags(&[0, 1])).expect("save");
        store.mark_clean();

        let (mut again, contents) = Doc::open_contents(&path).expect("reopen");
        let mut items = contents.outline;
        assert!(!items.is_empty(), "the flags are outline entries in the file, so any reader shows them");
        let flags = crate::outline::take_flags(&mut items);
        assert_eq!(flags.into_iter().collect::<Vec<_>>(), [0, second]);
        assert!(items.is_empty(), "and what is left is the contents the file had, which here is none");

        // A save that says nothing about flags leaves them; one that says there are none takes them away.
        store.mark_dirty(0);
        again.save(&store).expect("save without saying");
        let mut kept = Doc::open_contents(&path).expect("reopen").1.outline;
        assert_eq!(crate::outline::take_flags(&mut kept).len(), 2);
        again.save_to(&PathStore, &store, &with_flags(&[])).expect("save with none");
        let mut left = Doc::open_contents(&path).expect("reopen").1.outline;
        assert!(crate::outline::take_flags(&mut left).is_empty() && left.is_empty());
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
        assert_eq!((doc.path(), doc.name.as_str()), (None, UNTITLED));
    }

    #[test]
    fn a_blank_document_cannot_be_saved_in_place_but_can_be_saved_as() {
        let dir = temp_dir("saveas");
        let mut doc = Doc::blank(BLANK_PAGE).expect("blank");
        let mut store = InkStore::new();
        let page = doc.layout().page(0).expect("page");
        store.add(0, RED, vec![[page.x + 50.0, page.y + 50.0], [page.x + 120.0, page.y + 90.0]]);
        assert!(doc.save(&store).is_err(), "there is nowhere to write it yet");

        let path = dir.join("notes.pdf");
        doc.save_as(&path, &store).expect("save as");
        assert_eq!((doc.path(), doc.name.as_str()), (Some(path.as_path()), "notes.pdf"));
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
        let page = doc.layout().page(2).expect("page");
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
        let (_, contents) = Doc::open_contents(&path).expect("open");
        assert_eq!(contents.links.len(), 4);
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
        let (mut doc, contents) = Doc::open_contents(&path).expect("open");
        let n = doc.page_count();
        assert!(n >= 2, "the sample has more than one page");
        let mut links = contents.links;
        let was = doc.layout();
        let last = (n - 1) as u32;
        let mut store = InkStore::new();
        let at = doc.layout().page(n - 1).expect("page");
        store.add(last, RED, vec![[at.x + 30.0, at.y + 40.0], [at.x + 90.0, at.y + 70.0]]);
        store.mark_clean();
        store.add(last, RED, vec![[at.x + 30.0, at.y + 100.0], [at.x + 90.0, at.y + 130.0]]);

        // Insert a page after the first, the size of the first.
        insert(&mut doc, &mut store, 1, 0);
        assert_eq!(doc.page_count(), n + 1);
        assert_eq!(doc.page_size(1), doc.page_size(0));
        // The editor carries the links the same way: each keeps its place on its page, and links to a page from the new one on
        // point at its new number.
        let edit = PageEdit::Insert { at: 1, size: doc.page_size(0) };
        crate::links::remap(&mut links, &was, &Remap::new(&edit, &was, &doc.layout()).expect("remap"));
        doc.save(&store).expect("save");

        let (reopened, contents) = Doc::open_contents(&path).expect("reopen");
        let strokes = contents.strokes;
        assert_eq!(reopened.page_count(), n + 1, "the page is in the file");
        assert_eq!(reopened.page_size(1), doc.page_size(1));
        assert_eq!(strokes.len(), 2, "both strokes came back");
        assert!(strokes.iter().all(|s| s.0 == last + 1), "on the page they were on, which is one further on");
        // The links as read from the new file are where the carried ones are.
        let from_file = contents.links;
        assert_eq!(from_file.len(), links.len());
        for (a, b) in from_file.iter().zip(&links) {
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
        let at = doc.layout().page(last as usize).expect("page");
        store.add(last, RED, vec![[at.x + 30.0, at.y + 40.0], [at.x + 90.0, at.y + 70.0]]);
        doc.save(&store).expect("first save");
        store.mark_clean();
        // Now a page in front of it, and a second stroke on the page that moved.
        insert(&mut doc, &mut store, 1, 0);
        let moved = doc.layout().page(last as usize + 1).expect("page");
        store.add(last + 1, RED, vec![[moved.x + 30.0, moved.y + 200.0], [moved.x + 90.0, moved.y + 230.0]]);
        doc.save(&store).expect("second save");
        let (_, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!(strokes.len(), 2, "the old stroke and the new one, each once");
        assert!(strokes.iter().all(|s| s.0 == last + 1));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_dark_look_draws_the_pages_dark_and_leaves_what_is_saved_as_it_is() {
        let dir = temp_dir("dark-look");
        let (plain, dark) = (dir.join("plain.pdf"), dir.join("dark.pdf"));
        for (path, look) in [(&plain, pdf::PageLook::Plain), (&dark, pdf::PageLook::Dark)] {
            fs::write(path, sample_pdf()).expect("write");
            let (mut doc, _) = Doc::open(path).expect("open");
            assert_eq!(doc.look(), pdf::PageLook::Plain);
            assert_eq!(doc.set_look(look), look == pdf::PageLook::Dark, "a change is reported, no change is not");
            assert!(!doc.set_look(look));
            doc.set_queue(&[Key::Preview { page: 0 }]);
            let started = Instant::now();
            let page = loop {
                if let Some(page) = doc.try_recv() {
                    break page;
                }
                assert!(started.elapsed().as_secs() < 20, "the page was never rendered");
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let pixels = page.rgba.as_slice();
            let mean = pixels.chunks(4).map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2])).sum::<u64>() / (pixels.len() as u64 / 4) / 3;
            assert_eq!(mean < 100, look == pdf::PageLook::Dark, "the page is drawn {look:?}: its mean is {mean}");
            let mut store = InkStore::new();
            store.mark_dirty(0);
            let at = doc.layout().page(0).expect("page");
            store.add(0, RED, vec![[at.x + 40.0, at.y + 40.0], [at.x + 140.0, at.y + 60.0]]);
            doc.save(&store).expect("save");
        }
        // (The annotation names carry a clock, so the bytes are compared by what they say: the same size, the same ink read back.)
        let (a, b) = (fs::read(&plain).expect("plain"), fs::read(&dark).expect("dark"));
        assert_eq!(a.len(), b.len(), "the look is display only: the saved files are the same size");
        let strokes = |path: &Path| {
            Doc::open(path).expect("reopen").1.into_iter().map(|(page, style, pts, _, _, _, _)| (page, style.rgba, pts.len())).collect::<Vec<_>>()
        };
        assert_eq!(strokes(&plain), strokes(&dark), "with the same ink in them");
        assert_eq!(strokes(&dark).len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_renders_in_flight_survive_any_change_of_the_pages_and_pictures_keep_their_names() {
        let (mut doc, mut store, path) = open_pages("pictures-follow", 4);
        let before: Vec<Option<u32>> = (0..4).map(|p| doc.render_page(p)).collect();
        edit(&mut doc, &mut store, PageEdit::Move { pages: vec![3], to: 0 });
        assert_eq!(
            (0..4).map(|p| doc.render_page(p)).collect::<Vec<_>>(),
            [before[3], before[0], before[1], before[2]],
            "a moved page keeps what draws it"
        );
        edit(&mut doc, &mut store, PageEdit::Delete { pages: vec![1], grave: 4 });
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![0] });
        assert_eq!((0..4).map(|p| doc.render_page(p)).collect::<Vec<_>>(), [before[3], before[3], before[1], before[2]]);
        assert!(undo(&mut doc, &mut store) && undo(&mut doc, &mut store));
        assert_eq!((0..4).map(|p| doc.render_page(p)).collect::<Vec<_>>(), [before[3], before[0], before[1], before[2]]);
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn after_an_insertion_pages_render_from_the_pages_of_the_file_they_came_from() {
        let path = temp_pdf("render-map");
        let (mut doc, _) = Doc::open(&path).expect("open");
        assert!(doc.apply(&PageEdit::Insert { at: 0, size: doc.page_size(0) }), "in front of the only page");
        assert_eq!(doc.render_page(0), None, "the new page is blank");
        assert_eq!(doc.render_page(1), Some(0), "the page that was first is second now, and is drawn from the file's first");
        doc.set_queue(&[Key::Preview { page: 0 }]);
        let started = Instant::now();
        let rendered = loop {
            if let Some(page) = doc.try_recv() {
                break page;
            }
            assert!(started.elapsed().as_secs() < 20, "the page was never rendered");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(
            Key::from_tag(rendered.tag),
            Key::Preview { page: 0 },
            "it is tagged with the page of the file, which no change of the pages alters"
        );
        assert_eq!(rendered.page, 0, "and was drawn from the file's first page");
        assert!(rendered.error.is_none());
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    /// A PDF of `n` pages, page `i` being `600 + 10 * i` points wide, so a page can be told from the others.
    fn pdf_of_pages(n: usize) -> Vec<u8> {
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i)).collect();
        let mut objs = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" "))];
        objs.extend((0..n).map(|i| format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} 800] >>", 600 + 10 * i)));
        crate::testutil::build_pdf(&objs)
    }

    fn open_pages(name: &str, n: usize) -> (Doc, InkStore, PathBuf) {
        let dir = temp_dir(name);
        let path = dir.join("doc.pdf");
        fs::write(&path, pdf_of_pages(n)).expect("write");
        let (doc, _) = Doc::open(&path).expect("open");
        (doc, InkStore::new(), path)
    }

    /// Do an edit of the pages as the editor does: the document and the ink, with its history, both follow.
    fn edit(doc: &mut Doc, store: &mut InkStore, edit: PageEdit) {
        let before = doc.layout();
        let after = before.edited(&edit).expect("layout");
        let undo = edit.inverse(&before).expect("inverse");
        let forward = Remap::new(&edit, &before, &after).expect("remap");
        let backward = Remap::new(&undo, &after, &before).expect("remap back");
        assert!(doc.apply(&edit), "{edit:?} is done to the document");
        store.edit_pages(edit, undo, forward, backward);
    }

    /// A blank page as page `at`, as big as page `like`.
    fn insert(doc: &mut Doc, store: &mut InkStore, at: usize, like: usize) {
        let size = doc.page_size(like as u32);
        edit(doc, store, PageEdit::Insert { at, size });
    }

    fn widths(doc: &Doc) -> Vec<f32> {
        (0..doc.page_count()).map(|p| doc.page_size(p as u32)[0]).collect()
    }

    /// A stroke on `page`, as the document lays it out now.
    fn scribble(doc: &Doc, store: &mut InkStore, page: usize) {
        let at = doc.layout().page(page).expect("page");
        store.add(page as u32, RED, vec![[at.x + 30.0, at.y + 40.0], [at.x + 90.0, at.y + 70.0]]);
    }

    /// Delete page `at` the way the editor does, with the ink following.
    fn delete(doc: &mut Doc, store: &mut InkStore, at: usize) {
        edit(doc, store, PageEdit::delete(at));
    }

    #[test]
    fn a_page_deleted_from_the_middle_leaves_the_file_when_saved_and_the_ink_stays_with_its_pages() {
        let (mut doc, mut store, path) = open_pages("delete-middle", 4);
        for page in 0..4 {
            scribble(&doc, &mut store, page);
        }
        doc.save(&store).expect("first save");
        store.mark_clean();
        delete(&mut doc, &mut store, 1);
        assert_eq!(widths(&doc), vec![600.0, 620.0, 630.0], "the pages after it moved up");
        doc.save(&store).expect("save");

        let (reopened, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!(widths(&reopened), vec![600.0, 620.0, 630.0], "the file lost exactly the page that was deleted");
        let mut on: Vec<u32> = strokes.iter().map(|s| s.0).collect();
        on.sort_unstable();
        assert_eq!(on, vec![0, 1, 2], "the deleted page's ink went with it, the rest stayed on its own page, once each");
        // The strokes are where they were on their pages (the pages moved, the ink moved with them).
        for (page, _, pts, _, _, _, _) in &strokes {
            let r = reopened.layout().page(*page as usize).expect("page");
            assert!((pts[0][0] - r.x - 30.0).abs() < 0.5 && (pts[0][1] - r.y - 40.0).abs() < 0.5, "{pts:?} on page {page}");
        }
        // Saving again changes nothing: the file's own numbering was reset by the save.
        doc.save(&store).expect("save again");
        assert_eq!(widths(&Doc::open(&path).expect("again").0), vec![600.0, 620.0, 630.0]);
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn the_first_and_the_last_page_can_be_deleted_and_the_only_page_cannot() {
        let (mut doc, mut store, path) = open_pages("delete-ends", 3);
        delete(&mut doc, &mut store, 0);
        delete(&mut doc, &mut store, 1);
        assert_eq!(widths(&doc), vec![610.0]);
        assert!(!doc.apply(&PageEdit::delete(0)), "a document keeps at least one page");
        assert!(!doc.apply(&PageEdit::delete(5)), "and there is no page 5");
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![610.0], "the middle page is what is left");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn pages_added_and_pages_deleted_in_one_session_come_out_right_together() {
        let (mut doc, mut store, path) = open_pages("delete-and-add", 3);
        // [600, 610, 620] -> add a copy of page 0 after it -> delete the original second page.
        insert(&mut doc, &mut store, 1, 0);
        assert_eq!(widths(&doc), vec![600.0, 600.0, 610.0, 620.0]);
        delete(&mut doc, &mut store, 2);
        assert_eq!(widths(&doc), vec![600.0, 600.0, 620.0]);
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 600.0, 620.0], "the added page stayed, the deleted one went");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn deleting_a_page_that_was_only_added_leaves_the_file_as_it_was() {
        let (mut doc, mut store, path) = open_pages("delete-added", 2);
        insert(&mut doc, &mut store, 1, 0);
        delete(&mut doc, &mut store, 1);
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 610.0]);
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn a_document_whose_every_original_page_is_deleted_keeps_the_page_that_was_added() {
        let dir = temp_dir("delete-all-original");
        let mut doc = Doc::blank([600.0, 800.0]).expect("blank");
        let mut store = InkStore::new();
        insert(&mut doc, &mut store, 1, 0);
        delete(&mut doc, &mut store, 0);
        assert_eq!(doc.page_count(), 1);
        scribble(&doc, &mut store, 0);
        let path = dir.join("only-the-new-page.pdf");
        doc.save_as(&path, &store).expect("save as: the file's own page is going, the added one stays");
        let (reopened, strokes) = Doc::open(&path).expect("reopen");
        assert_eq!(reopened.page_count(), 1);
        assert_eq!(strokes.len(), 1, "and the ink on it");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pages_render_from_the_file_page_they_came_from_after_one_before_them_is_deleted() {
        let (mut doc, mut store, path) = open_pages("delete-render", 3);
        delete(&mut doc, &mut store, 0);
        assert_eq!((doc.render_page(0), doc.render_page(1)), (Some(1), Some(2)), "the pages that moved up are drawn from the pages they came from");
        doc.set_queue(&[Key::Preview { page: 2 }]);
        let started = Instant::now();
        let rendered = loop {
            if let Some(page) = doc.try_recv() {
                break page;
            }
            assert!(started.elapsed().as_secs() < 20, "the page was never rendered");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(Key::from_tag(rendered.tag), Key::Preview { page: 2 }, "tagged with the page of the file");
        assert_eq!(rendered.page, 2, "and drawn from the file's third page");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn links_follow_the_pages_that_move_up_and_go_with_the_page_that_is_deleted() {
        let dir = temp_dir("delete-links");
        let path = dir.join("links.pdf");
        fs::write(&path, crate::testutil::pdf_with_links()).expect("write");
        // The links go through the edit as the editor carries them.
        let through = |page: usize| {
            let (mut doc, contents) = Doc::open_contents(&path).expect("open");
            let mut links = contents.links;
            let before = doc.layout();
            let edit = PageEdit::delete(page);
            assert!(doc.apply(&edit));
            crate::links::remap(&mut links, &before, &Remap::new(&edit, &before, &doc.layout()).expect("remap"));
            (links, doc.page_count())
        };
        let all = Doc::open_contents(&path).expect("open").1.links.len();
        assert!(all > 0);
        // The sample's links are all on its first page; delete the second and they stay (those that went to it go).
        let to_second = Doc::open_contents(&path).expect("open").1.links.iter().filter(|l| l.target == crate::links::Target::Page(1)).count();
        let (links, pages) = through(1);
        assert_eq!(links.len(), all - to_second, "the links that went to the deleted page are gone");
        assert!(links.iter().all(|l| !matches!(l.target, crate::links::Target::Page(p) if p >= pages)));
        // Delete the first page: its links go with it, and no link is left pointing nowhere.
        let (links, pages) = through(0);
        assert!(links.iter().all(|l| !matches!(l.target, crate::links::Target::Page(p) if p >= pages)));
        let _ = fs::remove_dir_all(dir);
    }

    /// Undo the last step of the history the way the editor and the shell do: the ink follows inside the step, and the
    /// document is told what happened to the pages.
    fn undo(doc: &mut Doc, store: &mut InkStore) -> bool {
        let done = store.undo();
        if let Some(edit) = store.take_page_edit() {
            assert!(doc.apply(&edit), "the document follows the undoing: {edit:?}");
        }
        done
    }

    fn redo(doc: &mut Doc, store: &mut InkStore) -> bool {
        let done = store.redo();
        if let Some(edit) = store.take_page_edit() {
            assert!(doc.apply(&edit), "the document follows the redoing: {edit:?}");
        }
        done
    }

    /// The first point of each stroke in the file at `path`, with its page, to the tenth of a point, sorted.
    fn where_the_ink_is(path: &Path) -> Vec<(u32, i64, i64)> {
        let (_, strokes) = Doc::open(path).expect("reopen");
        let mut found: Vec<_> =
            strokes.iter().map(|(page, _, pts, ..)| (*page, (pts[0][0] * 10.0).round() as i64, (pts[0][1] * 10.0).round() as i64)).collect();
        found.sort_unstable();
        found
    }

    /// What `where_the_ink_is` should say for the strokes `store` holds, with the pages as `doc` has them: the file
    /// keeps a stroke in its page's own space and the reader turns it back into the document's, so the same points.
    fn where_the_ink_should_be(store: &InkStore) -> Vec<(u32, i64, i64)> {
        let mut found: Vec<_> =
            store.strokes().iter().map(|s| (s.page, (s.pts[0][0] * 10.0).round() as i64, (s.pts[0][1] * 10.0).round() as i64)).collect();
        found.sort_unstable();
        found
    }

    #[test]
    fn pages_moved_are_saved_in_their_new_order_and_their_ink_stays_with_them() {
        let (mut doc, mut store, path) = open_pages("move-pages", 5);
        for page in 0..5 {
            scribble(&doc, &mut store, page);
        }
        doc.save(&store).expect("first save");
        store.mark_clean();
        edit(&mut doc, &mut store, PageEdit::Move { pages: vec![3], to: 0 });
        assert_eq!(widths(&doc), vec![630.0, 600.0, 610.0, 620.0, 640.0]);
        edit(&mut doc, &mut store, PageEdit::Move { pages: vec![0, 4], to: 2 });
        assert_eq!(widths(&doc), vec![600.0, 610.0, 630.0, 640.0, 620.0]);
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 610.0, 630.0, 640.0, 620.0], "the file has the pages in the new order");
        assert_eq!(where_the_ink_is(&path), where_the_ink_should_be(&store), "and each page keeps its ink, where it was on it");
        // Saving again changes nothing: the numbering of the file is the document's now.
        doc.save(&store).expect("save again");
        assert_eq!(widths(&Doc::open(&path).expect("again").0), vec![600.0, 610.0, 630.0, 640.0, 620.0]);
        // And the moves can be undone, even after the save, since the file has all the pages.
        assert!(undo(&mut doc, &mut store) && undo(&mut doc, &mut store));
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0, 630.0, 640.0]);
        doc.save(&store).expect("save the undoing");
        assert_eq!(widths(&Doc::open(&path).expect("again").0), vec![600.0, 610.0, 620.0, 630.0, 640.0], "the file is as it was");
        assert_eq!(where_the_ink_is(&path), where_the_ink_should_be(&store));
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn a_deleted_page_comes_back_with_undo_and_the_file_is_as_if_nothing_had_happened() {
        let (mut doc, mut store, path) = open_pages("undo-delete", 4);
        for page in 0..4 {
            scribble(&doc, &mut store, page);
        }
        doc.save(&store).expect("first save");
        store.mark_clean();
        let ink = where_the_ink_is(&path);
        // The editor names the grave of a deletion, so that it can be put back.
        edit(&mut doc, &mut store, PageEdit::Delete { pages: vec![1, 2], grave: 5 });
        assert_eq!(widths(&doc), vec![600.0, 630.0]);
        assert!(undo(&mut doc, &mut store));
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0, 630.0], "both pages are back, in their places");
        assert_eq!((doc.render_page(1), doc.render_page(2)), (Some(1), Some(2)), "and are drawn from the file again");
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 610.0, 620.0, 630.0]);
        assert_eq!(where_the_ink_is(&path), ink, "with all their ink");
        // Redo deletes them again, and the file follows.
        assert!(redo(&mut doc, &mut store));
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 630.0]);
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn once_the_file_is_written_without_a_deleted_page_the_page_cannot_be_put_back() {
        let (mut doc, mut store, path) = open_pages("grave-cleared", 3);
        edit(&mut doc, &mut store, PageEdit::Delete { pages: vec![1], grave: 8 });
        doc.save(&store).expect("save");
        let restore = PageEdit::Restore { grave: 8, pages: vec![(1, [610.0, 800.0])] };
        assert!(!doc.apply(&restore), "the grave is empty: the file has been written without the page");
        assert_eq!(widths(&doc), vec![600.0, 620.0], "and nothing changed");
        assert!(!doc.apply(&PageEdit::Restore { grave: 99, pages: vec![(0, [600.0, 800.0])] }), "a grave that was never dug");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn adding_deleting_and_moving_pages_in_one_session_come_out_right_together() {
        let (mut doc, mut store, path) = open_pages("everything", 5);
        for page in 0..5 {
            scribble(&doc, &mut store, page);
        }
        // [600 610 620 630 640] -> a blank page after the first -> a page goes -> the last two swap -> a blank page in front.
        insert(&mut doc, &mut store, 1, 0);
        edit(&mut doc, &mut store, PageEdit::delete(3));
        assert_eq!(widths(&doc), vec![600.0, 600.0, 610.0, 630.0, 640.0]);
        edit(&mut doc, &mut store, PageEdit::Move { pages: vec![4], to: 3 });
        assert_eq!(widths(&doc), vec![600.0, 600.0, 610.0, 640.0, 630.0]);
        insert(&mut doc, &mut store, 0, 4);
        assert_eq!(widths(&doc), vec![630.0, 600.0, 600.0, 610.0, 640.0, 630.0]);
        doc.save(&store).expect("save");
        let reopened = Doc::open(&path).expect("reopen").0;
        assert_eq!(widths(&reopened), vec![630.0, 600.0, 600.0, 610.0, 640.0, 630.0]);
        assert_eq!(where_the_ink_is(&path), where_the_ink_should_be(&store), "every page kept its ink, and the page that went took its own");
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn a_document_follows_every_kind_of_edit_to_the_same_layout_the_editor_has() {
        let (mut doc, mut store, path) = open_pages("same-layout", 4);
        let mut editor_layout = doc.layout();
        for e in [
            PageEdit::Insert { at: 2, size: [500.0, 700.0] },
            PageEdit::Delete { pages: vec![0, 3], grave: 2 },
            PageEdit::Move { pages: vec![1], to: 0 },
            PageEdit::Arrange { order: vec![2, 0, 1] },
        ] {
            editor_layout = editor_layout.edited(&e).expect("layout");
            edit(&mut doc, &mut store, e);
            let mine = doc.layout();
            assert_eq!(mine.len(), editor_layout.len());
            assert!((0..mine.len()).all(|p| mine.page(p) == editor_layout.page(p)), "the same pages, laid out alike");
        }
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn copies_of_pages_are_saved_after_the_last_page_copied_with_their_ink_and_the_originals_keep_theirs() {
        let (mut doc, mut store, path) = open_pages("duplicate", 4);
        for page in 0..4 {
            scribble(&doc, &mut store, page);
        }
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![2, 1] });
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0, 610.0, 620.0, 630.0], "the copies come together after the page copied last");
        assert_eq!(store.len(), 6, "each copy has the ink of its page");
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 610.0, 620.0, 610.0, 620.0, 630.0], "the file has the copies");
        assert_eq!(where_the_ink_is(&path), where_the_ink_should_be(&store), "with their ink, and the others' ink where it was");
        // The copies draw from the page they copy.
        assert_eq!(
            (doc.render_page(3), doc.render_page(4)),
            (doc.render_page(1), doc.render_page(2)),
            "a copy is drawn from the page it copies: it shares its pictures"
        );
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn ink_saved_on_a_page_before_it_was_copied_is_not_doubled_on_the_copy() {
        let (mut doc, mut store, path) = open_pages("duplicate-saved", 3);
        scribble(&doc, &mut store, 1);
        doc.save(&store).expect("first save");
        store.mark_clean();
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![1] });
        assert_eq!(store.len(), 2);
        doc.save(&store).expect("save the copy");
        let ink = where_the_ink_is(&path);
        assert_eq!(ink.len(), 2, "one stroke on the page and one on its copy, not two on the copy: {ink:?}");
        assert_eq!(ink, where_the_ink_should_be(&store));
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn a_copy_outlives_the_page_it_was_made_from_and_copies_of_copies_and_of_added_pages_are_pages_too() {
        let (mut doc, mut store, path) = open_pages("duplicate-more", 3);
        for page in 0..3 {
            scribble(&doc, &mut store, page);
        }
        // Copy the middle page, then delete the original: the copy is what is left of it.
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![1] });
        edit(&mut doc, &mut store, PageEdit::delete(1));
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0]);
        // A copy of the copy, and of a page that was only added.
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![1] });
        insert(&mut doc, &mut store, 0, 0);
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![0] });
        assert_eq!(widths(&doc), vec![600.0, 600.0, 600.0, 610.0, 610.0, 620.0]);
        doc.save(&store).expect("save");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 600.0, 600.0, 610.0, 610.0, 620.0]);
        assert_eq!(where_the_ink_is(&path), where_the_ink_should_be(&store));
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }

    #[test]
    fn duplicating_can_be_undone_and_redone_and_the_file_follows() {
        let (mut doc, mut store, path) = open_pages("duplicate-undo", 3);
        for page in 0..3 {
            scribble(&doc, &mut store, page);
        }
        doc.save(&store).expect("first save");
        store.mark_clean();
        edit(&mut doc, &mut store, PageEdit::Duplicate { pages: vec![0, 2] });
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0, 600.0, 620.0]);
        assert!(undo(&mut doc, &mut store));
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0], "the copies are gone");
        assert_eq!(store.len(), 3, "with their ink");
        doc.save(&store).expect("save the undoing");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 610.0, 620.0]);
        assert!(redo(&mut doc, &mut store));
        assert_eq!(widths(&doc), vec![600.0, 610.0, 620.0, 600.0, 620.0]);
        assert_eq!(store.len(), 5, "and the ink is back");
        doc.save(&store).expect("save the redoing");
        assert_eq!(widths(&Doc::open(&path).expect("reopen").0), vec![600.0, 610.0, 620.0, 600.0, 620.0]);
        assert_eq!(where_the_ink_is(&path), where_the_ink_should_be(&store));
        let _ = fs::remove_dir_all(path.parent().expect("dir"));
    }
}
