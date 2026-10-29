//! Thumbnails: rendering the first page of a PDF into pixels. Explorer's thumbnail handler (the DLL
//! in apps/inkpdf-thumb) runs `inkpdf.exe --thumbnail SIZE` for each file, hands it the PDF on
//! standard input and reads the picture from standard output, so a damaged or hostile PDF can only
//! ever take down this short-lived process, never Explorer.
//!
//! The picture is 8 bytes (width, then height, each a little-endian u32) followed by width * height
//! pixels of opaque BGRA, rows from the top.

use std::io::{Read, Write};
use std::sync::Arc;

use crate::pdf::{self, RenderRequest};

/// Files larger than this are not read for a thumbnail: the shell asks for many at once.
pub const MAX_PDF_BYTES: usize = 256 << 20;
/// The smallest and largest thumbnails made, in pixels along the longer side.
pub const MIN_SIDE: u32 = 16;
pub const MAX_SIDE: u32 = 1024;

/// What the handler reads back from this process.
pub fn encode(t: &Thumbnail) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + t.bgra.len());
    out.extend_from_slice(&t.width.to_le_bytes());
    out.extend_from_slice(&t.height.to_le_bytes());
    out.extend_from_slice(&t.bgra);
    out
}

/// The child process: PDF on standard input, picture on standard output. The exit code is 0 when
/// a picture was written.
pub fn run_child(longest: u32) -> i32 {
    let mut pdf = Vec::new();
    if std::io::stdin().take(MAX_PDF_BYTES as u64 + 1).read_to_end(&mut pdf).is_err() {
        return 2;
    }
    let Some(thumbnail) = first_page(pdf, longest) else { return 1 };
    let mut out = std::io::stdout().lock();
    if out.write_all(&encode(&thumbnail)).and_then(|()| out.flush()).is_err() { 3 } else { 0 }
}

/// A finished thumbnail: opaque BGRA pixels, row by row from the top.
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Page 1 of `pdf`, scaled so its longer side is `longest` pixels, on white. `None` if the file
/// cannot be read or the page cannot be drawn.
pub fn first_page(pdf: Vec<u8>, longest: u32) -> Option<Thumbnail> {
    if pdf.is_empty() || pdf.len() > MAX_PDF_BYTES {
        return None;
    }
    let bytes = Arc::new(pdf);
    let longest = longest.clamp(MIN_SIDE, MAX_SIDE);
    let pages = pdf::native().inspect(&bytes).ok()?;
    let page = pages.first()?;
    let side = page.width.max(page.height);
    if !side.is_finite() || side <= 0.0 {
        return None;
    }
    let scale = (longest as f32 / side).clamp(0.005, 4.0);
    let page = pdf::native().render_now(bytes, &RenderRequest { page: 0, scale, ..RenderRequest::default() });
    let (width, height) = (page.width, page.height);
    let rgba: &[u8] = page.rgba.as_slice();
    if page.error.is_some() || width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    // The renderer's pixels are premultiplied and transparent where nothing was drawn: lay them on
    // white paper, which is what a page looks like.
    let mut bgra: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let behind = 255 - p[3];
            [p[2].saturating_add(behind), p[1].saturating_add(behind), p[0].saturating_add(behind), 255]
        })
        .collect();
    draw_border(&mut bgra, width as usize, height as usize);
    Some(Thumbnail { width, height, bgra })
}

/// A hairline of light grey around the page, so a white page does not melt into a white window.
fn draw_border(bgra: &mut [u8], width: usize, height: usize) {
    const EDGE: [u8; 4] = [190, 190, 190, 255];
    for x in 0..width {
        for y in [0, height - 1] {
            bgra[(y * width + x) * 4..][..4].copy_from_slice(&EDGE);
        }
    }
    for y in 0..height {
        for x in [0, width - 1] {
            bgra[(y * width + x) * 4..][..4].copy_from_slice(&EDGE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_is_sent_as_a_small_header_then_the_pixels() {
        let t = Thumbnail { width: 2, height: 1, bgra: vec![1, 2, 3, 255, 4, 5, 6, 255] };
        assert_eq!(encode(&t), [2, 0, 0, 0, 1, 0, 0, 0, 1, 2, 3, 255, 4, 5, 6, 255]);
    }

    /// A one-page 200 x 100 PDF with a black rectangle in it.
    fn pdf() -> Vec<u8> {
        let content = "0 0 0 rg 20 20 100 40 re f\n";
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R /Resources << >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }

    #[test]
    fn the_first_page_comes_out_on_white_at_the_asked_size() {
        let t = first_page(pdf(), 100).expect("thumbnail");
        assert_eq!((t.width, t.height), (100, 50), "the longer side is the size asked for");
        assert_eq!(t.bgra.len(), 100 * 50 * 4);
        assert!(t.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "opaque");
        let at = |x: usize, y: usize| &t.bgra[(y * 100 + x) * 4..][..3];
        assert_eq!(at(5, 5), &[255, 255, 255], "paper where nothing is drawn");
        assert_eq!(at(0, 25), &[190, 190, 190], "a hairline border");
        assert_eq!(at(99, 49), &[190, 190, 190]);
        assert_eq!(at(40, 25), &[0, 0, 0], "the black rectangle, from the top-left of the page");
    }

    #[test]
    fn sizes_are_kept_sensible() {
        let small = first_page(pdf(), 1).expect("clamped up");
        assert_eq!(small.width.max(small.height), MIN_SIDE);
        // Never past the biggest size, and never blown up more than four times over (this page is
        // 200 points wide, so that is 800 pixels).
        let big = first_page(pdf(), 100_000).expect("clamped down");
        assert_eq!(big.width.max(big.height), 800);
        assert!(big.width.max(big.height) <= MAX_SIDE);
    }

    #[test]
    fn anything_that_is_not_a_readable_pdf_gives_nothing() {
        assert!(first_page(Vec::new(), 100).is_none());
        assert!(first_page(b"not a pdf at all".to_vec(), 100).is_none());
        let mut broken = pdf();
        broken.truncate(broken.len() / 2);
        let _ = first_page(broken, 100);
        let mut noisy = pdf();
        for i in (20..noisy.len()).step_by(17) {
            noisy[i] = b'?';
        }
        let _ = first_page(noisy, 100);
    }
}
