//! Writes a many-page test PDF with dense text and vector drawing, for scrolling and zoom
//! benchmarks: `inkpdf-gen out.pdf [pages] [--outline]`. With `--outline` it also has a table of contents: a chapter every three
//! pages, and a section under the first.

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

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let with_outline = args.iter().any(|a| a == "--outline");
    args.retain(|a| a != "--outline");
    let mut args = args.into_iter();
    let Some(out) = args.next() else {
        eprintln!("usage: inkpdf-gen out.pdf [pages] [--outline]");
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
        let content = page_content(p);
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
