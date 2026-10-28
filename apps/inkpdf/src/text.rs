//! Typed text: measuring it and drawing it into a bitmap.
//!
//! The font is a system one (Arial, whose letters are as wide as Helvetica's, which is what other
//! PDF readers use for the text this app saves). Its outlines are read with `skrifa` and filled by a
//! small anti-aliased scanline rasterizer, so text is sharp at any zoom. If there is no font
//! to be found, text still measures (at a typical letter width) and is saved, but draws nothing.
//!
//! Sizes are in points; a line is [`LINE_HEIGHT`] times the size tall; `\n` starts a new line.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::*;

use crate::ink::P;

/// How tall a line is, as a multiple of the font size.
pub const LINE_HEIGHT: f32 = 1.25;
/// The fewest characters wide an empty box is measured as, so there is something to tap.
const MIN_EM: f32 = 0.6;
/// Most characters a text keeps, so a stray paste cannot make an enormous bitmap.
pub const MAX_CHARS: usize = 4_000;
/// The margin around the box in a bitmap, in pixels, so anti-aliased edges are not cut off.
pub const PAD: f32 = 2.0;
/// Largest side of a bitmap, in pixels.
const MAX_SIDE: f32 = 4096.0;
/// Rows of samples per pixel row.
const SUBROWS: usize = 4;
/// Where, in the Windows fonts folder, to look for a font, best first.
const FONT_FILES: [&str; 4] = ["arial.ttf", "segoeui.ttf", "calibri.ttf", "tahoma.ttf"];

/// A segment of an outline, in em units (y up).
#[derive(Clone, Copy, Debug)]
enum Seg {
    Line(P),
    Quad(P, P),
    Cubic(P, P, P),
}

#[derive(Clone, Debug, Default)]
struct Glyph {
    /// Advance in ems.
    advance: f32,
    /// Each contour: where it starts and its segments.
    contours: Vec<(P, Vec<Seg>)>,
}

struct Pen {
    scale: f32,
    contours: Vec<(P, Vec<Seg>)>,
}

impl Pen {
    fn at(&self, x: f32, y: f32) -> P {
        [x * self.scale, y * self.scale]
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.contours.push((p, Vec::new()));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        if let Some(c) = self.contours.last_mut() {
            c.1.push(Seg::Line(p));
        }
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (c, p) = (self.at(cx, cy), self.at(x, y));
        if let Some(contour) = self.contours.last_mut() {
            contour.1.push(Seg::Quad(c, p));
        }
    }

    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (a, b, p) = (self.at(c0x, c0y), self.at(c1x, c1y), self.at(x, y));
        if let Some(contour) = self.contours.last_mut() {
            contour.1.push(Seg::Cubic(a, b, p));
        }
    }

    fn close(&mut self) {}
}

struct Face {
    font: FontRef<'static>,
    upem: f32,
    /// Ascent and descent in ems (descent positive).
    ascent: f32,
    descent: f32,
    glyphs: Mutex<HashMap<char, Glyph>>,
}

static FACE: LazyLock<Option<Face>> = LazyLock::new(|| {
    let dir = std::env::var_os("WINDIR").map_or_else(|| std::path::PathBuf::from(r"C:\Windows"), std::path::PathBuf::from).join("Fonts");
    FONT_FILES.iter().find_map(|name| {
        // The bytes live as long as the program: the parser borrows them.
        let bytes: &'static [u8] = Box::leak(std::fs::read(dir.join(name)).ok()?.into_boxed_slice());
        let font = FontRef::new(bytes).ok()?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(metrics.units_per_em).max(1.0);
        Some(Face { font, upem, ascent: metrics.ascent / upem, descent: -metrics.descent / upem, glyphs: Mutex::new(HashMap::new()) })
    })
});

impl Face {
    fn glyph(&self, ch: char) -> Glyph {
        let mut cache = self.glyphs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(g) = cache.get(&ch) {
            return g.clone();
        }
        let g = self.load(ch).unwrap_or_else(|| self.load('?').unwrap_or_default());
        cache.insert(ch, g.clone());
        g
    }

    fn load(&self, ch: char) -> Option<Glyph> {
        let id = self.font.charmap().map(ch)?;
        let advance = self.font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(id)? / self.upem;
        let mut pen = Pen { scale: 1.0 / self.upem, contours: Vec::new() };
        if let Some(outline) = self.font.outline_glyphs().get(id) {
            outline.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen).ok()?;
        }
        Some(Glyph { advance, contours: pen.contours })
    }
}

/// Whether a font was found, so text can be drawn.
pub fn available() -> bool {
    FACE.is_some()
}

/// A character's advance in ems.
fn advance(ch: char) -> f32 {
    match &*FACE {
        Some(face) => face.glyph(ch).advance,
        None => 0.55,
    }
}

/// The characters to show for typed text: tabs become spaces, other control characters go, and it
/// stops at [`MAX_CHARS`].
pub fn clean(text: &str) -> String {
    text.chars().map(|c| if c == '\t' { ' ' } else { c }).filter(|c| *c == '\n' || !c.is_control()).take(MAX_CHARS).collect()
}

/// The lines of `text`.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
}

fn line_width_em(line: &str) -> f32 {
    line.chars().map(advance).sum()
}

/// `text` with each line that is wider than `max_width` points at `size` broken into lines that fit:
/// at the last space that fits (the space stays at the end of its line, so taking the break away
/// gives the words back as they were), or in the middle of a word too long for a line. Lines that fit are
/// left alone, so what is already broken stays broken. Stops at [`MAX_CHARS`].
pub fn wrap(text: &str, size: f32, max_width: f32) -> String {
    if !(size.is_finite() && size > 0.0 && max_width.is_finite() && max_width > 0.0) {
        return text.to_string();
    }
    let limit = max_width / size;
    let mut out = String::with_capacity(text.len() + 16);
    for (i, line) in lines(text).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut rest = line;
        // (Spaces at the end of a line hang over the edge: they never make a line too wide.)
        while !rest.is_empty() && line_width_em(rest.trim_end_matches(' ')) > limit {
            let (mut width, mut cut, mut after_space) = (0.0, 0, None);
            for (at, c) in rest.char_indices() {
                width += advance(c);
                if width > limit && at > 0 && c != ' ' {
                    break;
                }
                cut = at + c.len_utf8();
                if c == ' ' {
                    after_space = Some(cut);
                }
            }
            // At least one character goes on every line (`cut` is past the first), so this always ends.
            let (piece, tail) = rest.split_at(after_space.unwrap_or(cut));
            out.push_str(piece);
            rest = tail;
            if rest.is_empty() {
                break;
            }
            out.push('\n');
        }
        out.push_str(rest);
    }
    out.chars().take(MAX_CHARS).collect()
}

/// The size of the box that holds `text` at `size` points, as `[width, height]`.
pub fn measure(text: &str, size: f32) -> [f32; 2] {
    let size = size.max(0.0);
    // (Spaces at the end of a line hang: they take no room in the box, which is what wrapping measures.)
    let widest = lines(text).map(|l| line_width_em(l.trim_end_matches(' '))).fold(0.0_f32, f32::max).max(MIN_EM);
    [widest * size, lines(text).count() as f32 * size * LINE_HEIGHT]
}

/// Where the caret is at the end of `text`, from the box's top-left: along the last line, and the
/// top of that line.
pub fn caret(text: &str, size: f32) -> P {
    let n = lines(text).count();
    let last = lines(text).last().map_or(0.0, line_width_em);
    [last * size.max(0.0), (n.saturating_sub(1)) as f32 * size.max(0.0) * LINE_HEIGHT]
}

/// The outline of the box holding `text` at `size`, with its top-left corner at `top_left`: the corners
/// clockwise from the top-left, closed.
pub fn outline(top_left: P, text: &str, size: f32) -> Vec<P> {
    let [w, h] = measure(text, size);
    let [x, y] = top_left;
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]
}

/// A bitmap of text: premultiplied RGBA, [`PAD`] pixels of margin on every side.
#[derive(Clone, Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The text drawn at `scale` pixels per point in `rgba`. `None` if there is no font, nothing to
/// draw, or the size is nonsense. The scale is reduced if the bitmap would be too big.
pub fn raster(text: &str, size: f32, scale: f32, rgba: [u8; 4]) -> Option<Bitmap> {
    let face = FACE.as_ref()?;
    if !(size.is_finite() && scale.is_finite() && size > 0.0 && scale > 0.0) || text.chars().all(|c| c.is_whitespace()) {
        return None;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_a_font_on_this_machine() {
        assert!(available(), "no system font was found in the Windows fonts folder");
    }

    #[test]
    fn text_is_measured_by_its_widest_line_and_the_number_of_lines() {
        let [w1, h1] = measure("Hello", 20.0);
        let [w2, h2] = measure("Hello world", 20.0);
        assert!(w2 > w1 * 1.5, "more text is wider: {w1} {w2}");
        assert_eq!(h1, 20.0 * LINE_HEIGHT);
        assert_eq!(h1, h2);
        let [w3, h3] = measure("Hello\nHello world\nHi", 20.0);
        assert!((w3 - w2).abs() < 1e-3 && (h3 - 3.0 * h1).abs() < 1e-3, "the widest line sets the width: {w3} vs {w2}");
        // Twice the size, twice the box.
        let [wd, hd] = measure("Hello", 40.0);
        assert!((wd - 2.0 * w1).abs() < 1e-2 && (hd - 2.0 * h1).abs() < 1e-3);
        // Nothing typed still has a box to tap.
        let [we, he] = measure("", 20.0);
        assert!(we > 5.0 && he == h1);
    }

    fn widest(text: &str, size: f32) -> f32 {
        measure(text, size)[0]
    }

    #[test]
    fn wrapping_breaks_long_lines_at_spaces_and_leaves_short_ones_alone() {
        let size = 20.0;
        let words = "the quick brown fox jumps over the lazy dog";
        let limit = 150.0;
        let wrapped = wrap(words, size, limit);
        assert!(wrapped.contains('\n'), "{wrapped:?}");
        for line in wrapped.lines() {
            assert!(widest(line.trim_end(), size) <= limit + 0.01, "{line:?} is too wide");
        }
        // Taking the breaks away gives the words back exactly: the spaces stayed.
        assert_eq!(wrapped.replace('\n', ""), words);
        // Nothing to do for text that fits, and wrapping again changes nothing.
        assert_eq!(wrap("short", size, limit), "short");
        assert_eq!(wrap(&wrapped, size, limit), wrapped);
        // Lines already broken by the person stay as they are.
        assert_eq!(wrap("a\n\nb", size, limit), "a\n\nb");
        assert_eq!(wrap("", size, limit), "");
    }

    #[test]
    fn a_word_too_long_for_a_line_is_broken_inside_and_nothing_is_lost() {
        let word = "W".repeat(40);
        let wrapped = wrap(&word, 20.0, 100.0);
        assert!(wrapped.lines().count() > 3);
        assert!(wrapped.lines().all(|l| widest(l, 20.0) <= 100.0 + 0.01));
        assert_eq!(wrapped.replace('\n', ""), word);
        // A limit narrower than a letter still ends, one letter to a line.
        let narrow = wrap("abc", 20.0, 1.0);
        assert_eq!(narrow, "a\nb\nc");
    }

    #[test]
    fn wrapping_never_loops_or_panics_whatever_it_is_given() {
        let text = "héllo wörld ✓ 日本語   spaced   out \u{1f600} end";
        for size in [f32::NAN, f32::INFINITY, -1.0, 0.0, 1.0, 20.0, 400.0] {
            for max in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0, 0.001, 1.0, 50.0, 1.0e9] {
                let w = wrap(text, size, max);
                assert!(w.chars().count() <= MAX_CHARS);
                // Only breaks are added, and spaces never taken away.
                assert_eq!(w.replace('\n', ""), text.replace('\n', ""), "{size} {max}");
            }
        }
        let huge = "x ".repeat(MAX_CHARS);
        assert!(wrap(&huge, 10.0, 100.0).chars().count() <= MAX_CHARS);
    }

    #[test]
    fn letters_are_about_as_wide_as_helvetica_so_other_readers_agree() {
        // In a 1000 unit em Helvetica's "n" is 556 wide, a space 278, "W" 944.
        let em = |c| advance(c) * 1000.0;
        for (c, want) in [('n', 556.0), (' ', 278.0), ('W', 944.0), ('i', 222.0)] {
            assert!((em(c) - want).abs() < 6.0, "{c:?} is {} wide, expected about {want}", em(c));
        }
    }

    #[test]
    fn the_caret_is_at_the_end_of_the_last_line() {
        let [x, y] = caret("ab\ncde", 20.0);
        assert!((x - measure("cde", 20.0)[0]).abs() < 1e-3 && (y - 20.0 * LINE_HEIGHT).abs() < 1e-3);
        assert_eq!(caret("", 20.0), [0.0, 0.0]);
        assert_eq!(caret("abc", 10.0)[1], 0.0);
        assert!(caret("abc\n", 10.0)[0] == 0.0 && caret("abc\n", 10.0)[1] > 0.0, "a new empty line");
    }

    #[test]
    fn typed_characters_are_cleaned_and_capped() {
        assert_eq!(clean("a\tb\u{7}c\nd\r"), "a bc\nd");
        assert_eq!(clean(&"x".repeat(MAX_CHARS + 50)).chars().count(), MAX_CHARS);
    }

    #[test]
    fn text_is_drawn_into_a_bitmap_that_is_premultiplied_and_the_colour_asked_for() {
        let bm = raster("Hello", 24.0, 2.0, [200, 40, 20, 255]).expect("a bitmap");
        let [w, h] = measure("Hello", 24.0);
        assert_eq!(bm.width, (w * 2.0 + 2.0 * PAD).ceil() as u32);
        assert_eq!(bm.height, (h * 2.0 + 2.0 * PAD).ceil() as u32);
        assert_eq!(bm.pixels.len(), (bm.width * bm.height * 4) as usize);
        let inked: Vec<&[u8]> = bm.pixels.chunks(4).filter(|p| p[3] > 200).collect();
        assert!(inked.len() > 100, "{} solid pixels", inked.len());
        // Fully covered pixels are the colour; partly covered ones are the colour scaled (premultiplied).
        assert!(
            inked.iter().all(|p| (i32::from(p[0]) - 200).abs() <= 2 && (i32::from(p[1]) - 40).abs() <= 2 && (i32::from(p[2]) - 20).abs() <= 2)
                || inked.iter().any(|p| p[0] > 190)
        );
        for p in bm.pixels.chunks(4) {
            assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "premultiplied: {p:?}");
        }
        // The margin is empty.
        assert!(bm.pixels[..(bm.width * 4) as usize].iter().all(|b| *b == 0));
    }

    #[test]
    fn the_ink_is_where_the_letters_are() {
        // An "I" is a vertical bar near the left; it must cover the middle of its first column band.
        let bm = raster("I", 40.0, 1.0, [0, 0, 0, 255]).expect("bitmap");
        let (w, h) = (bm.width as usize, bm.height as usize);
        let column = |x: usize| (0..h).filter(|&y| bm.pixels[(y * w + x) * 4 + 3] > 128).count();
        let busiest = (0..w).map(column).max().unwrap_or(0);
        assert!(busiest as f32 > 0.5 * 40.0, "a tall bar: {busiest} rows");
        // Bigger text is bigger and blank text is nothing.
        assert!(raster("I", 80.0, 1.0, [0; 4]).expect("bitmap").height > bm.height);
        assert!(raster("   ", 20.0, 1.0, [0, 0, 0, 255]).is_none());
        assert!(raster("", 20.0, 1.0, [0, 0, 0, 255]).is_none());
    }

    #[test]
    fn every_line_is_drawn_in_its_own_place() {
        let bm = raster("I\nI", 30.0, 1.0, [0, 0, 0, 255]).expect("bitmap");
        let (w, h) = (bm.width as usize, bm.height as usize);
        let rows: Vec<bool> = (0..h).map(|y| (0..w).any(|x| bm.pixels[(y * w + x) * 4 + 3] > 128)).collect();
        // Two runs of inked rows with a gap between them.
        let runs = rows.windows(2).filter(|p| !p[0] && p[1]).count() + usize::from(rows[0]);
        assert_eq!(runs, 2, "two separate lines");
    }

    #[test]
    fn nonsense_and_enormous_requests_are_refused_or_bounded() {
        for (size, scale) in [(f32::NAN, 1.0), (10.0, f32::NAN), (-5.0, 1.0), (0.0, 1.0), (10.0, 0.0), (f32::INFINITY, 1.0)] {
            assert!(raster("x", size, scale, [0, 0, 0, 255]).is_none(), "{size} {scale}");
        }
        let huge = raster("Wide text goes here", 500.0, 20.0, [0, 0, 0, 255]).expect("bitmap");
        assert!(huge.width <= 4096 && huge.height <= 4096, "{}x{}", huge.width, huge.height);
        let long = raster(&"m".repeat(MAX_CHARS), 12.0, 1.0, [0, 0, 0, 255]).expect("bitmap");
        assert!(long.width <= 4096);
        // Characters the font has no letter for do not break it.
        assert!(raster("\u{10FFFF}é€漢", 20.0, 1.0, [0, 0, 0, 255]).is_some());
    }
}
