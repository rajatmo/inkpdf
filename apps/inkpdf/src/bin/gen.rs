//! Writes a many-page test PDF with dense text and vector drawing, for scrolling and zoom
//! benchmarks: `inkpdf-gen out.pdf [pages] [--outline] [--form] [--links]`. With `--outline` it also has a table of contents: a chapter every three
//! pages, and a section under the first. With `--form` page 1 is a form to fill in (a text field, a check box, radio buttons, a combo box and
//! a field of several lines). With `--links` page 1 has two links: one to the third page (or the last, if there are fewer) and one to a web address.

use std::fmt::Write as _;
use std::fs;

const WORDS: &[&str] = &[
    "annotation",
    "stylus",
    "vector",
    "render",
    "page",
    "ink",
    "smooth",
    "latency",
    "tile",
    "zoom",
    "scroll",
    "document",
    "pressure",
    "stroke",
    "layer",
    "canvas",
    "pixel",
    "frame",
    "texture",
    "cache",
    "queue",
    "viewport",
    "anchor",
    "glide",
    "native",
    "windows",
    "pen",
    "touch",
    "highlight",
    "margin",
];

fn page_content(page: usize) -> String {
    let mut c = String::new();
    let _ = writeln!(c, "BT /F2 22 Tf 60 740 Td (Page {}) Tj ET", page + 1);
    let mut seed = (page as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for line in 0..52 {
        let mut text = String::new();
        for _ in 0..11 {
            text.push_str(WORDS[(next() % WORDS.len() as u64) as usize]);
            text.push(' ');
        }
        let _ = writeln!(c, "BT /F1 10 Tf 60 {} Td ({}) Tj ET", 710 - line * 12, text.trim_end());
    }
    // A grid of filled shapes and some curves, so the rasteriser has vector work to do.
    for i in 0..40 {
        let (x, y) = (60.0 + (i % 8) as f64 * 66.0, 60.0 + (i / 8) as f64 * 14.0);
        let g = (next() % 100) as f64 / 100.0;
        let _ = writeln!(c, "{g:.2} {:.2} {:.2} rg {x} {y} 58 10 re f", 1.0 - g, g * 0.5);
    }
    for i in 0..25 {
        let x = 60.0 + i as f64 * 20.0;
        let _ = writeln!(c, "0.2 0.3 0.8 RG 0.8 w {x} 40 m {} 20 {} 60 {} 40 c S", x + 8.0, x + 12.0, x + 20.0);
    }
    c
}

/// A table of contents: a chapter every three pages, and "1.1" under the first one (on its second page).
fn add_outline(objs: &mut Vec<String>, pages: usize) {
    let root = objs.len() + 1;
    let chapters: Vec<usize> = (0..pages).step_by(3).collect();
    let ids: Vec<usize> = (0..chapters.len()).map(|k| root + 1 + k).collect();
    let section = root + 1 + chapters.len();
    let has_section = pages > 1;
    let page_obj = |page: usize| 5 + page * 2;
    let count = chapters.len() + usize::from(has_section);
    objs.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {count} >>", ids[0], ids[ids.len() - 1]));
    for (k, &page) in chapters.iter().enumerate() {
        let mut body = format!("<< /Title (Chapter {}) /Parent {root} 0 R /Dest [{} 0 R /Fit]", k + 1, page_obj(page));
        if k > 0 {
            let _ = write!(body, " /Prev {} 0 R", ids[k - 1]);
        }
        if k + 1 < chapters.len() {
            let _ = write!(body, " /Next {} 0 R", ids[k + 1]);
        }
        if k == 0 && has_section {
            let _ = write!(body, " /First {section} 0 R /Last {section} 0 R /Count 1");
        }
        body.push_str(" >>");
        objs.push(body);
    }
    if has_section {
        objs.push(format!("<< /Title (1.1 First section) /Parent {} 0 R /Dest [{} 0 R /Fit] >>", ids[0], page_obj(1)));
    }
    objs[0] = format!("<< /Type /Catalog /Pages 2 0 R /Outlines {root} 0 R >>");
}

/// What page 1 shows with `--form`: a title and the labels of the fields (the dense text of the other pages would run under them).
fn form_content() -> String {
    let mut c = String::from("BT /F2 22 Tf 60 740 Td (A form to fill in) Tj ET\n");
    for (x, y, label) in [
        (60, 686, "Name"),
        (178, 644, "I agree"),
        (60, 604, "Size"),
        (178, 604, "Small"),
        (278, 604, "Large"),
        (60, 562, "Colour"),
        (60, 520, "Notes"),
    ] {
        let _ = writeln!(c, "BT /F1 12 Tf {x} {y} Td ({label}) Tj ET");
    }
    c
}

/// The form of `--form` on page 1 (objects 5 and 6): a text field, a check box, a group of two radio buttons, a combo box and a field of several
/// lines, set in Helvetica; page 1's boxes are drawn (a frame, and a mark when on) by the file's own appearances.
fn add_form(objs: &mut Vec<String>) {
    let at = objs.len() + 1;
    let frame = "0.4 G 0.5 0.5 17 17 re S";
    let appearance = |extra: &str| {
        let content = format!("{frame}\n{extra}");
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 18 18] /Length {} >>\nstream\n{content}endstream", content.len())
    };
    let widget = |rest: &str| format!("<< /Type /Annot /Subtype /Widget /P 5 0 R /F 4 {rest} >>");
    let text = "/FT /Tx /DA (/Helv 12 Tf 0 g) /V ()";
    // (Objects, in order: the font, the five fields' boxes, the radio group, the form, and the two appearances.)
    let (on, off) = (at + 9, at + 10);
    let states = |name: &str| format!("/AP << /N << /{name} {on} 0 R /Off {off} 0 R >> >> /AS /Off");
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    objs.push(widget(&format!("{text} /T (name) /Rect [150 680 450 704]")));
    objs.push(widget(&format!("/FT /Btn /T (agree) /V /Off /Rect [150 640 168 658] {}", states("Yes"))));
    objs.push("<< /FT /Btn /Ff 32768 /T (size) /V /Off /Kids [".to_string() + &format!("{} 0 R {} 0 R] >>", at + 4, at + 5));
    objs.push(widget(&format!("/Parent {} 0 R /Rect [150 600 168 618] {}", at + 3, states("Small"))));
    objs.push(widget(&format!("/Parent {} 0 R /Rect [250 600 268 618] {}", at + 3, states("Large"))));
    objs.push(widget("/FT /Ch /Ff 131072 /T (colour) /Opt [(Red) (Green) (Blue)] /V () /DA (/Helv 12 Tf 0 g) /Rect [150 556 350 578]"));
    objs.push(widget(&format!("{text} /Ff 4096 /T (notes) /Rect [150 440 450 536]")));
    objs.push(format!(
        "<< /Fields [{} 0 R {} 0 R {} 0 R {} 0 R {} 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv {at} 0 R >> >> >>",
        at + 1,
        at + 2,
        at + 3,
        at + 6,
        at + 7
    ));
    objs.push(appearance("0 g 4 4 10 10 re f\n"));
    objs.push(appearance(""));
    let annots = [at + 1, at + 2, at + 4, at + 5, at + 6, at + 7].map(|n| format!("{n} 0 R")).join(" ");
    objs[4] = objs[4].replace("/Resources", &format!("/Annots [{annots}] /Resources"));
    objs[0] = objs[0].replace(" >>", &format!(" /AcroForm {} 0 R >>", at + 8));
}

/// The links of `--links` on page 1: bands down the page, in turn one that goes to the third page (the last if there are fewer) and one that goes to a web
/// address. (How much of the page a window shows depends on its shape, so a test takes a band that is in the middle of its window.)
fn add_links(objs: &mut Vec<String>, pages: usize) {
    let to = 5 + 2 * 2.min(pages - 1);
    let first = objs.len() + 1;
    let mut annots = Vec::new();
    for slot in 0..10 {
        let top = 792 - (120 + 60 * slot);
        let rect = format!("/Rect [100 {} 300 {top}]", top - 20);
        objs.push(if slot % 2 == 0 {
            format!("<< /Type /Annot /Subtype /Link /Border [0 0 0] {rect} /Dest [{to} 0 R /XYZ 0 792 null] >>")
        } else {
            format!("<< /Type /Annot /Subtype /Link /Border [0 0 0] {rect} /A << /S /URI /URI (https://example.com/page) >> >>")
        });
        annots.push(format!("{} 0 R", first + slot));
    }
    objs[4] = objs[4].replace("/Resources", &format!("/Annots [{}] /Resources", annots.join(" ")));
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let with_outline = args.iter().any(|a| a == "--outline");
    let with_form = args.iter().any(|a| a == "--form");
    let with_links = args.iter().any(|a| a == "--links");
    args.retain(|a| a != "--outline" && a != "--form" && a != "--links");
    let mut args = args.into_iter();
    let Some(out) = args.next() else {
        eprintln!("usage: inkpdf-gen out.pdf [pages] [--outline] [--form] [--links]");
        std::process::exit(2);
    };
    let pages: usize = args.next().and_then(|p| p.parse().ok()).unwrap_or(60).clamp(1, 5000);

    // Objects: 1 catalog, 2 pages, 3 and 4 fonts, then (page, content) pairs.
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(), // filled below once the kids are known
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
    ];
    let mut kids = String::new();
    for p in 0..pages {
        let page_obj = 5 + p * 2;
        let content = if with_form && p == 0 { form_content() } else { page_content(p) };
        let _ = write!(kids, "{page_obj} 0 R ");
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>",
            page_obj + 1
        ));
        objs.push(format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content));
    }
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.trim_end());
    if with_outline {
        add_outline(&mut objs, pages);
    }
    if with_form {
        add_form(&mut objs);
    }
    if with_links && !with_form {
        add_links(&mut objs, pages);
    }

    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    if let Err(e) = fs::write(&out, &pdf) {
        eprintln!("could not write {out}: {e}");
        std::process::exit(1);
    }
    println!("wrote {out}: {pages} pages, {} KB", pdf.len() / 1024);
}
