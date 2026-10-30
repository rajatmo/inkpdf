//! What any [`PdfBackend`] must do, as executable checks over a corpus of small PDFs made right here (no binary files to keep in step):
//! page geometry (rotated and cropped pages too), text in reading order, find, the outline and the links, annotations written and
//! read back, pages organized, hostile files, and what a render looks like (a few pixels, within a tolerance, so the Windows and Linux
//! rasterisers agree). Moving to another version of pdfcraft, or one day another pure-Rust engine, is verified by running this.

use std::sync::Arc;

use super::*;
use crate::testutil::{build_pdf, pdf_with_links, pdf_with_outline, sample_pdf};

/// One page of a corpus document.
struct Page {
    media: [i32; 4],
    rotate: i32,
    crop: Option<[i32; 4]>,
    content: String,
}

fn page(w: i32, h: i32, content: &str) -> Page {
    Page { media: [0, 0, w, h], rotate: 0, crop: None, content: content.to_string() }
}

/// A document of these pages, all set in Helvetica (one of the 14 standard fonts: no font file in the PDF).
fn document(pages: &[Page]) -> Vec<u8> {
    let first = 3;
    let font = first + 2 * pages.len();
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", first + 2 * i)).collect();
    let mut objs =
        vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len())];
    for (i, p) in pages.iter().enumerate() {
        let [a, b, c, d] = p.media;
        let crop = p.crop.map_or(String::new(), |[a, b, c, d]| format!(" /CropBox [{a} {b} {c} {d}]"));
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [{a} {b} {c} {d}]{crop} /Rotate {} /Contents {} 0 R /Resources << /Font << /F1 {font} 0 R >> >> >>",
            p.rotate,
            first + 2 * i + 1
        ));
        objs.push(format!("<< /Length {} >>\nstream\n{}\nendstream", p.content.len() + 1, p.content));
    }
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    build_pdf(&objs)
}

/// Text set at a place: `(x, y, size, words)`.
fn words(lines: &[(i32, i32, i32, &str)]) -> String {
    lines.iter().map(|(x, y, size, text)| format!("BT /F1 {size} Tf {x} {y} Td ({text}) Tj ET")).collect::<Vec<_>>().join("\n")
}

fn arc(bytes: Vec<u8>) -> Arc<Vec<u8>> {
    Arc::new(bytes)
}

/// Three pages: plain text; a turned and cropped page; two columns.
fn corpus() -> Arc<Vec<u8>> {
    arc(document(&[
        page(612, 792, &words(&[(72, 700, 24, "Hello world"), (72, 660, 12, "The quick brown fox"), (72, 640, 12, "jumps over the lazy dog")])),
        Page { media: [0, 0, 600, 800], rotate: 90, crop: Some([10, 20, 500, 700]), content: words(&[(72, 600, 24, "Turned page")]) },
        page(
            595,
            842,
            &words(&[
                (72, 700, 12, "Left column text"),
                (72, 686, 12, "continues down here"),
                (320, 700, 12, "Right column text"),
                (320, 686, 12, "also continues"),
            ]),
        ),
    ]))
}

/// A page with a red rectangle at `[100, 200] x [100, 200]` in the PDF's user space (so at x 100 to 300, y 592 to 692 from the top).
fn red_box() -> Arc<Vec<u8>> {
    arc(document(&[page(612, 792, "1 0 0 rg 100 100 200 100 re f")]))
}

fn widths(b: &dyn PdfBackend, bytes: &Arc<Vec<u8>>) -> Vec<f32> {
    b.inspect(bytes).expect("inspect").iter().map(|g| g.width).collect()
}

/// Pages 300, 400 and 500 points wide, to tell them apart after they are organized.
fn trio() -> Arc<Vec<u8>> {
    arc(document(&[page(300, 400, "q Q"), page(400, 400, "q Q"), page(500, 400, "q Q")]))
}

// ── the checks, each over any backend ─────────────────────────────────────────────────────────────

pub(super) fn check_geometry(b: &dyn PdfBackend) {
    let geoms = b.inspect(&corpus()).expect("inspect");
    assert_eq!(geoms.len(), 3, "three pages");
    assert_eq!((geoms[0].width, geoms[0].height, geoms[0].rotation), (612.0, 792.0, 0));
    assert_eq!(geoms[0].crop, [0.0, 0.0, 612.0, 792.0], "no crop box: the page");
    // A page turned a quarter and cropped is as big as the crop box, turned: its displayed width is the box's height.
    assert_eq!((geoms[1].width, geoms[1].height, geoms[1].rotation), (680.0, 490.0, 90));
    assert_eq!(geoms[1].crop, [10.0, 20.0, 500.0, 700.0], "the crop box in the page's own user space");
    assert_eq!((geoms[2].width, geoms[2].height), (595.0, 842.0));
    assert!(b.inspect(&arc(Vec::new())).is_err() && b.inspect(&arc(b"not a pdf".to_vec())).is_err(), "what is not a PDF is refused");
    let blank = b.blank([300.0, 400.0]).expect("blank");
    let geoms = b.inspect(&arc(blank)).expect("a blank document is a document");
    assert_eq!((geoms.len(), geoms[0].width, geoms[0].height), (1, 300.0, 400.0));
}

pub(super) fn check_sizing(_: &dyn PdfBackend) {
    assert_eq!((device_pixels(612.0, 1.0), device_pixels(612.0, 2.0), device_pixels(0.0, 1.0)), (612, 1224, 1));
    assert_eq!(device_pixels(612.0, 1.0 / 3.0), 204, "float noise does not round a whole number of pixels up");
    for scale in [0.001, 1.0, 100.0, 1.0e9, f32::NAN, 0.0, -1.0] {
        let s = effective_scale(612.0, 792.0, scale);
        assert!(s.is_finite() && s > 0.0, "a scale of {scale} becomes {s}: something the renderer takes");
    }
    assert!(effective_scale(612.0, 792.0, 1.0e9) < 100.0, "a huge scale is capped so a page cannot be made enormous");
}

pub(super) fn check_text(b: &dyn PdfBackend) {
    let doc = corpus();
    let t = b.page_text(&doc, 0).expect("text of page 0");
    assert_eq!(t.plain_text(), "Hello world\nThe quick brown fox\njumps over the lazy dog");
    let all: Vec<usize> = (0..t.glyph_count()).collect();
    assert_eq!(t.text_of(0..t.glyph_count()), t.plain_text(), "the whole range is the whole text");
    assert!(!t.is_empty());
    // A glyph is in its own word and line, and is nearest to itself.
    for g in [0, 6, 20, t.glyph_count() - 1] {
        let (a, z) = t.word_at(g).expect("a word");
        assert!(a <= g && g <= z, "glyph {g} is in its word {a}..={z} (first and last glyph, both included)");
        let (a, z) = t.line_at(g).expect("a line");
        assert!(a <= g && g <= z, "glyph {g} is on its line {a}..={z}");
        let r = t.glyph_rects(&[g])[0];
        assert_eq!(t.nearest((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0), Some(g), "the middle of glyph {g} is nearest to it");
    }
    assert_eq!(t.word_at(0).map(|(a, z)| t.text_of(a..z + 1)), Some("Hello".to_string()));
    assert_eq!(t.line_at(20).map(|(a, z)| t.text_of(a..z + 1)), Some("The quick brown fox".to_string()));
    // A marquee over everything takes everything; a selection is one box per line.
    assert_eq!(t.glyphs_in([0.0, 0.0, 612.0, 792.0]).len(), t.glyph_count());
    assert_eq!(t.line_rects(0..t.glyph_count()).len(), 3, "three lines, three boxes");
    assert_eq!(t.column_text(&all), t.plain_text());
    assert!(b.page_text(&doc, 99).map_or(true, |t| t.is_empty()), "a page that is not there has no text (or is refused)");
}

pub(super) fn check_find(b: &dyn PdfBackend) {
    let doc = corpus();
    let t = b.page_text(&doc, 0).expect("text");
    let found = t.find("WORLD", false, false, 10);
    assert_eq!(found, vec![6..11], "case is folded");
    assert!(t.find("WORLD", true, false, 10).is_empty(), "or kept");
    assert_eq!(t.find("row", false, false, 10).len(), 1, "inside a word");
    assert!(t.find("row", false, true, 10).is_empty(), "but not as a word of its own");
    assert_eq!(t.find("fox", false, true, 10).len(), 1);
    let o = t.plain_text().to_lowercase().matches('o').count();
    assert_eq!(t.find("o", false, false, 100).len(), o, "every one");
    assert_eq!(t.find("o", false, false, 2).len(), 2, "up to the limit");
    for hit in t.find("o", false, false, 100) {
        assert_eq!(t.text_of(hit).to_lowercase(), "o");
    }
    assert!(t.find("", false, false, 10).is_empty() && t.find("zzzz", false, false, 10).is_empty());
    // Two columns are read line by line today (a line of the left column, then the same line of the right): what a selection across
    // them gets. If an engine reads column by column, this is where it shows.
    let t2 = b.page_text(&doc, 2).expect("text");
    let all: Vec<usize> = (0..t2.glyph_count()).collect();
    assert_eq!(t2.column_text(&all), "Left column text\tRight column text\ncontinues down here\talso continues");
    // A page turned in the file has its text in the page as displayed: the text of "Turned page" is on the long side.
    let t1 = b.page_text(&doc, 1).expect("text");
    assert_eq!(t1.plain_text(), "Turned page");
    let r = t1.line_rects(0..t1.glyph_count())[0];
    assert!(r[3] - r[1] > r[2] - r[0], "a line of text on a page turned a quarter runs down the page: {r:?}");
    assert!(r[2] <= 680.0 && r[3] <= 490.0, "and is inside the displayed page: {r:?}");
}

pub(super) fn check_outline_and_links(b: &dyn PdfBackend) {
    let outline = b.open(&arc(pdf_with_outline())).expect("open").outline();
    let flat: Vec<(String, usize, Option<usize>, bool)> = outline.iter().map(|o| (o.title.clone(), o.depth, o.page, o.open)).collect();
    assert_eq!(
        flat,
        [
            ("Chapter 1".to_string(), 0, Some(0), true),
            ("1.1 Basics".to_string(), 1, Some(1), false),
            ("1.2 More".to_string(), 1, Some(2), false),
            ("Chapter 2".to_string(), 0, Some(3), false),
            ("Appendix".to_string(), 0, None, false),
        ]
    );
    assert!(b.open(&arc(sample_pdf())).expect("open").outline().is_empty(), "a file with no outline has none");
    let links = b.open(&arc(pdf_with_links())).expect("open").links();
    let kinds: Vec<(usize, RawTarget)> = links.iter().map(|l| (l.page, l.target.clone())).collect();
    assert_eq!(
        &kinds[..4],
        &[(0, RawTarget::Url("https://example.com/".into())), (0, RawTarget::Page(1)), (0, RawTarget::Page(1)), (0, RawTarget::Page(0))],
        "a web address, an explicit destination, one in the names tree, one in the old dictionary"
    );
    assert_eq!(links[0].rect, [100.0, 680.0, 300.0, 700.0], "the box in the page's own user space");
    // What the engine hands over is raw: refusing a launch or a script is the program's business (`links::safe_url`), so a
    // backend may list them as addresses, but it must not turn them into pages.
    assert!(links[4..].iter().all(|l| !matches!(l.target, RawTarget::Page(_))));
}

pub(super) fn check_annotations(b: &dyn PdfBackend) {
    let base = arc(sample_pdf());
    let mut doc = b.open(&base).expect("open");
    assert!(doc.annotations().is_empty());
    let ink = NewAnnotation {
        page: 0,
        shape: NewShape::Ink { strokes: vec![vec![[10.0, 10.0], [50.0, 50.0], [90.0, 20.0]], vec![[100.0, 100.0], [120.0, 140.0]]] },
        colour: [1.0, 0.0, 0.5],
        opacity: 0.5,
        width: 2.5,
        text: String::new(),
        author: "InkPDF".into(),
        name: "inkpdf-test-1".into(),
        date: b.date(0),
    };
    let text = NewAnnotation {
        shape: NewShape::Text { rect: [100.0, 300.0, 260.0, 340.0], font_size: 18.0 },
        colour: [0.0, 0.0, 1.0],
        opacity: 1.0,
        width: 0.0,
        text: "Typed\nnote".into(),
        name: "inkpdf-test-2".into(),
        ..ink.clone()
    };
    doc.add_annotation(&ink).expect("add ink");
    doc.add_annotation(&text).expect("add text");
    let updated = arc(doc.save_incremental(&b.date(0)).expect("an update"));
    assert!(updated.starts_with(&base[..]), "an update is appended: the original bytes stay at the start of the file");
    let back = b.open(&updated).expect("reopen").annotations();
    assert_eq!(back.len(), 2);
    let (i, t) = (&back[0], &back[1]);
    assert_eq!((i.kind, i.name.as_str(), i.page, i.index), (AnnotKind::Ink, "inkpdf-test-1", 0, 0));
    assert_eq!(i.strokes, vec![vec![[10.0, 10.0], [50.0, 50.0], [90.0, 20.0]], vec![[100.0, 100.0], [120.0, 140.0]]]);
    assert!(
        i.colour.iter().zip([1.0, 0.0, 0.5]).all(|(a, b)| (a - b).abs() < 1e-3) && (i.opacity - 0.5).abs() < 1e-3 && (i.width - 2.5).abs() < 1e-3
    );
    assert_eq!((t.kind, t.name.as_str(), t.text.as_str(), t.index), (AnnotKind::Text, "inkpdf-test-2", "Typed\nnote", 1));
    assert!((t.font_size - 18.0).abs() < 1e-3 && t.colour.iter().zip([0.0, 0.0, 1.0]).all(|(a, b)| (a - b).abs() < 1e-3));
    assert!(t.rect.iter().zip([100.0, 300.0, 260.0, 340.0]).all(|(a, b)| (a - b).abs() < 1e-3), "{:?}", t.rect);
    // Deleting one (the highest index first, as the program does) leaves the other, in the file written whole.
    let mut doc = b.open(&updated).expect("open");
    doc.delete_annotation(0, 1).expect("delete");
    let whole = arc(doc.save_full().expect("whole file"));
    let left = b.open(&whole).expect("reopen").annotations();
    assert_eq!(left.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["inkpdf-test-1"]);
    assert!(b.open(&whole).expect("open").delete_annotation(0, 7).is_err(), "an annotation that is not there");
    // An annotation without our kind of name is still listed with its name, and one without any is not: the program filters by name.
    let foreign = NewAnnotation { name: "other-1".into(), ..ink };
    let mut doc = b.open(&base).expect("open");
    doc.add_annotation(&foreign).expect("add");
    let listed = b.open(&arc(doc.save_incremental(&b.date(0)).expect("update"))).expect("open").annotations();
    assert_eq!(listed.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["other-1"]);
    assert!(b.date(0).starts_with("D:1970"), "a date in the file's own format: {}", b.date(0));
}

pub(super) fn check_organize(b: &dyn PdfBackend) {
    let base = trio();
    let after = |edit: &dyn Fn(&mut dyn PdfDocument) -> Result<(), String>| -> Vec<f32> {
        let mut doc = b.open(&base).expect("open");
        edit(&mut *doc).expect("the edit");
        widths(b, &arc(doc.save_full().expect("save")))
    };
    assert_eq!(b.open(&base).expect("open").page_count(), Ok(3));
    assert_eq!(after(&|d| d.duplicate_pages(&[0], Some(3))), [300.0, 400.0, 500.0, 300.0], "a copy goes where it is told (the program always says)");
    assert_eq!(after(&|d| d.duplicate_pages(&[2], Some(0))), [500.0, 300.0, 400.0, 500.0], "at the front too");
    assert_eq!(after(&|d| d.duplicate_pages(&[0], None)), [300.0, 300.0, 400.0, 500.0], "with no place told, right after the page it copies");
    assert_eq!(after(&|d| d.delete_pages(&[1]).map_err(|e| e.to_string())), [300.0, 500.0]);
    assert_eq!(after(&|d| d.insert_blank_page(1, [100.0, 200.0])), [300.0, 100.0, 400.0, 500.0], "a blank page as big as it is told");
    assert_eq!(after(&|d| d.move_pages(&[0], 2)), [400.0, 500.0, 300.0]);
    assert_eq!(after(&|d| d.move_pages(&[2], 0)), [500.0, 300.0, 400.0]);
    let mut doc = b.open(&base).expect("open");
    assert_eq!(doc.delete_pages(&[0, 1, 2]), Err(OrganizeError::WouldRemoveAllPages), "a file cannot have none");
    assert!(doc.delete_pages(&[9]).is_err() && doc.move_pages(&[9], 0).is_err(), "pages that are not there");
    assert_eq!(widths(b, &arc(doc.save_full().expect("save"))), [300.0, 400.0, 500.0], "what was refused changed nothing");
}

pub(super) fn check_render(b: &dyn PdfBackend) {
    let doc = red_box();
    let pixel = |r: &Rendered, x: usize, y: usize| -> [u8; 4] {
        let at = (y * r.width as usize + x) * 4;
        r.rgba.as_slice()[at..at + 4].try_into().expect("a pixel")
    };
    let page = b.render_now(doc.clone(), &RenderRequest { page: 0, scale: 1.0, tag: 7, ..RenderRequest::default() });
    assert!(page.error.is_none(), "{:?}", page.error);
    assert_eq!((page.width, page.height, page.tag, page.page), (612, 792, 7, 0), "a page at 1 pixel to the point, with its tag back");
    assert_eq!(page.rgba.as_slice().len(), 612 * 792 * 4);
    // Premultiplied RGBA: the box is opaque red (within a tolerance: rasterisers differ at the edges, not in the middle), and where
    // nothing is drawn there is nothing.
    let inside = pixel(&page, 200, 640);
    assert!(inside[0] > 230 && inside[1] < 25 && inside[2] < 25 && inside[3] > 230, "inside the box: {inside:?}");
    // Where nothing is drawn the page is paper (opaque white) or nothing (transparent), as the engine makes it: never red.
    let bare = |p: [u8; 4]| p[3] == 0 || (p[0] > 240 && p[1] > 240 && p[2] > 240);
    assert!(bare(pixel(&page, 20, 20)), "nothing where nothing is drawn: {:?}", pixel(&page, 20, 20));
    assert!(bare(pixel(&page, 200, 100)), "not above the box either (the page's top is at the top of the pixels): {:?}", pixel(&page, 200, 100));
    // A tile is a part of the page, drawn alone, as big as it was asked.
    let tile = b.render_now(
        doc.clone(),
        &RenderRequest { page: 0, scale: 1.0, tile: Some(Tile { x: 120, y: 600, w: 64, h: 48 }), ..RenderRequest::default() },
    );
    assert_eq!((tile.width, tile.height), (64, 48));
    assert!(tile.rgba.as_slice().chunks(4).all(|p| p[0] > 230 && p[3] > 230), "a tile inside the box is all red");
    // At twice the scale, twice the pixels; a preview is the same size.
    let twice = b.render_now(doc.clone(), &RenderRequest { page: 0, scale: 2.0, ..RenderRequest::default() });
    assert_eq!((twice.width, twice.height), (1224, 1584));
    let preview = b.render_now(doc.clone(), &RenderRequest { page: 0, scale: 0.25, preview: true, ..RenderRequest::default() });
    assert_eq!((preview.width, preview.height), (device_pixels(612.0, 0.25), device_pixels(792.0, 0.25)));
    // A turned page is rendered as displayed.
    let turned = b.render_now(corpus(), &RenderRequest { page: 1, scale: 1.0, ..RenderRequest::default() });
    assert_eq!((turned.width, turned.height), (680, 490));
    // A page that is not there, and what is not a PDF, come back as an error, not a crash.
    assert!(b.render_now(doc.clone(), &RenderRequest { page: 9, ..RenderRequest::default() }).error.is_some());
    assert!(b.render_now(arc(b"not a pdf".to_vec()), &RenderRequest::default()).error.is_some());
}

pub(super) fn check_pool(b: &dyn PdfBackend) {
    let pool = b.render_pool(red_box(), 2, PageLook::Plain);
    let want = |page, tag| RenderRequest { page, scale: 0.5, tag, ..RenderRequest::default() };
    pool.set_queue(vec![want(0, 11), want(0, 12)]);
    let mut tags = Vec::new();
    let started = std::time::Instant::now();
    while tags.len() < 2 && started.elapsed() < std::time::Duration::from_secs(20) {
        match pool.try_recv() {
            Some(r) => {
                assert!(r.error.is_none(), "{:?}", r.error);
                assert_eq!((r.width, r.height, r.page), (306, 396, 0));
                tags.push(r.tag);
            }
            None => std::thread::sleep(std::time::Duration::from_millis(5)),
        }
    }
    tags.sort_unstable();
    assert_eq!(tags, [11, 12], "every request comes back once, with its own tag");
    assert!(pool.try_recv().is_none(), "and nothing more");
}

/// A pool renders in the look it was made with: the plain look leaves a blank page white, the dark one draws it on the program's dark
/// paper, and a page with writing on it keeps its writing readable (lighter than the paper) rather than going all dark.
pub(super) fn check_look(b: &dyn PdfBackend) {
    let first = |bytes: Arc<Vec<u8>>, look| {
        let pool = b.render_pool(bytes, 1, look);
        pool.set_queue(vec![RenderRequest { page: 0, scale: 0.5, tag: 1, ..RenderRequest::default() }]);
        let started = std::time::Instant::now();
        loop {
            if let Some(r) = pool.try_recv() {
                assert!(r.error.is_none(), "{:?}", r.error);
                return r;
            }
            assert!(started.elapsed() < std::time::Duration::from_secs(20), "the page was never rendered");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    let pixel = |r: &Rendered, x: u32, y: u32| {
        let at = ((y * r.width + x) * 4) as usize;
        [r.rgba.as_slice()[at], r.rgba.as_slice()[at + 1], r.rgba.as_slice()[at + 2]]
    };
    let blank = arc(b.blank([200.0, 200.0]).expect("a blank page"));
    let plain = first(blank.clone(), PageLook::Plain);
    assert_eq!(pixel(&plain, 50, 50), [255, 255, 255], "a plain page is white");
    let dark = first(blank, PageLook::Dark);
    let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2);
    assert!(near(pixel(&dark, 50, 50), crate::palette::DARK_PAPER), "a dark page is the dark paper: {:?}", pixel(&dark, 50, 50));
    // A page with writing: the writing is light on the dark paper, and the look changes nothing but the pictures.
    let written = first(arc(sample_pdf()), PageLook::Dark);
    let light = |p: [u8; 3]| p.iter().map(|&v| u32::from(v)).sum::<u32>() > 3 * 150;
    let bright = (0..written.height).flat_map(|y| (0..written.width).map(move |x| (x, y))).filter(|&(x, y)| light(pixel(&written, x, y))).count();
    let all = (written.width * written.height) as usize;
    assert!(bright > 0 && bright < all / 2, "light writing on a dark page, not a light page: {bright} of {all} pixels are light");
}

pub(super) fn check_hostile(b: &dyn PdfBackend) {
    // A file that is cut short, or damaged in many places, is refused or opens with less: never a panic, anywhere the program looks.
    let good = [corpus(), arc(pdf_with_links()), arc(pdf_with_outline())];
    let explore = |bytes: Vec<u8>| {
        let bytes = arc(bytes);
        let _ = b.inspect(&bytes);
        if let Ok(mut doc) = b.open(&bytes) {
            let _ = (doc.page_count(), doc.links(), doc.outline(), doc.annotations());
            let _ = doc.delete_annotation(0, 0);
            let _ = doc.save_full();
        }
        let _ = b.page_text(&bytes, 0).map(|t| (t.plain_text(), t.find("a", false, false, 5)));
        let _ = b.render_now(bytes, &RenderRequest { scale: 0.1, ..RenderRequest::default() });
    };
    for bytes in &good {
        for cut in (0..bytes.len()).step_by(61) {
            explore(bytes[..cut].to_vec());
        }
        for at in (20..bytes.len().saturating_sub(20)).step_by(29) {
            let mut damaged = bytes.to_vec();
            damaged[at] = b'#';
            explore(damaged);
        }
    }
    explore(Vec::new());
    explore(vec![0xff; 5000]);
}

// ── any backend ──────────────────────────────────────────────────────────────────────────────────

/// Every check, over a backend. A new backend runs this and nothing else.
#[allow(dead_code, reason = "for the backend that comes next: `backend_contract(&TheNewOne)`")]
pub(super) fn backend_contract(b: &dyn PdfBackend) {
    check_geometry(b);
    check_sizing(b);
    check_text(b);
    check_find(b);
    check_outline_and_links(b);
    check_annotations(b);
    check_organize(b);
    check_render(b);
    check_pool(b);
    check_look(b);
    check_hostile(b);
}

// pdfcraft, one test per check (so a failure names what broke).

#[test]
fn pdfcraft_reads_the_geometry_of_pages() {
    check_geometry(native());
}

#[test]
fn pdfcraft_sizes_renders_as_the_program_lays_tiles_out() {
    check_sizing(native());
}

#[test]
fn pdfcraft_reads_the_text_of_a_page() {
    check_text(native());
}

#[test]
fn pdfcraft_finds_text_and_reads_turned_pages_and_columns() {
    check_find(native());
}

#[test]
fn pdfcraft_reads_the_outline_and_the_links() {
    check_outline_and_links(native());
}

#[test]
fn pdfcraft_writes_annotations_and_reads_them_back() {
    check_annotations(native());
}

#[test]
fn pdfcraft_organizes_pages() {
    check_organize(native());
}

#[test]
fn pdfcraft_renders_what_the_page_holds() {
    check_render(native());
}

#[test]
fn pdfcraft_renders_in_a_pool_behind_the_frame() {
    check_pool(native());
}

#[test]
fn pdfcraft_renders_pages_in_the_dark_look() {
    check_look(native());
}

#[test]
fn pdfcraft_survives_damaged_files() {
    check_hostile(native());
}

/// A backend that gets the geometry of the second page wrong, to show that the suite can fail.
struct Wrong<'a>(&'a dyn PdfBackend);

impl PdfBackend for Wrong<'_> {
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<Vec<PageGeom>, String> {
        let mut geoms = self.0.inspect(bytes)?;
        if let Some(g) = geoms.get_mut(1) {
            g.width += 1.0;
        }
        Ok(geoms)
    }

    fn blank(&self, size: [f32; 2]) -> Result<Vec<u8>, String> {
        self.0.blank(size)
    }

    fn open(&self, bytes: &Arc<Vec<u8>>) -> Result<Box<dyn PdfDocument>, String> {
        self.0.open(bytes)
    }

    fn render_pool(&self, bytes: Arc<Vec<u8>>, threads: usize, look: PageLook) -> Box<dyn RenderPool> {
        self.0.render_pool(bytes, threads, look)
    }

    fn render_now(&self, bytes: Arc<Vec<u8>>, request: &RenderRequest) -> Rendered {
        self.0.render_now(bytes, request)
    }

    fn page_text(&self, bytes: &Arc<Vec<u8>>, page: usize) -> Result<Box<dyn PageTextView>, String> {
        self.0.page_text(bytes, page)
    }

    fn text_source(&self, bytes: Arc<Vec<u8>>) -> Box<dyn TextSource> {
        self.0.text_source(bytes)
    }

    fn date(&self, unix_seconds: i64) -> String {
        self.0.date(unix_seconds)
    }
}

#[test]
fn the_suite_fails_for_a_backend_that_is_wrong() {
    let wrong = Wrong(native());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check_geometry(&wrong))).is_err(), "a wrong page width is caught");
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check_text(&wrong))).is_ok(), "and what it got right still passes");
}

// ── numbers, not checks ──────────────────────────────────────────────────────────────────────────

/// Times and memory of the backend on a dense generated document (`inkpdf-gen out.pdf 200 --outline`; give its path in
/// `INKPDF_MEASURE_PDF`): what `docs/PDF-BACKEND.md` records, and what to compare before moving to another version. Run it on the
/// machine you care about, in release: `INKPDF_MEASURE_PDF=... cargo test -p inkpdf --lib --release measure -- --ignored --nocapture`.
#[test]
#[ignore = "prints numbers: run by hand, in release, with INKPDF_MEASURE_PDF"]
fn measure_the_backend() {
    use std::time::Instant;
    let Ok(path) = std::env::var("INKPDF_MEASURE_PDF") else { return eprintln!("set INKPDF_MEASURE_PDF to a PDF made by inkpdf-gen") };
    let b = native();
    let bytes = arc(std::fs::read(path).expect("read the PDF"));
    let rss = || -> u64 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let kb =
            |key: &str| status.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok());
        kb("VmHWM:").unwrap_or(0) / 1024
    };
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    let summary = |mut v: Vec<f64>| -> String {
        v.sort_by(f64::total_cmp);
        format!("median {:.2} ms, p95 {:.2} ms, worst {:.2} ms ({} samples)", v[v.len() / 2], v[v.len() * 95 / 100], v[v.len() - 1], v.len())
    };
    let t = Instant::now();
    let geoms = b.inspect(&bytes).expect("inspect");
    eprintln!("file: {} bytes, {} pages; inspect {:.1} ms; peak RSS so far {} MB", bytes.len(), geoms.len(), ms(t), rss());
    let t = Instant::now();
    let doc = b.open(&bytes).expect("open");
    eprintln!("open for change {:.1} ms; outline {} items, links {}", ms(t), doc.outline().len(), doc.links().len());
    // The pool, one thread, a mix as the frame asks for it: a preview, a thumbnail and the sharp tiles of a page, over many pages.
    let n = geoms.len().min(60);
    let mut requests = Vec::new();
    let mut tag = 0;
    for (page, geom) in geoms.iter().enumerate().take(n) {
        let [w, h] = [geom.width, geom.height];
        let tiles = crate::tiles::request(crate::tiles::Key::Preview { page: page as u32 }, [w, h]);
        let thumb = crate::tiles::request(crate::tiles::Key::Thumb { page: page as u32 }, [w, h]);
        let tile = crate::tiles::request(crate::tiles::Key::Tile { page: page as u32, level: 2, tx: 1, ty: 1 }, [w, h]);
        for r in [tiles, thumb, tile] {
            tag += 1;
            requests.push(RenderRequest { tag, ..r });
        }
    }
    let pool = b.render_pool(bytes.clone(), 1, PageLook::Plain);
    let (mut per, mut last) = (Vec::new(), Instant::now());
    pool.set_queue(requests.clone());
    let started = Instant::now();
    while per.len() < requests.len() && started.elapsed().as_secs() < 300 {
        if let Some(r) = pool.try_recv() {
            assert!(r.error.is_none(), "{:?}", r.error);
            per.push(ms(last));
            last = Instant::now();
        } else {
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
    }
    eprintln!(
        "pool, 1 thread, {} requests (preview, thumbnail and a sharp tile of each of {n} pages): {}; total {:.0} ms",
        requests.len(),
        summary(per),
        ms(started)
    );
    // Whole-page renders and text, each from a fresh parse (what a single call costs).
    let one = |r: RenderRequest| {
        let t = Instant::now();
        let p = b.render_now(bytes.clone(), &r);
        assert!(p.error.is_none());
        ms(t)
    };
    let pages: Vec<usize> = (0..n).step_by(5).collect();
    eprintln!(
        "render_now at 1.0, parse included: {}",
        summary(pages.iter().map(|&p| one(RenderRequest { page: p, scale: 1.0, ..RenderRequest::default() })).collect())
    );
    eprintln!(
        "render_now at 2.0, parse included: {}",
        summary(pages.iter().map(|&p| one(RenderRequest { page: p, scale: 2.0, ..RenderRequest::default() })).collect())
    );
    let text: Vec<f64> = pages
        .iter()
        .map(|&p| {
            let t = Instant::now();
            let text = b.page_text(&bytes, p).expect("text");
            let found = text.find("stylus", false, false, 5000).len();
            assert!(text.glyph_count() > 100 && found > 0, "page {p}");
            ms(t)
        })
        .collect();
    eprintln!("page_text + find, parse included: {}", summary(text));
    eprintln!("peak RSS {} MB", rss());
}
