//! The PDF engine behind one seam. The program asks a [`PdfBackend`] to open, render, read and change PDFs, in its own types; the one
//! implementation today is pdfcraft's (`pdfcraft.rs`), and it is the only code that names `pdfcraft_*` (a test says so). What any
//! backend must do is held by a conformance suite (`tests/corpus`), so moving to another version of pdfcraft, or one day another
//! pure-Rust engine, is a matter of running it.
//!
//! The seam is shaped by what three kinds of caller need: **rendering** (tiles, previews and thumbnails of a page, as pixels, with a
//! pool that works behind the frame), **text** (the glyphs of a page, [`PageTextView`]) and **the file** (open, read the links and the
//! outline, find, add and delete annotations, organize pages, write the whole file or an update appended to it).

mod pdfcraft;

#[cfg(test)]
mod contract;

use std::sync::{Arc, OnceLock};

use crate::coords::PageGeom;
use crate::ink::MarkupKind;
use crate::pagetext::PageTextView;

pub use pdfcraft::{device_pixels, effective_scale};

/// The backend this program was built with.
pub fn native() -> &'static dyn PdfBackend {
    static BACKEND: OnceLock<pdfcraft::Pdfcraft> = OnceLock::new();
    BACKEND.get_or_init(|| pdfcraft::Pdfcraft)
}

/// How pages are drawn: as the file has them, or on a dark paper with light text (what the program's dark pages are). Display only: the
/// file is never changed, and what the pages hold (ink, annotations) is saved as it was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageLook {
    #[default]
    Plain,
    Dark,
}

/// A region of a page to render, in device pixels at the request's scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// One render: a page, at a scale (device pixels per point), whole or as a tile. `tag` is the caller's own and comes back with the
/// result (the pdfcraft backend reserves bit 60 of it: keep it clear).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderRequest {
    pub page: usize,
    pub scale: f32,
    pub tag: u64,
    pub tile: Option<Tile>,
    /// A low-detail render for backdrops and thumbnails: never for a final one.
    pub preview: bool,
}

impl Default for RenderRequest {
    fn default() -> Self {
        Self { page: 0, scale: 1.0, tag: 0, tile: None, preview: false }
    }
}

/// Premultiplied RGBA8 pixels, row by row from the top, owned by whatever the backend made them in (no copy is made to hand them over).
pub struct Pixels(Box<dyn AsRef<[u8]> + Send + Sync>);

impl Pixels {
    pub fn new(pixels: impl AsRef<[u8]> + Send + Sync + 'static) -> Self {
        Self(Box::new(pixels))
    }

    pub fn as_slice(&self) -> &[u8] {
        (*self.0).as_ref()
    }
}

/// A finished render.
pub struct Rendered {
    /// The request's own tag, and the page it was for.
    pub tag: u64,
    #[allow(dead_code, reason = "read by the tests and the conformance suite")]
    pub page: usize,
    pub width: u32,
    pub height: u32,
    pub rgba: Pixels,
    /// Why it could not be drawn, when it could not (the pixels are then empty).
    pub error: Option<String>,
}

/// Renders pages on threads of its own, behind the frame: the caller says what it wants *now* and takes what is finished.
pub trait RenderPool {
    /// Replace the work with exactly these requests, most urgent first. Requests that are unchanged keep running.
    fn set_queue(&self, requests: Vec<RenderRequest>);
    /// A finished render, if there is one.
    fn try_recv(&self) -> Option<Rendered>;
}

/// A link on a page as the file has it: a box in the page's user space and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct RawLink {
    pub page: usize,
    /// `[x0, y0, x1, y1]` in the page's user space (the PDF's own, y up, before rotation and cropping).
    pub rect: [f32; 4],
    pub target: RawTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RawTarget {
    /// A page of this document (0-based), by an explicit or a named destination.
    Page(usize),
    /// A web address as the file wrote it (the program checks it before it is ever followed).
    Url(String),
}

/// What a form field holds, and what to put in one.
#[derive(Clone, Debug, PartialEq)]
pub enum RawValue {
    Text(String),
    /// A check box: checked or not.
    Check(bool),
    /// A radio group: the on state of the button chosen (none: none chosen).
    Radio(Option<String>),
    /// A combo or list box: the export values chosen.
    Choice(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawFieldKind {
    Text,
    Check,
    Radio,
    /// A combo box or a list box.
    Choice,
}

/// One place a form field is drawn: a box on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct RawWidget {
    pub page: usize,
    /// `[x0, y0, x1, y1]` in the page's user space, as for [`RawLink`].
    pub rect: [f32; 4],
    /// A check box or radio button: the name of its "on" appearance (what [`RawValue::Radio`] names).
    pub on_state: Option<String>,
}

/// A form field the program can fill, as the file has it. Fields that are read only, hidden, buttons, signatures or passwords are not offered.
#[derive(Clone, Debug, PartialEq)]
pub struct RawField {
    /// The field's full name, which is what [`PdfDocument::set_fields`] finds it by.
    pub name: String,
    pub kind: RawFieldKind,
    /// What it holds now: the text; the on state of a check box or radio group (empty when off); the export values chosen.
    pub value: Vec<String>,
    /// A choice's options: (export value, shown text).
    pub options: Vec<(String, String)>,
    pub multiline: bool,
    pub max_len: Option<usize>,
    pub widgets: Vec<RawWidget>,
}

/// One entry of the table of contents, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct RawOutlineItem {
    pub title: String,
    /// 0 for the top level.
    pub depth: usize,
    pub page: Option<usize>,
    /// Whether the file shows it opened out to begin with.
    pub open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotKind {
    Ink,
    /// Typed text (a FreeText annotation).
    Text,
    /// A text markup (highlight, underline, strike-out, squiggly): its boxes are in `Annotation::quads`.
    Markup(MarkupKind),
}

/// An annotation the file has that carries a name (the program marks its own by it), as read.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub page: usize,
    /// Its place in the page's annotations (what [`PdfDocument::delete_annotation`] takes).
    pub index: usize,
    pub kind: AnnotKind,
    pub name: String,
    /// The colour it is drawn in (red, green, blue, each 0 to 1): the stroke's, or the text's.
    pub colour: [f64; 3],
    pub opacity: f64,
    /// A stroke's thickness in points (0 for text).
    pub width: f64,
    /// An ink annotation's strokes, in the page's user space.
    pub strokes: Vec<Vec<[f32; 2]>>,
    /// A text annotation's text, its font size in points, and its box `[x0, y0, x1, y1]` in the page's user space.
    pub text: String,
    pub font_size: f64,
    pub rect: [f64; 4],
    /// A markup's boxes in the page's user space, each as the file writes them: the corners top-left, top-right, bottom-left, bottom-right
    /// (`x, y` for each).
    pub quads: Vec<[f64; 8]>,
}

/// What to add to a page.
#[derive(Clone, Debug, PartialEq)]
pub enum NewShape {
    Ink {
        strokes: Vec<Vec<[f64; 2]>>,
    },
    Text {
        rect: [f64; 4],
        font_size: f64,
    },
    /// A text markup over boxes (corners as in [`Annotation::quads`]).
    Markup {
        kind: MarkupKind,
        quads: Vec<[f64; 8]>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewAnnotation {
    pub page: usize,
    pub shape: NewShape,
    pub colour: [f64; 3],
    pub opacity: f64,
    pub width: f64,
    pub text: String,
    pub author: String,
    /// The name that marks it, and the date it was made (the file's own format, see [`PdfBackend::date`]).
    pub name: String,
    pub date: String,
}

/// Why pages could not be organized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrganizeError {
    /// Every page was asked to go: a file cannot have none.
    WouldRemoveAllPages,
    Other(String),
}

impl std::fmt::Display for OrganizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganizeError::WouldRemoveAllPages => f.write_str("the document would have no pages"),
            OrganizeError::Other(e) => f.write_str(e),
        }
    }
}

/// A PDF open for change: read what it holds, change its annotations and pages, and write it.
pub trait PdfDocument {
    fn page_count(&self) -> Result<usize, String>;
    /// Every link that goes somewhere the program can follow.
    fn links(&self) -> Vec<RawLink>;
    fn outline(&self) -> Vec<RawOutlineItem>;
    /// Make the top-level outline entries whose title begins with `prefix` (the program's own flags: see [`crate::outline::FLAG`]) be exactly
    /// one for each of `pages` (0-based), at the end of the outline, each titled `prefix` and the page's number; every other entry is left as it was.
    fn set_flags(&mut self, prefix: &str, pages: &[usize]) -> Result<(), String>;
    /// The form fields that can be filled (an empty list for a file with no form).
    fn fields(&self) -> Vec<RawField>;
    /// Fill the fields named, each with its value, and redraw them (and whatever the form calculates from them). A name the file does not
    /// have, a field that is read only and a value that does not fit are skipped: the rest are written.
    fn set_fields(&mut self, values: &[(String, RawValue)]) -> Result<(), String>;
    /// The annotations that have a name and are ink or typed text, page by page, in the order the file has them.
    fn annotations(&self) -> Vec<Annotation>;
    fn delete_annotation(&mut self, page: usize, index: usize) -> Result<(), String>;
    fn add_annotation(&mut self, new: &NewAnnotation) -> Result<(), String>;
    fn delete_pages(&mut self, pages: &[usize]) -> Result<(), OrganizeError>;
    /// Copy pages to `at` (the program always says where; with `None` each copy goes right after the page it copies).
    fn duplicate_pages(&mut self, pages: &[usize], at: Option<usize>) -> Result<(), String>;
    fn insert_blank_page(&mut self, at: usize, size: [f32; 2]) -> Result<(), String>;
    fn move_pages(&mut self, pages: &[usize], to: usize) -> Result<(), String>;
    /// The whole file, rewritten.
    fn save_full(&self) -> Result<Vec<u8>, String>;
    /// An update appended to the file it was opened from, with `modified` (a date, see [`PdfBackend::date`]) as its modification date.
    fn save_incremental(&self, modified: &str) -> Result<Vec<u8>, String>;
}

/// The text of the pages of one file, read page by page. A source is made on the thread that uses it (a parsed file is not shared between
/// threads), and keeps the file parsed between pages: reading every page of a long document is one parse, not one per page.
pub trait TextSource {
    /// The text of a page: no text for a page with none, an error for a page that is not there or cannot be read.
    fn page(&mut self, page: usize) -> Result<Box<dyn PageTextView>, String>;
}

/// The PDF engine.
pub trait PdfBackend: Send + Sync {
    /// The geometry of each page, as the file says (size, crop box, rotation). An error if it is not a PDF this engine can read.
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<Vec<PageGeom>, String>;
    /// The bytes of a new document of one blank page of this size (points).
    fn blank(&self, size: [f32; 2]) -> Result<Vec<u8>, String>;
    fn open(&self, bytes: &Arc<Vec<u8>>) -> Result<Box<dyn PdfDocument>, String>;
    /// A pool of `threads` threads rendering pages of this file, in this look.
    fn render_pool(&self, bytes: Arc<Vec<u8>>, threads: usize, look: PageLook) -> Box<dyn RenderPool>;
    /// One render on this thread, now (a thumbnail of a file that is not open).
    fn render_now(&self, bytes: Arc<Vec<u8>>, request: &RenderRequest) -> Rendered;
    /// The text of one page (a one-off: from a fresh parse; see [`PdfBackend::text_source`] for many).
    #[allow(dead_code, reason = "the conformance suite is its user")]
    fn page_text(&self, bytes: &Arc<Vec<u8>>, page: usize) -> Result<Box<dyn PageTextView>, String>;
    /// A reader of the text of this file's pages.
    fn text_source(&self, bytes: Arc<Vec<u8>>) -> Box<dyn TextSource>;
    /// A date in the file's own format, for a modification date and an annotation's.
    fn date(&self, unix_seconds: i64) -> String;
}
