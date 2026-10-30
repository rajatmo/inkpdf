//! pdfcraft behind the [`PdfBackend`] seam: the only code in the program that names `pdfcraft_*`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pdfcraft_annot::appearance::parse_da;
use pdfcraft_annot::{Markup, Meta, NewAnnotation as AnnotNew, Shape, Style as AnnotStyle, add_annotation, delete_annotation, page_refs};
use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, pdf_date, write_full, write_incremental};
use pdfcraft_organize::{OrganizeError as OrgError, bookmarks, delete_pages, duplicate_pages, insert_blank_page, move_pages, page_count};
use pdfcraft_render::{
    PageRenderer, PageText, ReadingColors, ReadingParams, RenderConfig, RenderPool as Pool, RenderRequest as Request, RenderedPage, Tile as PdfTile,
    text::TextFilter,
};

use super::{
    AnnotKind, Annotation, NewAnnotation, NewShape, OrganizeError, PageLook, PdfBackend, PdfDocument, Pixels, RawLink, RawOutlineItem, RawTarget,
    RenderPool, RenderRequest, Rendered, TextSource,
};
use crate::coords::PageGeom;
use crate::ink::MarkupKind;
use crate::pagetext::PageTextView;

/// Most links read from one file, so a hostile file cannot fill memory.
const MAX_LINKS: usize = 200_000;
/// Named destinations read from one file, and how deep their tree may go.
const MAX_NAMED: usize = 200_000;
const MAX_TREE_DEPTH: usize = 24;
/// Most outline entries read from one file, so a hostile file cannot fill memory.
const MAX_ITEMS: usize = 20_000;
/// The bit of a request's tag that pdfcraft reads as "a print preview" (it then ignores the reading colours). Ours must keep it clear.
const PRINT_TAG: u64 = 1 << 60;

/// How many device pixels cover `pt` points at `scale` (what the renderer will make, so the caller can lay tiles out exactly).
pub fn device_pixels(pt: f32, scale: f32) -> u32 {
    pdfcraft_render::device_pixels(pt, scale)
}

/// A scale the renderer will accept for a page of this size: its caps on the side and the area applied.
pub fn effective_scale(width_pt: f32, height_pt: f32, scale: f32) -> f32 {
    pdfcraft_render::effective_scale(width_pt, height_pt, scale)
}

pub struct Pdfcraft;

/// What a look asks of pdfcraft: its reading colours, with the dark paper as the background and light text (near-black paints go to the
/// text colour, light ones to the paper; photographs keep their colours).
fn config_of(look: PageLook) -> RenderConfig {
    match look {
        PageLook::Plain => RenderConfig::default(),
        PageLook::Dark => RenderConfig {
            reading_colors: ReadingColors {
                params: Some(ReadingParams {
                    text: crate::palette::DARK_TEXT,
                    background: crate::palette::DARK_PAPER,
                    only_black: false,
                    line_art: true,
                }),
            },
            ..RenderConfig::default()
        },
    }
}

fn markup_of(kind: MarkupKind) -> Markup {
    match kind {
        MarkupKind::Highlight => Markup::Highlight,
        MarkupKind::Underline => Markup::Underline,
        MarkupKind::StrikeOut => Markup::StrikeOut,
        MarkupKind::Squiggly => Markup::Squiggly,
    }
}

fn request_of(r: &RenderRequest) -> Request {
    debug_assert_eq!(r.tag & PRINT_TAG, 0, "bit 60 of a tag is pdfcraft's (a print preview): keep it clear");
    Request {
        page: r.page,
        scale: r.scale,
        tag: r.tag & !PRINT_TAG,
        tile: r.tile.map(|t| PdfTile { x: t.x, y: t.y, w: t.w, h: t.h }),
        preview: r.preview,
        ..Request::default()
    }
}

fn rendered_of(page: RenderedPage) -> Rendered {
    Rendered {
        tag: page.request.tag,
        page: page.request.page,
        width: page.width,
        height: page.height,
        rgba: Pixels::new(page.rgba),
        error: page.error,
    }
}

struct PdfcraftPool(Pool);

impl RenderPool for PdfcraftPool {
    fn set_queue(&self, requests: Vec<RenderRequest>) {
        self.0.set_queue(requests.iter().map(request_of).collect());
    }

    fn try_recv(&self) -> Option<Rendered> {
        self.0.try_recv().map(rendered_of)
    }
}

impl PdfBackend for Pdfcraft {
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<Vec<PageGeom>, String> {
        let info = pdfcraft_render::inspect(bytes.clone(), None).map_err(|e| e.to_string())?;
        Ok(info.pages.iter().map(|p| PageGeom { width: p.width, height: p.height, crop: p.crop, rotation: p.rotation }).collect())
    }

    fn blank(&self, size: [f32; 2]) -> Result<Vec<u8>, String> {
        let pdf = pdfcraft_create::blank(f64::from(size[0]), f64::from(size[1]), 1).map_err(|e| e.to_string())?;
        write_full(&pdf, &SaveOptions::default()).map_err(|e| e.to_string())
    }

    fn open(&self, bytes: &Arc<Vec<u8>>) -> Result<Box<dyn PdfDocument>, String> {
        Ok(Box::new(PdfcraftDoc { doc: Document::open(bytes.clone()).map_err(|e| e.to_string())? }))
    }

    fn render_pool(&self, bytes: Arc<Vec<u8>>, threads: usize, look: PageLook) -> Box<dyn RenderPool> {
        Box::new(PdfcraftPool(Pool::new(bytes, threads, config_of(look))))
    }

    fn render_now(&self, bytes: Arc<Vec<u8>>, request: &RenderRequest) -> Rendered {
        rendered_of(PageRenderer::new(bytes, RenderConfig::default()).render(request_of(request)))
    }

    fn page_text(&self, bytes: &Arc<Vec<u8>>, page: usize) -> Result<Box<dyn PageTextView>, String> {
        self.text_source(bytes.clone()).page(page)
    }

    fn text_source(&self, bytes: Arc<Vec<u8>>) -> Box<dyn TextSource> {
        Box::new(PdfcraftSource(PageRenderer::new(bytes, RenderConfig::default())))
    }

    fn date(&self, unix_seconds: i64) -> String {
        pdf_date(unix_seconds)
    }
}

struct PdfcraftSource(PageRenderer);

impl TextSource for PdfcraftSource {
    fn page(&mut self, page: usize) -> Result<Box<dyn PageTextView>, String> {
        Ok(Box::new(PdfcraftText(self.0.filtered_text(page, TextFilter::default())?)))
    }
}

struct PdfcraftText(PageText);

impl PageTextView for PdfcraftText {
    fn glyph_count(&self) -> usize {
        self.0.glyphs.len()
    }

    fn nearest(&self, x: f32, y: f32) -> Option<usize> {
        self.0.nearest(x, y)
    }

    fn word_at(&self, glyph: usize) -> Option<(usize, usize)> {
        self.0.word_at(glyph)
    }

    fn line_at(&self, glyph: usize) -> Option<(usize, usize)> {
        self.0.line_at(glyph)
    }

    fn line_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        self.0.line_rects(range)
    }

    fn glyph_rects(&self, glyphs: &[usize]) -> Vec<[f32; 4]> {
        self.0.glyph_rects(glyphs)
    }

    fn glyphs_in(&self, rect: [f32; 4]) -> Vec<usize> {
        self.0.glyphs_in(rect)
    }

    fn column_text(&self, glyphs: &[usize]) -> String {
        self.0.column_text(glyphs)
    }

    fn text_of(&self, range: std::ops::Range<usize>) -> String {
        self.0.text_of(range)
    }

    fn plain_text(&self) -> String {
        self.0.plain_text()
    }

    fn find(&self, needle: &str, case_sensitive: bool, whole_words: bool, limit: usize) -> Vec<std::ops::Range<usize>> {
        self.0.find_opts_limited(needle, case_sensitive, whole_words, limit)
    }
}

struct PdfcraftDoc {
    doc: Document,
}

fn numbers(doc: &Document, dict: &Dict, key: &[u8]) -> Vec<f64> {
    dict.get(key)
        .map(|o| doc.resolve(o))
        .and_then(|o| o.as_array().map(|a| a.iter().map(|x| doc.resolve(x).as_f64().unwrap_or(0.0)).collect()))
        .unwrap_or_default()
}

fn org_err(e: OrgError) -> OrganizeError {
    match e {
        OrgError::WouldRemoveAllPages => OrganizeError::WouldRemoveAllPages,
        other => OrganizeError::Other(other.to_string()),
    }
}

impl PdfDocument for PdfcraftDoc {
    fn page_count(&self) -> Result<usize, String> {
        page_count(&self.doc).map_err(|e| e.to_string())
    }

    fn links(&self) -> Vec<RawLink> {
        let doc = &self.doc;
        let Ok(pages) = page_refs(doc) else { return Vec::new() };
        let mut named = NamedDests::new(doc, &pages);
        let mut out = Vec::new();
        for (page, page_ref) in pages.iter().enumerate() {
            let page_obj = doc.get(*page_ref);
            let Some(annots) = page_obj.as_dict().and_then(|d| d.get(b"Annots")).map(|a| doc.resolve(a)) else { continue };
            let Some(list) = annots.as_array() else { continue };
            for entry in list {
                let obj = doc.resolve(entry);
                let Some(d) = obj.as_dict() else { continue };
                if d.name(b"Subtype") != Some(b"Link") {
                    continue;
                }
                let Some(rect) = d.get(b"Rect").map(|r| doc.resolve(r)).and_then(|r| {
                    let nums: Vec<f64> = r.as_array()?.iter().filter_map(|x| doc.resolve(x).as_f64()).collect();
                    let [x0, y0, x1, y1] = nums[..] else { return None };
                    Some([x0 as f32, y0 as f32, x1 as f32, y1 as f32])
                }) else {
                    continue;
                };
                let Some(target) = target_of(doc, d, &pages, &mut named) else { continue };
                out.push(RawLink { page, rect, target });
                if out.len() >= MAX_LINKS {
                    return out;
                }
            }
        }
        out
    }

    fn outline(&self) -> Vec<RawOutlineItem> {
        bookmarks(&self.doc)
            .iter()
            .take(MAX_ITEMS)
            .map(|b| RawOutlineItem { title: b.title.clone(), depth: b.path.len().saturating_sub(1), page: b.page, open: b.open })
            .collect()
    }

    fn annotations(&self) -> Vec<Annotation> {
        let doc = &self.doc;
        let mut out = Vec::new();
        let Ok(pages) = page_refs(doc) else { return out };
        for (page, page_ref) in pages.iter().enumerate() {
            let page_obj = doc.get(*page_ref);
            let Some(page_dict) = page_obj.as_dict() else { continue };
            let Some(annots) = page_dict.get(b"Annots").map(|a| doc.resolve(a)) else { continue };
            let Some(list) = annots.as_array() else { continue };
            for (index, entry) in list.iter().enumerate() {
                let obj = doc.resolve(entry);
                let Some(d) = obj.as_dict() else { continue };
                let kind = match d.name(b"Subtype") {
                    Some(b"Ink") => AnnotKind::Ink,
                    Some(b"FreeText") => AnnotKind::Text,
                    Some(b"Highlight") => AnnotKind::Markup(MarkupKind::Highlight),
                    Some(b"Underline") => AnnotKind::Markup(MarkupKind::Underline),
                    Some(b"StrikeOut") => AnnotKind::Markup(MarkupKind::StrikeOut),
                    Some(b"Squiggly") => AnnotKind::Markup(MarkupKind::Squiggly),
                    _ => continue,
                };
                let name = match d.get(b"NM").map(|o| doc.resolve(o)).as_deref() {
                    Some(Object::String(s)) => s.to_text(),
                    _ => continue,
                };
                let opacity = d.get(b"CA").map(|o| doc.resolve(o)).and_then(|o| o.as_f64()).unwrap_or(1.0);
                if kind == AnnotKind::Text {
                    let Some(contents) = d.get(b"Contents").map(|o| doc.resolve(o)) else { continue };
                    let Object::String(s) = &*contents else { continue };
                    let rect = numbers(doc, d, b"Rect");
                    if rect.len() != 4 {
                        continue;
                    }
                    let (colour, size) = parse_da(d);
                    out.push(Annotation {
                        page,
                        index,
                        kind,
                        name,
                        colour,
                        opacity,
                        width: 0.0,
                        strokes: Vec::new(),
                        text: s.to_text(),
                        font_size: size,
                        rect: [rect[0], rect[1], rect[2], rect[3]],
                        quads: Vec::new(),
                    });
                    continue;
                }
                if matches!(kind, AnnotKind::Markup(_)) {
                    let c = numbers(doc, d, b"C");
                    let colour = if c.len() == 3 { [c[0], c[1], c[2]] } else { [1.0, 0.94, 0.0] };
                    let quads = numbers(doc, d, b"QuadPoints").chunks_exact(8).map(|q| [q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7]]).collect();
                    out.push(Annotation {
                        page,
                        index,
                        kind,
                        name,
                        colour,
                        opacity,
                        width: 0.0,
                        strokes: Vec::new(),
                        text: String::new(),
                        font_size: 0.0,
                        rect: [0.0; 4],
                        quads,
                    });
                    continue;
                }
                let ink_list = d.get(b"InkList").map(|o| doc.resolve(o));
                let strokes: Vec<Vec<[f32; 2]>> = ink_list
                    .as_ref()
                    .and_then(|l| l.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| {
                                let row = doc.resolve(row);
                                let nums: Vec<f64> = row.as_array()?.iter().map(|x| doc.resolve(x).as_f64().unwrap_or(0.0)).collect();
                                Some(nums.as_chunks::<2>().0.iter().map(|&[x, y]| [x as f32, y as f32]).collect())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let c = numbers(doc, d, b"C");
                let colour = if c.len() == 3 { [c[0], c[1], c[2]] } else { [0.0; 3] };
                let width =
                    d.get(b"BS").map(|b| doc.resolve(b)).and_then(|b| b.as_dict().and_then(|b| b.get(b"W").and_then(Object::as_f64))).unwrap_or(1.0);
                out.push(Annotation {
                    page,
                    index,
                    kind,
                    name,
                    colour,
                    opacity,
                    width,
                    strokes,
                    text: String::new(),
                    font_size: 0.0,
                    rect: [0.0; 4],
                    quads: Vec::new(),
                });
            }
        }
        out
    }

    fn delete_annotation(&mut self, page: usize, index: usize) -> Result<(), String> {
        delete_annotation(&mut self.doc, page, index).map_err(|e| e.to_string())
    }

    fn add_annotation(&mut self, new: &NewAnnotation) -> Result<(), String> {
        let shape = match &new.shape {
            NewShape::Ink { strokes } => Shape::Ink { strokes: strokes.clone() },
            NewShape::Text { rect, font_size } => Shape::Typewriter { rect: *rect, font_size: *font_size },
            NewShape::Markup { kind, quads } => Shape::TextMarkup { kind: markup_of(*kind), quads: quads.clone() },
        };
        let annot = AnnotNew {
            page: new.page,
            shape,
            style: AnnotStyle { color: new.colour, opacity: new.opacity, width: new.width, fill: None },
            contents: new.text.clone(),
            author: new.author.clone(),
        };
        add_annotation(&mut self.doc, &annot, &Meta { date: Some(new.date.clone()), id: new.name.clone() }).map(drop).map_err(|e| e.to_string())
    }

    fn delete_pages(&mut self, pages: &[usize]) -> Result<(), OrganizeError> {
        delete_pages(&mut self.doc, pages).map_err(org_err)
    }

    fn duplicate_pages(&mut self, pages: &[usize], at: Option<usize>) -> Result<(), String> {
        duplicate_pages(&mut self.doc, pages, at).map_err(|e| e.to_string())
    }

    fn insert_blank_page(&mut self, at: usize, size: [f32; 2]) -> Result<(), String> {
        insert_blank_page(&mut self.doc, at, f64::from(size[0]), f64::from(size[1])).map(drop).map_err(|e| e.to_string())
    }

    fn move_pages(&mut self, pages: &[usize], to: usize) -> Result<(), String> {
        move_pages(&mut self.doc, pages, to).map_err(|e| e.to_string())
    }

    fn save_full(&self) -> Result<Vec<u8>, String> {
        write_full(&self.doc, &SaveOptions::default()).map_err(|e| e.to_string())
    }

    fn save_incremental(&self, modified: &str) -> Result<Vec<u8>, String> {
        write_incremental(&self.doc, &SaveOptions { mod_date: Some(modified.to_string()), ..SaveOptions::default() }).map_err(|e| e.to_string())
    }
}

fn target_of(doc: &Document, d: &Dict, pages: &[ObjRef], named: &mut NamedDests) -> Option<RawTarget> {
    if let Some(dest) = d.get(b"Dest") {
        return dest_page(doc, dest, pages, named).map(RawTarget::Page);
    }
    let action = d.get(b"A").map(|a| doc.resolve(a))?;
    let action = action.as_dict()?;
    match action.name(b"S")? {
        b"URI" => {
            let uri = action.get(b"URI").map(|u| doc.resolve(u))?;
            let bytes = &uri.as_string()?.bytes;
            Some(RawTarget::Url(String::from_utf8_lossy(bytes).into_owned()))
        }
        b"GoTo" => dest_page(doc, action.get(b"D")?, pages, named).map(RawTarget::Page),
        _ => None,
    }
}

/// The page a destination (an explicit array, or the name of one) is on.
fn dest_page(doc: &Document, dest: &Object, pages: &[ObjRef], named: &mut NamedDests) -> Option<usize> {
    let dest = doc.resolve(dest);
    match &*dest {
        Object::Array(a) => page_of_array(a, pages),
        Object::Name(_) | Object::String(_) => named.page_of(&text_bytes(&dest)?),
        _ => None,
    }
}

/// An explicit destination, `[page /XYZ ...]`: the page object it names.
fn page_of_array(array: &[Object], pages: &[ObjRef]) -> Option<usize> {
    let page = array.first()?.as_ref()?;
    pages.iter().position(|p| *p == page)
}

fn text_bytes(o: &Object) -> Option<Vec<u8>> {
    match o {
        Object::Name(_) => o.as_name().map(<[u8]>::to_vec),
        Object::String(s) => Some(s.bytes.clone()),
        _ => None,
    }
}

/// Named destinations (`/Dests` in the catalog, or the `/Names /Dests` tree), read once, when the
/// first link needs one, into a table from name to page.
struct NamedDests<'a> {
    doc: &'a Document,
    pages: &'a [ObjRef],
    table: Option<HashMap<Vec<u8>, usize>>,
}

impl<'a> NamedDests<'a> {
    fn new(doc: &'a Document, pages: &'a [ObjRef]) -> Self {
        Self { doc, pages, table: None }
    }

    fn page_of(&mut self, name: &[u8]) -> Option<usize> {
        if self.table.is_none() {
            self.table = Some(self.build());
        }
        self.table.as_ref()?.get(name).copied()
    }

    fn build(&self) -> HashMap<Vec<u8>, usize> {
        let mut table = HashMap::new();
        let Some(root) = self.doc.root() else { return table };
        let catalog = self.doc.get(root);
        let Some(catalog) = catalog.as_dict() else { return table };
        if let Some(legacy) = catalog.get(b"Dests").map(|d| self.doc.resolve(d))
            && let Some(dict) = legacy.as_dict()
        {
            for (name, value) in dict.iter() {
                if let Some(page) = self.value_page(value) {
                    table.insert(name.clone(), page);
                }
            }
        }
        let tree = catalog.get(b"Names").map(|n| self.doc.resolve(n));
        if let Some(tree) = tree.as_ref().and_then(|n| n.as_dict()).and_then(|n| n.get(b"Dests")) {
            self.walk(tree, 0, &mut HashSet::new(), &mut table);
        }
        table
    }

    /// A destination value: an array, or a dictionary holding one under `/D`.
    fn value_page(&self, value: &Object) -> Option<usize> {
        let value = self.doc.resolve(value);
        match &*value {
            Object::Array(a) => page_of_array(a, self.pages),
            Object::Dict(d) => match &*self.doc.resolve(d.get(b"D")?) {
                Object::Array(a) => page_of_array(a, self.pages),
                _ => None,
            },
            _ => None,
        }
    }

    fn walk(&self, node: &Object, depth: usize, seen: &mut HashSet<ObjRef>, table: &mut HashMap<Vec<u8>, usize>) {
        if depth > MAX_TREE_DEPTH || table.len() >= MAX_NAMED || node.as_ref().is_some_and(|r| !seen.insert(r)) {
            return;
        }
        let node = self.doc.resolve(node);
        let Some(dict) = node.as_dict() else { return };
        if let Some(names) = dict.get(b"Names").map(|n| self.doc.resolve(n))
            && let Some(pairs) = names.as_array()
        {
            for [name, value] in pairs.as_chunks::<2>().0 {
                if let (Some(name), Some(page)) = (text_bytes(&self.doc.resolve(name)), self.value_page(value)) {
                    table.insert(name, page);
                }
            }
        }
        if let Some(kids) = dict.get(b"Kids").map(|k| self.doc.resolve(k))
            && let Some(kids) = kids.as_array()
        {
            for kid in kids {
                self.walk(kid, depth + 1, seen, table);
            }
        }
    }
}
