//! Reading ink from a PDF and writing it back, as standard Ink annotations.
//!
//! Only annotations this app wrote (their `/NM` starts with `inkpdf-`) are loaded as editable
//! strokes; every other annotation is left exactly as it is. A shape (see [`crate::figure`]) is
//! written as an annotation of its own whose `/NM` also carries the shape's anchor points and
//! handles after a `~`, so it can be edited again after the file is reopened; any other reader sees
//! an ordinary ink annotation. On save, the pages whose ink changed
//! have our annotations deleted and rewritten, then the file is updated incrementally, so the
//! original bytes stay untouched at the start of the file.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::coords::PageGeom;
use crate::figure::Figure;
use crate::form::Form;
use crate::ink::{InkStore, Item, MarkupKind, P, PageStroke, Style, TextBlock};
use crate::links::Link;
use crate::outline::OutlineItem;
use crate::pdf::{self, AnnotKind, NewAnnotation, NewShape, OrganizeError, PdfBackend, PdfDocument, Reply};
use crate::tools::items;
use crate::view::Layout;

/// What marks an annotation as ours.
const NAME_PREFIX: &str = "inkpdf-";
/// Where, in an annotation's name, the shape it was made from begins.
const FIGURE_MARK: char = '~';
/// What an extension's item's name begins with after ours, then the name of its kind and a `:` (see `inkpdf_api::ItemKind`).
const ITEM_MARK: &str = "inkpdf-item:";
/// Strokes of one style written into one annotation.
const STROKES_PER_ANNOTATION: usize = 32;

pub struct Loaded {
    /// `(page, style, points in document space)`, in drawing order.
    pub strokes: Vec<PageStroke>,
    /// What the page renderer should draw: the file without our ink annotations, because the ink
    /// layer draws those (so they can be erased and moved). The file itself when it has none.
    pub render_bytes: Arc<Vec<u8>>,
    /// The links the file has, where touch can follow them, and a link for each box of a form field (see [`crate::form`]).
    pub links: Vec<Link>,
    /// The fields of the file's form that can be filled.
    pub form: Form,
    /// The table of contents the file has.
    pub outline: Vec<OutlineItem>,
    /// What the file's owner allows to be done to it.
    pub permissions: crate::permissions::Permissions,
}

struct Found {
    page: usize,
    index: usize,
    style: Style,
    /// One entry per stroke, in user space.
    strokes: Vec<Vec<[f32; 2]>>,
    /// The shape this annotation's one stroke was made from, as written (see [`Figure::encode`]).
    figure: Option<String>,
    /// For a text annotation (FreeText): its text and size, and where its box is, in user space.
    text: Option<FoundText>,
    /// For a text markup: its kind and its boxes, in user space, as the file has them.
    markup: Option<(MarkupKind, Vec<[f64; 8]>)>,
    /// For an extension's item: what it holds (see [`FoundItem`]).
    item: Option<FoundItem>,
}

struct FoundItem {
    kind: String,
    /// What the kind made of the note the file holds.
    data: String,
    /// `[x0, y0, x1, y1]` with `y0 <= y1`, in user space.
    rect: [f32; 4],
}

struct FoundText {
    text: String,
    size: f32,
    /// `[x0, y0, x1, y1]` with `y0 <= y1`.
    rect: [f32; 4],
}

/// Our annotations in the document, with their positions in each page's `/Annots`: the ones with a name that starts with ours.
fn find_ours(doc: &dyn PdfDocument) -> Vec<Found> {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    doc.annotations()
        .into_iter()
        .filter(|a| a.name.starts_with(NAME_PREFIX))
        .filter_map(|a| {
            let rgba = [byte(a.colour[0]), byte(a.colour[1]), byte(a.colour[2]), byte(a.opacity)];
            match a.kind {
                AnnotKind::Text => {
                    let text = crate::text::clean(&a.text);
                    if text.is_empty() {
                        return None;
                    }
                    let r = a.rect;
                    let rect = [r[0].min(r[2]) as f32, r[1].min(r[3]) as f32, r[0].max(r[2]) as f32, r[1].max(r[3]) as f32];
                    let size = (a.font_size as f32).clamp(1.0, 400.0);
                    Some(Found {
                        page: a.page,
                        index: a.index,
                        style: Style { rgba, width: 0.0 },
                        strokes: Vec::new(),
                        figure: None,
                        text: Some(FoundText { text, size, rect }),
                        markup: None,
                        item: None,
                    })
                }
                AnnotKind::Markup(kind) => Some(Found {
                    page: a.page,
                    index: a.index,
                    style: Style { rgba, width: 1.0 },
                    strokes: Vec::new(),
                    figure: None,
                    text: None,
                    markup: Some((kind, a.quads)),
                    item: None,
                }),
                AnnotKind::Note => {
                    // Only a note this build has a kind for: any other is left in the file as it is, for the renderer to draw.
                    let kind = a.name.strip_prefix(ITEM_MARK).and_then(|rest| rest.split_once(':')).map(|(kind, _)| kind)?;
                    // (A reply that sets a status is not said: the latest one says whether the thread is done.)
                    let closed = a.replies.iter().rev().find_map(|r| r.state.as_deref()) == Some("Completed");
                    let note = inkpdf_api::Note {
                        author: a.author.clone(),
                        date: a.date.clone(),
                        text: a.text.clone(),
                        replies: a
                            .replies
                            .iter()
                            .filter(|r| r.state.is_none())
                            .map(|r| inkpdf_api::Message { author: r.author.clone(), date: r.date.clone(), text: r.text.clone() })
                            .collect(),
                        closed,
                    };
                    let data = items::kind(kind)?.read(&note)?;
                    let r = a.rect;
                    let rect = [r[0].min(r[2]) as f32, r[1].min(r[3]) as f32, r[0].max(r[2]) as f32, r[1].max(r[3]) as f32];
                    Some(Found {
                        page: a.page,
                        index: a.index,
                        style: Style { rgba, width: 1.0 },
                        strokes: Vec::new(),
                        figure: None,
                        text: None,
                        markup: None,
                        item: Some(FoundItem { kind: kind.to_string(), data, rect }),
                    })
                }
                AnnotKind::Ink => Some(Found {
                    page: a.page,
                    index: a.index,
                    style: Style { rgba, width: a.width as f32 },
                    strokes: a.strokes,
                    figure: a.name.split_once(FIGURE_MARK).map(|(_, f)| f.to_string()),
                    text: None,
                    markup: None,
                    item: None,
                }),
            }
        })
        .collect()
}

/// Read the ink this app saved earlier, with the program's PDF engine.
#[cfg(test)]
pub fn load(bytes: &Arc<Vec<u8>>, geoms: &[PageGeom], layout: &Layout) -> Result<Loaded, String> {
    load_with(pdf::native(), bytes, geoms, layout)
}

/// [`load`] with another PDF engine.
pub fn load_with(engine: &dyn PdfBackend, bytes: &Arc<Vec<u8>>, geoms: &[PageGeom], layout: &Layout) -> Result<Loaded, String> {
    let mut doc = engine.open(bytes)?;
    let mut links = crate::pdf_read::links(&*doc, geoms, layout);
    let (form, boxes) = crate::pdf_read::form(&*doc, geoms, layout);
    // (The boxes go last: where a link and a box overlap, the box is what a press finds.)
    links.extend(boxes);
    let outline = crate::pdf_read::outline(&*doc);
    let permissions = doc.permissions();
    let found = find_ours(&*doc);
    if found.is_empty() {
        return Ok(Loaded { strokes: Vec::new(), render_bytes: bytes.clone(), links, form, outline, permissions });
    }
    let mut strokes = Vec::new();
    for f in &found {
        let (Some(geom), Some(rect)) = (geoms.get(f.page), layout.page(f.page)) else { continue };
        let to_doc = |p: P| {
            let v = geom.user_to_view(p);
            [rect.x + v[0], rect.y + v[1]]
        };
        if let Some(t) = &f.text {
            // The box's top-left corner, wherever the page's rotation put it.
            let corners = [[t.rect[0], t.rect[1]], [t.rect[2], t.rect[1]], [t.rect[2], t.rect[3]], [t.rect[0], t.rect[3]]].map(to_doc);
            let top_left = [corners.iter().map(|c| c[0]).fold(f32::MAX, f32::min), corners.iter().map(|c| c[1]).fold(f32::MAX, f32::min)];
            let block = TextBlock { text: t.text.clone(), size: t.size };
            strokes.push((f.page as u32, f.style, crate::text::outline(top_left, &t.text, t.size), None, Some(block), None, None));
            continue;
        }
        if let Some(it) = &f.item {
            // The box the note is in, wherever the page's rotation put its corners.
            let corners = [[it.rect[0], it.rect[1]], [it.rect[2], it.rect[1]], [it.rect[2], it.rect[3]], [it.rect[0], it.rect[3]]].map(to_doc);
            let lo = corners.iter().fold([f32::MAX; 2], |m, c| [m[0].min(c[0]), m[1].min(c[1])]);
            let hi = corners.iter().fold([f32::MIN; 2], |m, c| [m[0].max(c[0]), m[1].max(c[1])]);
            strokes.push((f.page as u32, f.style, vec![lo, hi], None, None, None, Some(Item { kind: it.kind.clone(), data: it.data.clone() })));
            continue;
        }
        if let Some((kind, quads)) = &f.markup {
            // Each box, wherever the page's rotation put its corners: the box that holds them. All of an annotation's boxes are one stroke (two
            // opposite corners for each, as a markup made in the program has them), so that it is saved as the one annotation it was.
            let mut pts: Vec<P> = Vec::new();
            for q in quads {
                let corners = [[q[0], q[1]], [q[2], q[3]], [q[4], q[5]], [q[6], q[7]]].map(|c| to_doc([c[0] as f32, c[1] as f32]));
                let lo = corners.iter().fold([f32::MAX; 2], |m, c| [m[0].min(c[0]), m[1].min(c[1])]);
                let hi = corners.iter().fold([f32::MIN; 2], |m, c| [m[0].max(c[0]), m[1].max(c[1])]);
                pts.extend([lo, hi]);
            }
            strokes.push((f.page as u32, f.style, pts, None, None, Some(*kind), None));
            continue;
        }
        for pts in &f.strokes {
            let doc_pts: Vec<P> = pts.iter().map(|&p| to_doc(p)).collect();
            // A shape that reads back is drawn from its anchors, not from the points as rounded in the file.
            let figure = f.figure.as_deref().filter(|_| f.strokes.len() == 1).and_then(|text| Figure::decode(text, to_doc));
            let (pts, figure) = match figure {
                Some(figure) => (figure.points(), Some(figure)),
                None => (doc_pts, None),
            };
            strokes.push((f.page as u32, f.style, pts, figure, None, None, None));
        }
    }
    // The page renderer must not also draw them: delete from the highest index down so the
    // earlier ones keep their positions.
    for f in found.iter().rev() {
        doc.delete_annotation(f.page, f.index)?;
    }
    let render_bytes = doc.save_full()?;
    Ok(Loaded { strokes, render_bytes: Arc::new(render_bytes), links, form, outline, permissions })
}

/// Put the pages of the file that are left (the deleted ones are out of it by now) in the order the document has them:
/// `order` is, for each page of the document, the page of the file it was, or `None` if it was added (it is put in
/// afterwards). The pages that are already in order are not touched.
fn arrange(doc: &mut dyn PdfDocument, doc_of_file: &[Option<usize>], order: &[Option<usize>]) -> Result<(), String> {
    let mut present: Vec<usize> = (0..doc_of_file.len()).filter(|&f| doc_of_file[f].is_some()).collect();
    let wanted: Vec<usize> = order.iter().flatten().copied().collect();
    for (at, &file_page) in wanted.iter().enumerate() {
        let Some(now) = present.iter().position(|&f| f == file_page) else { continue };
        if now != at {
            doc.move_pages(&[now], at)?;
            present.remove(now);
            present.insert(at, file_page);
        }
    }
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The boxes of a text markup waiting to be written as one annotation.
struct Marks {
    kind: MarkupKind,
    style: Style,
    /// In the page's user space.
    quads: Vec<[f64; 8]>,
}

/// Write the markup that was gathered (if any) as an annotation of its own.
fn finish_markup(
    doc: &mut dyn PdfDocument,
    marks: &mut Option<Marks>,
    page: u32,
    serial: &std::cell::Cell<u32>,
    stamp: u128,
    now: &str,
) -> Result<(), String> {
    let Some(m) = marks.take() else { return Ok(()) };
    if m.quads.is_empty() {
        return Ok(());
    }
    serial.set(serial.get() + 1);
    let c = m.style.rgba;
    doc.add_annotation(&NewAnnotation {
        page: page as usize,
        shape: NewShape::Markup { kind: m.kind, quads: m.quads },
        colour: [f64::from(c[0]) / 255.0, f64::from(c[1]) / 255.0, f64::from(c[2]) / 255.0],
        opacity: f64::from(c[3]) / 255.0,
        width: 0.0,
        text: String::new(),
        author: "InkPDF".into(),
        name: format!("{NAME_PREFIX}{stamp:x}-{}", serial.get()),
        date: now.to_string(),
        replies: Vec::new(),
    })
}

/// The file with `store`'s ink written into the pages in `pages` (as the document numbers them),
/// as an update appended to `base`. `doc_of_file` says, for each page of the file (from the first, as many as the
/// document can show), which page of the document it now is, or `None` if it was deleted (and is to be left out of the
/// saved file); a page of the document that no page of the file is, is one that was added (it is put in, as big as
/// `layout` says), and the pages of the file are put in the order the document has them. `copies` are the pages of the
/// document that are copies of pages of the file, each with the page of the file it is a copy of: they are made from them.
#[cfg(test)]
pub fn save(
    base: &Arc<Vec<u8>>,
    store: &InkStore,
    geoms: &[PageGeom],
    layout: &Layout,
    pages: &BTreeSet<u32>,
    doc_of_file: &[Option<usize>],
    copies: &[(usize, usize)],
) -> Result<Vec<u8>, String> {
    save_with(pdf::native(), base, store, geoms, layout, pages, doc_of_file, copies, &Extras::default())
}

/// What a save writes besides the ink and the pages.
#[derive(Default)]
pub struct Extras<'a> {
    /// The pages of the document that are flagged (see [`crate::outline::FLAG`]): the flags of the file are made to be those. `None`: left as they are.
    pub flags: Option<&'a [usize]>,
    /// The values the person put in the fields of the form (see [`crate::form`]).
    pub fields: &'a [(String, pdf::RawValue)],
}

/// [`save`], and with `extras`.
#[allow(clippy::too_many_arguments, reason = "the same arguments as `save`, each a different part of what the document is, and one more")]
pub fn save_with(
    engine: &dyn PdfBackend,
    base: &Arc<Vec<u8>>,
    store: &InkStore,
    geoms: &[PageGeom],
    layout: &Layout,
    pages: &BTreeSet<u32>,
    doc_of_file: &[Option<usize>],
    copies: &[(usize, usize)],
    extras: &Extras<'_>,
) -> Result<Vec<u8>, String> {
    let mut doc = engine.open(base)?;
    let now = engine.date(unix_now());
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let serial = std::cell::Cell::new(0_u32);

    // Delete our previous annotations on the changed pages, highest index first, while the pages
    // are still numbered as the file has them (a page deleted needs none: it goes, with them).
    let mut old = find_ours(&*doc);
    old.retain(|f| doc_of_file.get(f.page).copied().flatten().is_some_and(|page| pages.contains(&(page as u32))));
    for f in old.iter().rev() {
        doc.delete_annotation(f.page, f.index)?;
    }
    // Copies of pages come first, as pages at the end of the file (so that a page that was copied and then deleted still has
    // its copy): ink this app saved on the page they are copies of is not theirs, and goes.
    let originals = doc_of_file.len();
    for &(_, source) in copies {
        let end = doc.page_count()?;
        doc.duplicate_pages(&[source], Some(end))?;
    }
    if !copies.is_empty() {
        let mut stale = find_ours(&*doc);
        stale.retain(|f| f.page >= originals);
        for f in stale.iter().rev() {
            doc.delete_annotation(f.page, f.index)?;
        }
    }
    let mut of_file: Vec<Option<usize>> = doc_of_file.to_vec();
    of_file.extend(copies.iter().map(|&(page, _)| Some(page)));
    let doc_of_file = &of_file[..];
    // Then the pages that were deleted go, and the pages that were added are put in (each at its place in the
    // finished document: every page before it is there by then, as the pages go in from the front).
    let deleted: Vec<usize> = doc_of_file.iter().enumerate().filter(|(_, at)| at.is_none()).map(|(file_page, _)| file_page).collect();
    // For each page of the document, the page of the file it was (none: it was added).
    let mut order: Vec<Option<usize>> = vec![None; layout.len()];
    for (file_page, at) in doc_of_file.iter().enumerate() {
        if let Some(slot) = at.and_then(|at| order.get_mut(at)) {
            *slot = Some(file_page);
        }
    }
    let added: Vec<(usize, [f32; 2])> =
        order.iter().enumerate().filter(|(_, from)| from.is_none()).filter_map(|(at, _)| layout.page(at).map(|r| (at, [r.w, r.h]))).collect();
    let mut inserts = &added[..];
    if !deleted.is_empty() {
        match doc.delete_pages(&deleted) {
            Ok(()) => {}
            // Every page of the file is going: a file cannot have none, so the first page added goes in first.
            Err(OrganizeError::WouldRemoveAllPages) => {
                let (&(_, size), rest) = inserts.split_first().ok_or("the document would have no pages")?;
                doc.insert_blank_page(deleted.len(), size)?;
                doc.delete_pages(&deleted).map_err(|e| e.to_string())?;
                inserts = rest;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    arrange(&mut *doc, doc_of_file, &order)?;
    for &(at, size) in inserts {
        doc.insert_blank_page(at, size)?;
    }
    // (The pages are in their places: the flags are the document's numbers.)
    if let Some(flags) = extras.flags {
        doc.set_flags(crate::outline::FLAG, flags)?;
    }
    if !extras.fields.is_empty() {
        doc.set_fields(extras.fields)?;
    }

    for &page in pages {
        let (Some(geom), Some(rect)) = (geoms.get(page as usize), layout.page(page as usize)) else { continue };
        let mut marks: Option<Marks> = None;
        let mut run: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut run_style: Option<Style> = None;
        let flush = |doc: &mut dyn PdfDocument,
                     run: &mut Vec<Vec<[f64; 2]>>,
                     style: Option<Style>,
                     figure: Option<&str>,
                     text: Option<(&TextBlock, [f64; 4])>|
         -> Result<(), String> {
            let Some(style) = style else { return Ok(()) };
            // Typed text is a FreeText annotation (so any reader shows it as text); the rest is ink.
            let (shape, text, width) = match text {
                Some((block, rect)) => (NewShape::Text { rect, font_size: f64::from(block.size) }, block.text.clone(), 0.0),
                None if run.is_empty() => return Ok(()),
                None => (NewShape::Ink { strokes: std::mem::take(run) }, String::new(), f64::from(style.width)),
            };
            let c = style.rgba;
            serial.set(serial.get() + 1);
            let tag = figure.map_or_else(String::new, |f| format!("{FIGURE_MARK}{f}"));
            let new = NewAnnotation {
                page: page as usize,
                shape,
                colour: [f64::from(c[0]) / 255.0, f64::from(c[1]) / 255.0, f64::from(c[2]) / 255.0],
                opacity: f64::from(c[3]) / 255.0,
                width,
                text,
                author: "InkPDF".into(),
                name: format!("{NAME_PREFIX}{stamp:x}-{}{tag}", serial.get()),
                date: now.clone(),
                replies: Vec::new(),
            };
            doc.add_annotation(&new)
        };
        for s in store.strokes().iter().filter(|s| s.page == page) {
            if let Some(item) = &s.item {
                flush(&mut *doc, &mut run, run_style, None, None)?;
                run_style = None;
                finish_markup(&mut *doc, &mut marks, page, &serial, stamp, &now)?;
                // (An item of a kind this build does not have was never loaded, so it cannot be here, but one made by a test might be.)
                let Some(kind) = items::kind(&item.kind) else { continue };
                let note = kind.write(&item.data);
                // The note's top-left corner, as it looks on the page, in the page's user space.
                let corner = s.quads().next().map_or([0.0, 0.0], |q| [q[0], q[1]]);
                let u = geom.view_to_user([corner[0] - rect.x, corner[1] - rect.y]);
                serial.set(serial.get() + 1);
                let c = s.style.rgba;
                let said = |date: &str| if date.is_empty() { now.clone() } else { date.to_string() };
                let author = if note.author.is_empty() { "InkPDF".to_string() } else { note.author.clone() };
                doc.add_annotation(&NewAnnotation {
                    page: page as usize,
                    shape: NewShape::Note { at: [f64::from(u[0]), f64::from(u[1])] },
                    colour: [f64::from(c[0]) / 255.0, f64::from(c[1]) / 255.0, f64::from(c[2]) / 255.0],
                    opacity: f64::from(c[3]) / 255.0,
                    width: 0.0,
                    text: note.text,
                    author: author.clone(),
                    name: format!("{ITEM_MARK}{}:{stamp:x}-{}", item.kind, serial.get()),
                    date: said(&note.date),
                    replies: note
                        .replies
                        .into_iter()
                        .map(|r| Reply { author: r.author, date: said(&r.date), text: r.text, state: None })
                        .chain(note.closed.then(|| Reply {
                            author: author.clone(),
                            date: now.clone(),
                            text: String::new(),
                            state: Some("Completed".into()),
                        }))
                        .collect(),
                })?;
                continue;
            }
            if let Some(kind) = s.markup {
                flush(&mut *doc, &mut run, run_style, None, None)?;
                run_style = None;
                finish_markup(&mut *doc, &mut marks, page, &serial, stamp, &now)?;
                // One annotation for the markup: each box as its four corners as they look on the page (top-left, top-right,
                // bottom-left, bottom-right), put into the page's user space.
                let user = |x: f32, y: f32| {
                    let u = geom.view_to_user([x - rect.x, y - rect.y]);
                    [f64::from(u[0]), f64::from(u[1])]
                };
                let quads: Vec<[f64; 8]> = s
                    .quads()
                    .map(|q| {
                        let (tl, tr, bl, br) = (user(q[0], q[1]), user(q[2], q[1]), user(q[0], q[3]), user(q[2], q[3]));
                        [tl[0], tl[1], tr[0], tr[1], bl[0], bl[1], br[0], br[1]]
                    })
                    .collect();
                // (The boxes of one selection are one annotation, as a reader's own would be; two selections are two, so that each can be taken away by itself in any
                // reader, even if they are of one kind and colour and follow one another.)
                marks.get_or_insert_with(|| Marks { kind, style: s.style, quads: Vec::new() }).quads.extend(quads);
                finish_markup(&mut *doc, &mut marks, page, &serial, stamp, &now)?;
                continue;
            }
            finish_markup(&mut *doc, &mut marks, page, &serial, stamp, &now)?;
            if let Some(block) = &s.text {
                flush(&mut *doc, &mut run, run_style, None, None)?;
                run_style = None;
                let corners = s.pts.iter().map(|p| geom.view_to_user([p[0] - rect.x, p[1] - rect.y])).map(|u| [f64::from(u[0]), f64::from(u[1])]);
                let r =
                    corners.fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |r, c| [r[0].min(c[0]), r[1].min(c[1]), r[2].max(c[0]), r[3].max(c[1])]);
                flush(&mut *doc, &mut Vec::new(), Some(s.style), None, Some((block, r)))?;
                continue;
            }
            if run_style != Some(s.style) || run.len() >= STROKES_PER_ANNOTATION || s.figure.is_some() {
                flush(&mut *doc, &mut run, run_style, None, None)?;
                run_style = Some(s.style);
            }
            let mut user: Vec<[f64; 2]> = s
                .pts
                .iter()
                .map(|p| {
                    let u = geom.view_to_user([p[0] - rect.x, p[1] - rect.y]);
                    [f64::from(u[0]), f64::from(u[1])]
                })
                .collect();
            // A dot is a stroke of one point; give it a sliver of length so every reader draws it.
            if user.len() == 1 {
                user.push([user[0][0] + 0.01, user[0][1]]);
            }
            run.push(user);
            // A shape has an annotation to itself, which says what it was made from.
            if let Some(figure) = &s.figure {
                let text = figure.encode(|p| {
                    let u = geom.view_to_user([p[0] - rect.x, p[1] - rect.y]);
                    [u[0], u[1]]
                });
                flush(&mut *doc, &mut run, run_style, Some(&text), None)?;
                run_style = None;
            }
        }
        flush(&mut *doc, &mut run, run_style, None, None)?;
        finish_markup(&mut *doc, &mut marks, page, &serial, stamp, &now)?;
    }
    doc.save_incremental(&now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::sample_pdf;

    fn setup() -> (Arc<Vec<u8>>, Vec<PageGeom>, Layout) {
        let geom = PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 0 };
        (Arc::new(sample_pdf()), vec![geom], Layout::new(&[[612.0, 792.0]]))
    }

    const RED: Style = Style { rgba: [255, 0, 0, 255], width: 2.0 };
    const BLUE: Style = Style { rgba: [0, 0, 255, 255], width: 3.0 };

    fn page_origin(layout: &Layout) -> [f32; 2] {
        let r = layout.page(0).expect("page");
        [r.x, r.y]
    }

    fn store_with(layout: &Layout) -> InkStore {
        let o = page_origin(layout);
        let mut store = InkStore::new();
        store.add(0, RED, vec![[o[0] + 100.0, o[1] + 100.0], [o[0] + 150.0, o[1] + 120.0], [o[0] + 200.0, o[1] + 100.0]]);
        store.add(0, RED, vec![[o[0] + 10.0, o[1] + 10.0], [o[0] + 20.0, o[1] + 10.0]]);
        store.add(0, BLUE, vec![[o[0] + 300.0, o[1] + 400.0]]);
        store
    }

    #[test]
    fn a_file_without_our_ink_loads_empty_and_renders_unchanged() {
        let (bytes, geoms, layout) = setup();
        let loaded = load(&bytes, &geoms, &layout).expect("load");
        assert!(loaded.strokes.is_empty());
        assert!(Arc::ptr_eq(&loaded.render_bytes, &bytes), "no copy when there is nothing to hide");
    }

    #[test]
    fn saving_appends_to_the_original_and_loading_returns_the_same_strokes() {
        let (bytes, geoms, layout) = setup();
        let store = store_with(&layout);
        let pages: BTreeSet<u32> = [0].into();
        let saved = save(&bytes, &store, &geoms, &layout, &pages, &[Some(0)], &[]).expect("save");
        assert_eq!(&saved[..bytes.len()], &bytes[..], "the original bytes are kept as the start of the file");
        assert!(saved.len() > bytes.len());

        let saved = Arc::new(saved);
        let loaded = load(&saved, &geoms, &layout).expect("load");
        assert_eq!(loaded.strokes.len(), 3);
        let o = page_origin(&layout);
        let (page, style, pts, ..) = &loaded.strokes[0];
        assert_eq!((*page, *style), (0, RED));
        assert_eq!(pts.len(), 3);
        for (got, want) in pts.iter().zip([[100.0, 100.0], [150.0, 120.0], [200.0, 100.0]]) {
            assert!((got[0] - (o[0] + want[0])).abs() < 1e-2 && (got[1] - (o[1] + want[1])).abs() < 1e-2, "{got:?} vs {want:?}");
        }
        assert_eq!(loaded.strokes[2].1, BLUE);
        // The dot came back as a near-zero-length stroke at the same place.
        let dot = &loaded.strokes[2].2;
        assert!((dot[0][0] - (o[0] + 300.0)).abs() < 1e-2 && (dot[0][1] - (o[1] + 400.0)).abs() < 1e-2);
    }

    #[test]
    fn a_markup_is_saved_as_a_markup_annotation_and_read_back_as_ours_and_hidden_from_the_renderer() {
        for rotation in [0, 90] {
            let (bytes, mut geoms, _) = setup();
            geoms[0].rotation = rotation;
            let layout = Layout::new(&[if rotation == 90 { [792.0, 612.0] } else { [612.0, 792.0] }]);
            let o = page_origin(&layout);
            let mark = Style { rgba: [255, 235, 0, 110], width: 1.0 };
            let boxes = [[o[0] + 100.0, o[1] + 100.0, o[0] + 200.0, o[1] + 114.0], [o[0] + 100.0, o[1] + 130.0, o[0] + 160.0, o[1] + 144.0]];
            let mut store = InkStore::new();
            store.add_markup(0, mark, MarkupKind::Highlight, &boxes);
            store.add_markup(0, Style { rgba: [32, 96, 224, 255], ..mark }, MarkupKind::Squiggly, &boxes[..1]);
            let saved = Arc::new(save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
            let found = find_ours(&*pdf::native().open(&saved).expect("open"));
            assert_eq!(
                found.iter().filter_map(|f| f.markup.as_ref().map(|(k, q)| (*k, q.len()))).collect::<Vec<_>>(),
                [(MarkupKind::Highlight, 2), (MarkupKind::Squiggly, 1)]
            );
            let loaded = load(&saved, &geoms, &layout).expect("load");
            assert_eq!(loaded.strokes.len(), 2, "an annotation is a stroke: the highlight of two boxes, and the wavy line");
            for ((page, style, pts, _, _, kind, _), (want_kind, want_boxes, want_style)) in loaded
                .strokes
                .iter()
                .zip([(MarkupKind::Highlight, &boxes[..], mark), (MarkupKind::Squiggly, &boxes[..1], Style { rgba: [32, 96, 224, 255], ..mark })])
            {
                assert_eq!((*page, kind.as_ref()), (0, Some(&want_kind)));
                assert_eq!((style.rgba[0], style.rgba[3]), (want_style.rgba[0], want_style.rgba[3]), "its colour and opacity come back");
                assert_eq!(pts.len(), 2 * want_boxes.len(), "two corners for each box");
                for (b, want_box) in want_boxes.iter().enumerate() {
                    for (got, want) in [pts[2 * b][0], pts[2 * b][1], pts[2 * b + 1][0], pts[2 * b + 1][1]].iter().zip(want_box) {
                        assert!((got - want).abs() < 0.05, "rotation {rotation}: {pts:?} vs {want_box:?}");
                    }
                }
            }
            // The renderer does not draw them as well (the ink layer does).
            assert!(find_ours(&*pdf::native().open(&loaded.render_bytes).expect("open")).is_empty());
            // And saving again leaves one set, not two.
            let mut again = InkStore::new();
            again.load(loaded.strokes);
            let twice = Arc::new(save(&saved, &again, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save again"));
            assert_eq!(find_ours(&*pdf::native().open(&twice).expect("open")).len(), 2);
        }
    }

    #[test]
    fn two_selections_of_one_kind_and_colour_are_two_annotations_and_stay_two_when_the_file_is_opened_and_saved_again() {
        let (bytes, geoms, _) = setup();
        let layout = Layout::new(&[[612.0, 792.0]]);
        let o = page_origin(&layout);
        let mark = Style { rgba: [255, 235, 0, 110], width: 1.0 };
        let (one, two) = (
            [[o[0] + 100.0, o[1] + 100.0, o[0] + 200.0, o[1] + 114.0]],
            [[o[0] + 300.0, o[1] + 400.0, o[0] + 360.0, o[1] + 414.0], [o[0] + 300.0, o[1] + 416.0, o[0] + 340.0, o[1] + 430.0]],
        );
        let mut store = InkStore::new();
        store.add_markup(0, mark, MarkupKind::Highlight, &one);
        store.add_markup(0, mark, MarkupKind::Highlight, &two);
        let counts = |bytes: &Arc<Vec<u8>>| -> Vec<usize> {
            find_ours(&*pdf::native().open(bytes).expect("open")).iter().filter_map(|f| f.markup.as_ref().map(|(_, q)| q.len())).collect()
        };
        let saved = Arc::new(save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        assert_eq!(counts(&saved), [1, 2], "one annotation each, the second with its two lines");
        let loaded = load(&saved, &geoms, &layout).expect("load");
        let mut again = InkStore::new();
        again.load(loaded.strokes);
        let twice = Arc::new(save(&saved, &again, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save again"));
        assert_eq!(counts(&twice), [1, 2], "and after a reopen and a save the same");
    }
    #[test]
    fn an_item_is_saved_as_a_note_with_its_replies_and_read_back_as_the_item_in_the_same_place_on_every_rotation() {
        for rotation in [0, 90, 270] {
            let (bytes, mut geoms, _) = setup();
            geoms[0].rotation = rotation;
            let layout = Layout::new(&[if rotation == 0 { [612.0, 792.0] } else { [792.0, 612.0] }]);
            let o = page_origin(&layout);
            let mut store = InkStore::new();
            let bounds = [o[0] + 100.0, o[1] + 120.0, o[0] + 120.0, o[1] + 140.0];
            store.add_item(0, Style { rgba: [255, 204, 0, 255], width: 1.0 }, "testbox", "first\nsecond\nthird".into(), bounds);
            let saved = Arc::new(save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
            // In the file it is a note (text annotation) with two replies, which any reader shows.
            let notes: Vec<_> = pdf::native().open(&saved).expect("open").annotations().into_iter().filter(|a| a.kind == AnnotKind::Note).collect();
            assert_eq!(notes.len(), 1, "rotation {rotation}");
            assert_eq!((notes[0].text.as_str(), notes[0].replies.len()), ("first", 2));
            assert!(notes[0].name.starts_with("inkpdf-item:testbox:"), "{}", notes[0].name);
            let loaded = load(&saved, &geoms, &layout).expect("load");
            assert_eq!(loaded.strokes.len(), 1, "rotation {rotation}");
            let (page, style, pts, _, _, _, item) = &loaded.strokes[0];
            assert_eq!(*page, 0);
            assert_eq!(style.rgba[..3], [255, 204, 0], "its colour comes back");
            assert_eq!(item.as_ref().map(|i| (i.kind.as_str(), i.data.as_str())), Some(("testbox", "first\nsecond\nthird")));
            // (A note's box in the file is its own size, so its corner is what has to be where it was: the top left, as the page shows it.)
            assert_eq!(pts.len(), 2);
            let top_left = [pts[0][0].min(pts[1][0]), pts[0][1].min(pts[1][1])];
            let want = [bounds[0], bounds[1]];
            assert!(rotation != 0 || ((top_left[0] - want[0]).abs() < 0.1 && (top_left[1] - want[1]).abs() < 0.1), "{top_left:?} vs {want:?}");
            // The renderer does not draw it as well (the ink layer does), and saving again leaves one note, not two.
            assert!(find_ours(&*pdf::native().open(&loaded.render_bytes).expect("open")).is_empty(), "rotation {rotation}");
            let mut again = InkStore::new();
            again.load(loaded.strokes);
            let twice = Arc::new(save(&saved, &again, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save again"));
            let notes = pdf::native().open(&twice).expect("open").annotations().into_iter().filter(|a| a.kind == AnnotKind::Note).count();
            assert_eq!(notes, 1, "rotation {rotation}");
        }
    }

    #[test]
    fn a_note_of_another_program_or_of_a_kind_this_build_has_not_is_left_in_the_file_and_not_loaded() {
        let (bytes, geoms, layout) = setup();
        let mut doc = pdf::native().open(&bytes).expect("open");
        let note = |name: &str| NewAnnotation {
            page: 0,
            shape: NewShape::Note { at: [100.0, 700.0] },
            colour: [1.0, 1.0, 0.0],
            opacity: 1.0,
            width: 0.0,
            text: "from elsewhere".into(),
            author: "someone".into(),
            name: name.into(),
            date: "D:20260101000000Z".into(),
            replies: Vec::new(),
        };
        doc.add_annotation(&note("their-note")).expect("add");
        doc.add_annotation(&note("inkpdf-item:no-such-kind:1-1")).expect("add");
        let theirs = Arc::new(doc.save_incremental("D:20260101000000Z").expect("update"));
        let loaded = load(&theirs, &geoms, &layout).expect("load");
        assert!(loaded.strokes.is_empty(), "neither is ours to edit");
        assert!(Arc::ptr_eq(&loaded.render_bytes, &theirs), "and the renderer keeps drawing both");
        // Saving the page leaves them where they are.
        let saved = Arc::new(save(&theirs, &InkStore::new(), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        let kept = pdf::native().open(&saved).expect("open").annotations().into_iter().filter(|a| a.kind == AnnotKind::Note).count();
        assert_eq!(kept, 2);
    }

    #[test]
    fn same_style_strokes_share_an_annotation_and_a_new_style_starts_another() {
        let (bytes, geoms, layout) = setup();
        let saved = save(&bytes, &store_with(&layout), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save");
        let doc = pdf::native().open(&Arc::new(saved)).expect("open");
        let found = find_ours(&*doc);
        // Two red strokes in one annotation, one blue dot in another.
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].strokes.len(), 2);
        assert_eq!(found[1].strokes.len(), 1);
    }

    #[test]
    fn resaving_replaces_our_annotations_instead_of_piling_up() {
        let (bytes, geoms, layout) = setup();
        let first = Arc::new(save(&bytes, &store_with(&layout), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("first save"));
        // Erase everything and save again on top of the first save.
        let empty = InkStore::new();
        let second = save(&first, &empty, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("second save");
        let doc = pdf::native().open(&Arc::new(second)).expect("open");
        assert!(find_ours(&*doc).is_empty(), "the old annotations were deleted");
    }

    #[test]
    fn the_render_copy_has_none_of_our_annotations() {
        let (bytes, geoms, layout) = setup();
        let saved = Arc::new(save(&bytes, &store_with(&layout), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        let loaded = load(&saved, &geoms, &layout).expect("load");
        assert!(!Arc::ptr_eq(&loaded.render_bytes, &saved));
        let doc = pdf::native().open(&loaded.render_bytes.clone()).expect("open the render copy");
        assert!(find_ours(&*doc).is_empty());
        // And the page still renders.
        assert_eq!(doc.page_count(), Ok(1));
        let page = pdf::native().render_now(loaded.render_bytes.clone(), &pdf::RenderRequest::default());
        assert!(page.error.is_none() && page.width > 0, "{:?}", page.error);
    }

    #[test]
    fn pages_not_listed_are_left_alone() {
        let (bytes, geoms, layout) = setup();
        let first = Arc::new(save(&bytes, &store_with(&layout), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        // Saving with no changed pages writes no new annotations and deletes none.
        let again = save(&first, &InkStore::new(), &geoms, &layout, &BTreeSet::new(), &[Some(0)], &[]).expect("save");
        let doc = pdf::native().open(&Arc::new(again)).expect("open");
        assert_eq!(find_ours(&*doc).len(), 2);
    }

    #[test]
    fn long_runs_of_one_style_are_split() {
        let (bytes, geoms, layout) = setup();
        let o = page_origin(&layout);
        let mut store = InkStore::new();
        for i in 0..(STROKES_PER_ANNOTATION * 2 + 5) {
            store.add(0, RED, vec![[o[0] + i as f32, o[1]], [o[0] + i as f32 + 1.0, o[1] + 1.0]]);
        }
        let saved = save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save");
        let found = find_ours(&*pdf::native().open(&Arc::new(saved)).expect("open"));
        assert_eq!(found.len(), 3);
        assert_eq!(found.iter().map(|f| f.strokes.len()).sum::<usize>(), STROKES_PER_ANNOTATION * 2 + 5);
    }

    fn text_store(layout: &Layout, words: &str, size: f32) -> InkStore {
        let o = page_origin(layout);
        let mut store = store_with(layout);
        let top_left = [o[0] + 100.0, o[1] + 200.0];
        store.add_text(
            0,
            Style { rgba: [200, 20, 20, 255], width: 0.0 },
            TextBlock { text: words.into(), size },
            crate::text::outline(top_left, words, size),
        );
        store
    }

    fn loaded_texts(loaded: &Loaded) -> Vec<(String, f32, Vec<[f32; 2]>)> {
        loaded.strokes.iter().filter_map(|(_, _, pts, _, t, _, _)| t.as_ref().map(|t| (t.text.clone(), t.size, pts.clone()))).collect()
    }

    #[test]
    fn text_is_saved_as_a_free_text_annotation_and_read_back_in_the_same_place() {
        for (words, size) in [("Hello", 24.0), ("Two\nlines of text", 12.0), ("Zwei Ünï — ünïcode ✓", 18.0)] {
            let (bytes, geoms, layout) = setup();
            let saved = Arc::new(save(&bytes, &text_store(&layout, words, size), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
            let doc = pdf::native().open(&saved.clone()).expect("open");
            assert!(find_ours(&*doc).iter().any(|f| f.text.is_some()), "{words:?}: the text is one of our annotations");
            let loaded = load(&saved, &geoms, &layout).expect("load");
            assert_eq!(loaded.strokes.len(), 4, "the three strokes and the text");
            let texts = loaded_texts(&loaded);
            assert_eq!(texts.len(), 1, "{words:?}");
            let (got, got_size, outline) = &texts[0];
            assert_eq!((got.as_str(), *got_size), (crate::text::clean(words).as_str(), size), "{words:?}");
            let o = page_origin(&layout);
            let want = crate::text::outline([o[0] + 100.0, o[1] + 200.0], words, size);
            for (a, b) in outline.iter().zip(&want) {
                assert!((a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05, "{words:?}: {a:?} vs {b:?}");
            }
            let colour = loaded.strokes.iter().find(|s| s.4.is_some()).map(|s| s.1);
            assert_eq!(colour, Some(Style { rgba: [200, 20, 20, 255], width: 0.0 }));
        }
    }

    #[test]
    fn text_on_a_turned_page_comes_back_where_it_was() {
        let geom = PageGeom { width: 612.0, height: 792.0, crop: [0.0, 0.0, 612.0, 792.0], rotation: 90 };
        let (bytes, layout) = (Arc::new(sample_pdf()), Layout::new(&[[792.0, 612.0]]));
        let geoms = vec![geom];
        let saved = Arc::new(save(&bytes, &text_store(&layout, "Turned", 20.0), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        let loaded = load(&saved, &geoms, &layout).expect("load");
        let texts = loaded_texts(&loaded);
        assert_eq!(texts.len(), 1);
        let o = page_origin(&layout);
        let want = crate::text::outline([o[0] + 100.0, o[1] + 200.0], "Turned", 20.0);
        for (a, b) in texts[0].2.iter().zip(&want) {
            assert!((a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn other_readers_draw_the_saved_text_in_its_colour_where_the_box_was() {
        let (bytes, geoms, layout) = setup();
        // Only the text (no ink), so any red in the box is the text.
        let o = page_origin(&layout);
        let mut store = InkStore::new();
        let (words, size, top_left) = ("Hello", 40.0, [o[0] + 100.0, o[1] + 200.0]);
        store.add_text(
            0,
            Style { rgba: [200, 20, 20, 255], width: 0.0 },
            TextBlock { text: words.into(), size },
            crate::text::outline(top_left, words, size),
        );
        let saved = Arc::new(save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        let page = pdf::native().render_now(saved, &pdf::RenderRequest { page: 0, scale: 1.0, ..pdf::RenderRequest::default() });
        assert!(page.error.is_none(), "{:?}", page.error);
        let (w, rgba) = (page.width as usize, page.rgba.as_slice());
        let [bw, bh] = crate::text::measure(words, size);
        let reddish =
            |x: usize, y: usize| rgba.get((y * w + x) * 4..(y * w + x) * 4 + 4).is_some_and(|p| p[3] > 128 && p[0] > 150 && p[1] < 90 && p[2] < 90);
        let inside =
            (0..bh as usize + 2).flat_map(|dy| (0..bw as usize + 2).map(move |dx| (dx, dy))).filter(|&(dx, dy)| reddish(100 + dx, 200 + dy)).count();
        let outside = (0..60).flat_map(|dy| (0..60).map(move |dx| (dx, dy))).filter(|&(dx, dy)| reddish(300 + dx, 400 + dy)).count();
        assert!(inside > 200, "the text should be drawn in the box ({inside} red pixels)");
        assert_eq!(outside, 0, "and nowhere else");
    }

    #[test]
    fn other_readers_draw_text_upright_and_in_place_on_a_page_turned_in_the_file() {
        for rotate in [0, 90, 180, 270] {
            let objs = [
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
                format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate {rotate} >>"),
            ];
            let bytes = Arc::new(crate::testutil::build_pdf(&objs));
            let geoms: Vec<PageGeom> = pdf::native().inspect(&bytes).expect("inspect");
            let sizes: Vec<[f32; 2]> = geoms.iter().map(|p| [p.width, p.height]).collect();
            let layout = Layout::new(&sizes);
            let o = page_origin(&layout);
            let mut store = InkStore::new();
            let (words, size, top_left) = ("Hello", 40.0, [o[0] + 100.0, o[1] + 200.0]);
            store.add_text(
                0,
                Style { rgba: [200, 20, 20, 255], width: 0.0 },
                TextBlock { text: words.into(), size },
                crate::text::outline(top_left, words, size),
            );
            let saved = Arc::new(save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
            // Read back by this app, the box is where it was put.
            let back = load(&saved, &geoms, &layout).expect("load");
            let texts = loaded_texts(&back);
            assert_eq!(texts.len(), 1, "rotate {rotate}");
            let want = crate::text::outline(top_left, words, size);
            assert!(
                texts[0].2.iter().zip(&want).all(|(a, b)| (a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05),
                "rotate {rotate}: {:?} vs {want:?}",
                texts[0].2
            );
            let page = pdf::native().render_now(saved, &pdf::RenderRequest { page: 0, scale: 1.0, ..pdf::RenderRequest::default() });
            let (w, rgba) = (page.width as usize, page.rgba.as_slice());
            let mut b = [usize::MAX, usize::MAX, 0, 0];
            for (i, p) in rgba.as_chunks::<4>().0.iter().enumerate() {
                if p[3] > 128 && p[0] > 150 && p[1] < 90 {
                    let (x, y) = (i % w, i / w);
                    b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                }
            }
            let [bw, bh] = crate::text::measure(words, size);
            // Where we show the box is where the text is drawn, whichever way the page is turned.
            assert!(b[0] != usize::MAX, "rotate {rotate}: no text was drawn");
            let (x0, y0, x1, y1) = (100.0 - 2.0, 200.0 - 2.0, 100.0 + bw + 2.0, 200.0 + bh + 2.0);
            assert!(
                b[0] as f32 >= x0 && b[1] as f32 >= y0 && b[2] as f32 <= x1 && b[3] as f32 <= y1,
                "rotate {rotate}: drawn at {b:?}, the box is {x0},{y0} to {x1},{y1}"
            );
            assert!(
                (b[2] - b[0]) as f32 > 0.5 * bw && (b[3] - b[1]) as f32 > 0.4 * bh,
                "rotate {rotate}: upright text fills most of its box, not a sideways sliver: {b:?}"
            );
        }
    }

    #[test]
    fn resaving_changed_text_replaces_it() {
        let (bytes, geoms, layout) = setup();
        let first = Arc::new(save(&bytes, &text_store(&layout, "before", 18.0), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        let second = save(&first, &text_store(&layout, "after", 18.0), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save again");
        let loaded = load(&Arc::new(second), &geoms, &layout).expect("load");
        let texts = loaded_texts(&loaded);
        assert_eq!(texts.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), ["after"], "one text box, the new one");
    }

    #[test]
    fn a_damaged_text_annotation_is_skipped_not_fatal() {
        let (bytes, geoms, layout) = setup();
        // No text, and nothing but control characters, are not text boxes.
        for words in ["", "\u{1}\u{2}"] {
            let saved = Arc::new(save(&bytes, &text_store(&layout, words, 18.0), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
            let loaded = load(&saved, &geoms, &layout).expect("load");
            assert!(loaded_texts(&loaded).is_empty(), "{words:?}");
            assert_eq!(loaded.strokes.len(), 3, "the ink is still there");
        }
    }

    /// Not a check: writes a demo PDF for looking at in a viewer. Run with
    /// `INKPDF_DEMO_BASE=in.pdf INKPDF_DEMO_OUT=out.pdf cargo test -p inkpdf write_demo_pdf -- --ignored`.
    /// The ink goes in through the editor as pen events, and out through the real save.
    #[test]
    #[ignore = "writes a file; run by hand"]
    fn write_demo_pdf() {
        use crate::editor::Editor;
        use crate::input::{Device, PenButtons, Phase, PointerEvent, Sample};
        let (Ok(base), Ok(out)) = (std::env::var("INKPDF_DEMO_BASE"), std::env::var("INKPDF_DEMO_OUT")) else { return };
        let bytes = Arc::new(std::fs::read(base).expect("read the base PDF"));
        let geoms: Vec<PageGeom> = pdf::native().inspect(&bytes).expect("inspect");
        let sizes: Vec<[f32; 2]> = geoms.iter().map(|p| [p.width, p.height]).collect();
        let layout = Layout::new(&sizes);

        let mut ed = Editor::reference([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
        ed.load(layout.clone(), vec![]);
        ed.fit_mode = false;
        ed.view.zoom = 1.0;
        ed.pan_px(0.0, 0.0);
        let tip = PenButtons { in_range: true, contact: true, eraser: false, barrel: false };
        let stroke = |ed: &mut Editor, pts: Vec<[f32; 2]>| {
            let ev = |phase, pts: &[[f32; 2]]| PointerEvent {
                device: Device::Pen,
                id: 1,
                phase,
                samples: pts.iter().enumerate().map(|(i, &pos)| Sample { pos, pressure: 0.5, t_ms: 1000.0 + i as f64 * 4.0 }).collect(),
                pen: tip,
                contact_px: 0.0,
            };
            ed.pointer_event(ev(Phase::Down, &pts[..1]));
            ed.pointer_event(ev(Phase::Move, &pts[1..]));
            ed.pointer_event(ev(Phase::Up, &pts[pts.len() - 1..]));
        };
        let on_page = |ed: &Editor, x: f32, y: f32| ed.view.doc_to_screen([12.0 + x, 12.0 + y]);
        // Black, blue, red, green strokes in different thicknesses.
        for (colour, width, y) in [(0, 2.5, 150.0), (1, 5.0, 230.0), (2, 1.2, 310.0), (3, 5.0, 390.0)] {
            ed.styles.pen.colour = colour;
            ed.styles.pen.width = width;
            let wave: Vec<[f32; 2]> = (0..80).map(|i| on_page(&ed, 80.0 + i as f32 * 6.0, y + (i as f32 * 0.35).sin() * 22.0)).collect();
            stroke(&mut ed, wave);
        }
        ed.styles.pen.colour = 2;
        ed.styles.pen.width = 5.0;
        let circle: Vec<[f32; 2]> = (0..=60)
            .map(|i| {
                on_page(
                    &ed,
                    300.0 + 90.0 * (i as f32 / 60.0 * std::f32::consts::TAU).cos(),
                    560.0 + 90.0 * (i as f32 / 60.0 * std::f32::consts::TAU).sin(),
                )
            })
            .collect();
        stroke(&mut ed, circle);
        let dot = on_page(&ed, 480.0, 560.0);
        ed.styles.pen.colour = 0;
        ed.styles.pen.width = 5.0;
        stroke(&mut ed, vec![dot, dot]);
        // And a stroke on page 2.
        let (page2_x, page2_y) = (12.0, 12.0 + 792.0 + 12.0);
        let below: Vec<[f32; 2]> =
            (0..60).map(|i| ed.view.doc_to_screen([page2_x + 100.0 + i as f32 * 6.0, page2_y + 120.0 + (i as f32 * 0.4).cos() * 30.0])).collect();
        ed.scroll_by_px(0.0, -400.0);
        for _ in 0..200 {
            ed.step_view(0.016);
        }
        let shift = ed.view.scroll[1];
        let below: Vec<[f32; 2]> = below.iter().map(|p| [p[0], p[1] - shift]).collect();
        stroke(&mut ed, below);
        assert!(ed.ink.len() >= 6, "{} strokes", ed.ink.len());

        let saved = save(&bytes, &ed.ink, &geoms, &layout, ed.ink.dirty_pages(), &[Some(0)], &[]).expect("save");
        std::fs::write(&out, &saved).expect("write the demo");
        println!("wrote {out}: {} strokes, {} -> {} bytes", ed.ink.len(), bytes.len(), saved.len());
    }

    #[test]
    fn foreign_annotations_are_not_loaded_as_ours() {
        // An Ink annotation with some other name is not ours: it must survive a save untouched.
        let (bytes, geoms, layout) = setup();
        let mut doc = pdf::native().open(&bytes).expect("open");
        let new = NewAnnotation {
            page: 0,
            shape: NewShape::Ink { strokes: vec![vec![[10.0, 10.0], [50.0, 50.0]]] },
            colour: [0.0, 1.0, 0.0],
            opacity: 1.0,
            width: 2.0,
            text: String::new(),
            author: "someone else".into(),
            name: "other-1".into(),
            date: "D:20260101000000Z".into(),
            replies: Vec::new(),
        };
        doc.add_annotation(&new).expect("add");
        let with_foreign = Arc::new(doc.save_incremental("D:20260101000000Z").expect("write"));
        assert!(load(&with_foreign, &geoms, &layout).expect("load").strokes.is_empty());
        let saved = save(&with_foreign, &store_with(&layout), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save");
        let reopened = pdf::native().open(&Arc::new(saved)).expect("open");
        let ink_annots = reopened.annotations().into_iter().filter(|a| a.kind == AnnotKind::Ink).count();
        assert_eq!(ink_annots, 3, "the foreign one plus our two");
    }

    #[test]
    fn shapes_come_back_as_shapes_of_their_own_kind_with_their_anchors_and_handwriting_stays_plain() {
        use crate::figure::{Control, Figure};
        use crate::geometry::Shape;
        let (bytes, geoms, layout) = setup();
        let o = page_origin(&layout);
        let at = |x: f32, y: f32| [o[0] + x, o[1] + y];
        let mut rect = Figure::from_shape(Shape::Rectangle, at(100.0, 100.0), at(250.0, 180.0)).expect("rect");
        rect.move_control(Control::Anchor(1), at(300.0, 230.0), false);
        let ellipse = Figure::from_shape(Shape::Ellipse, at(50.0, 300.0), at(200.0, 380.0)).expect("ellipse");
        let mut curve = Figure::from_shape(Shape::Curve, at(50.0, 400.0), at(200.0, 480.0)).expect("curve");
        curve.move_control(Control::Fwd(0), at(260.0, 380.0), true);
        let axes = Figure::from_shape(Shape::Axes3d, at(300.0, 300.0), at(500.0, 450.0)).expect("axes");
        let broken = Figure::Poly { points: vec![at(300.0, 500.0), at(350.0, 540.0), at(400.0, 500.0)], closed: false };
        let polygon = Figure::Poly { points: vec![at(10.0, 600.0), at(60.0, 640.0), at(110.0, 600.0)], closed: true };
        let figures = [&rect, &ellipse, &curve, &axes, &broken, &polygon];
        let mut store = InkStore::new();
        store.add(0, RED, vec![at(10.0, 10.0), at(40.0, 20.0)]);
        for f in figures {
            store.add_figure(0, BLUE, f.clone());
        }
        store.add(0, BLUE, vec![at(500.0, 500.0), at(520.0, 540.0)]);
        let saved = Arc::new(save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save"));
        let loaded = load(&saved, &geoms, &layout).expect("load");
        let kinds: Vec<bool> = loaded.strokes.iter().map(|s| s.3.is_some()).collect();
        assert_eq!(kinds, [false, true, true, true, true, true, true, false], "order and kind are kept");
        for ((_, _, pts, figure, _, _, _), want) in loaded.strokes.iter().filter(|s| s.3.is_some()).zip(figures) {
            let figure = figure.as_ref().expect("shape");
            assert_eq!(std::mem::discriminant(figure), std::mem::discriminant(want), "{want:?} came back as {figure:?}");
            assert_eq!(figure.anchor_count(), want.anchor_count());
            assert!(
                (0..want.anchor_count()).all(|i| {
                    let (a, b) = (want.anchor_pos(i).expect("a"), figure.anchor_pos(i).expect("b"));
                    (a[0] - b[0]).abs() < 1e-2 && (a[1] - b[1]).abs() < 1e-2
                }),
                "anchors of {want:?}"
            );
            assert_eq!(pts, &figure.points(), "the points are what the figure draws");
        }
        // A handle that was broken off stays broken off; a polygon stays closed and a broken line open.
        let Some(Figure::Path { anchors, .. }) = loaded.strokes[3].3.clone() else { panic!("a spline") };
        assert!(!anchors[0].smooth);
        assert!(matches!(loaded.strokes[5].3, Some(Figure::Poly { closed: false, .. })));
        assert!(matches!(loaded.strokes[6].3, Some(Figure::Poly { closed: true, .. })));
        // The store takes them as they are, and they edit again.
        let mut again = InkStore::new();
        again.load(loaded.strokes);
        assert_eq!(again.strokes().iter().filter(|s| s.figure.is_some()).count(), 6);
        let id = again.strokes()[1].id;
        let mut moved = again.get(id).expect("rect").figure.clone().expect("figure");
        moved.move_control(Control::Anchor(0), at(80.0, 90.0), false);
        assert!(again.reshape(id, moved));
        // And an annotation that is not one of ours is still left alone.
        assert_eq!(find_ours(&*pdf::native().open(&saved).expect("open")).len(), 8, "a shape has an annotation of its own");
    }
    #[test]
    fn a_shape_whose_description_is_damaged_loads_as_plain_ink() {
        use crate::geometry::Shape;
        let (bytes, geoms, layout) = setup();
        let o = page_origin(&layout);
        let mut store = InkStore::new();
        store.add_figure(
            0,
            RED,
            crate::figure::Figure::from_shape(Shape::Rectangle, [o[0] + 100.0, o[1] + 100.0], [o[0] + 250.0, o[1] + 180.0]).expect("rect"),
        );
        let saved = save(&bytes, &store, &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save");
        let at = saved.windows(3).position(|w| w == b"~r;").expect("the description is in the file");
        let mut damaged = saved.clone();
        damaged[at + 1] = b'z';
        let loaded = load(&Arc::new(damaged), &geoms, &layout).expect("load");
        assert_eq!(loaded.strokes.len(), 1);
        assert!(loaded.strokes[0].3.is_none(), "not a shape any more, but the ink is still there");
        assert_eq!(loaded.strokes[0].2.len(), 5);
        // An annotation another program wrote, with no description at all, is plain ink.
        let plain = save(&bytes, &store_with(&layout), &geoms, &layout, &[0].into(), &[Some(0)], &[]).expect("save");
        assert!(load(&Arc::new(plain), &geoms, &layout).expect("load").strokes.iter().all(|s| s.3.is_none()));
    }
}
