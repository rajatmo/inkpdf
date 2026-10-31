//! Text drawn into a bitmap: the glyph outlines cut into edges and filled by a small anti-aliased scanline rasteriser.

use crate::plane::P;

use super::font::{Seg, face};
use super::layout::{lines, measure};
use super::{LINE_HEIGHT, PAD};

/// Largest side of a bitmap, in pixels.
const MAX_SIDE: f32 = 4096.0;
/// Rows of samples per pixel row.
const SUBROWS: usize = 4;

/// A bitmap of text: premultiplied RGBA, [`PAD`] pixels of margin on every side.
#[derive(Clone, Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The text drawn at `scale` pixels per point in `rgba`. `None` if there is nothing to
/// draw or the size is nonsense. The scale is reduced if the bitmap would be too big.
pub fn raster(text: &str, size: f32, scale: f32, rgba: [u8; 4]) -> Option<Bitmap> {
    raster_weighted(text, size, scale, rgba, false)
}

/// [`raster`], heavy (`bold`): each outline is drawn again a little to the right, which thickens its strokes as a bold face does.
pub fn raster_weighted(text: &str, size: f32, scale: f32, rgba: [u8; 4], bold: bool) -> Option<Bitmap> {
    if !(size.is_finite() && scale.is_finite() && size > 0.0 && scale > 0.0) || text.chars().all(|c| c.is_whitespace()) {
        return None;
    }
    let face = face();
    let [w, h] = measure(text, size);
    let pad = PAD;
    let scale = scale.min((MAX_SIDE - 2.0 * pad) / w.max(1.0)).min((MAX_SIDE - 2.0 * pad) / h.max(1.0));
    let (width, height) = ((w * scale + 2.0 * pad).ceil().max(1.0) as usize, (h * scale + 2.0 * pad).ceil().max(1.0) as usize);
    let mut coverage = vec![0.0_f32; width * height];
    let line_h = size * LINE_HEIGHT;
    for (i, line) in lines(text).enumerate() {
        // The baseline: the line's text centred in the line, ascent below its top.
        let baseline = i as f32 * line_h + (line_h - (face.ascent + face.descent) * size) / 2.0 + face.ascent * size;
        let mut edges: Vec<(P, P)> = Vec::new();
        let mut pen_x = 0.0;
        for ch in line.chars() {
            let glyph = face.glyph(ch);
            let at = |p: P| [pad + (pen_x + p[0] * size) * scale, pad + (baseline - p[1] * size) * scale];
            for (start, segs) in &glyph.contours {
                flatten(
                    at(*start),
                    segs.iter().map(|s| match *s {
                        Seg::Line(p) => Seg::Line(at(p)),
                        Seg::Quad(c, p) => Seg::Quad(at(c), at(p)),
                        Seg::Cubic(a, b, p) => Seg::Cubic(at(a), at(b), at(p)),
                    }),
                    size * scale,
                    &mut edges,
                );
            }
            pen_x += glyph.advance * size;
        }
        if bold {
            let by = 0.045 * size * scale;
            let again: Vec<(P, P)> = edges.iter().map(|&(a, b)| ([a[0] + by, a[1]], [b[0] + by, b[1]])).collect();
            edges.extend(again);
        }
        fill(&edges, &mut coverage, width, height, (i as f32 * line_h * scale) as usize, ((i + 1) as f32 * line_h * scale + 2.0 * pad) as usize + 1);
    }
    let [r, g, b, a] = rgba.map(f32::from);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for c in coverage {
        let k = c.clamp(0.0, 1.0) * a / 255.0;
        pixels.extend([(r * k).round() as u8, (g * k).round() as u8, (b * k).round() as u8, (255.0 * k).round() as u8]);
    }
    Some(Bitmap { width: width as u32, height: height as u32, pixels })
}

/// Cut a contour into straight edges (closing it), as many pieces per curve as the size needs.
fn flatten(start: P, segs: impl Iterator<Item = Seg>, size_px: f32, out: &mut Vec<(P, P)>) {
    let pieces = (size_px / 6.0).ceil().clamp(3.0, 32.0) as usize;
    let mut at = start;
    let mut line = |from: P, to: P| out.push((from, to));
    for seg in segs {
        match seg {
            Seg::Line(p) => {
                line(at, p);
                at = p;
            }
            Seg::Quad(c, p) => {
                let mut prev = at;
                for k in 1..=pieces {
                    let t = k as f32 / pieces as f32;
                    let u = 1.0 - t;
                    let q = [u * u * at[0] + 2.0 * u * t * c[0] + t * t * p[0], u * u * at[1] + 2.0 * u * t * c[1] + t * t * p[1]];
                    line(prev, q);
                    prev = q;
                }
                at = p;
            }
            Seg::Cubic(a, b, p) => {
                let mut prev = at;
                for k in 1..=pieces {
                    let t = k as f32 / pieces as f32;
                    let u = 1.0 - t;
                    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    let q = [w[0] * at[0] + w[1] * a[0] + w[2] * b[0] + w[3] * p[0], w[0] * at[1] + w[1] * a[1] + w[2] * b[1] + w[3] * p[1]];
                    line(prev, q);
                    prev = q;
                }
                at = p;
            }
        }
    }
    if at != start {
        line(at, start);
    }
}

/// Add the coverage of the area the edges enclose (non-zero winding) to `coverage`, sampling
/// [`SUBROWS`] rows per pixel row in the pixel rows `rows` (clamped to the bitmap).
fn fill(edges: &[(P, P)], coverage: &mut [f32], width: usize, height: usize, first_row: usize, end_row: usize) {
    if edges.is_empty() {
        return;
    }
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in first_row..end_row.min(height) {
        for sub in 0..SUBROWS {
            let y = row as f32 + (sub as f32 + 0.5) / SUBROWS as f32;
            crossings.clear();
            for &(a, b) in edges {
                let (lo, hi, dir) = if a[1] < b[1] { (a, b, 1) } else { (b, a, -1) };
                if y >= lo[1] && y < hi[1] && hi[1] > lo[1] {
                    let t = (y - lo[1]) / (hi[1] - lo[1]);
                    crossings.push((lo[0] + (hi[0] - lo[0]) * t, dir));
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    span(coverage, row * width, width, pair[0].0, pair[1].0);
                }
            }
        }
    }
}

/// Add one sample row's coverage of the span `[x0, x1)` to the pixel row starting at `at`.
fn span(coverage: &mut [f32], at: usize, width: usize, x0: f32, x1: f32) {
    let (x0, x1) = (x0.max(0.0), x1.min(width as f32));
    if x1 <= x0 {
        return;
    }
    let first = x0.floor() as usize;
    let last = (x1.ceil() as usize).min(width);
    for px in first..last {
        let overlap = (x1.min(px as f32 + 1.0) - x0.max(px as f32)).max(0.0);
        if let Some(c) = coverage.get_mut(at + px) {
            *c += overlap / SUBROWS as f32;
        }
    }
}
