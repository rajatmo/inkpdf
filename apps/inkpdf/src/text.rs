//! Typed text: measuring it and drawing it into a bitmap.
//!
//! The font is Arial where the system has it (see [`set_font`]), else Liberation Sans, which is built into the program and whose
//! letters are exactly as wide as Arial's (and so as Helvetica's, which is what other PDF readers use for the text this app
//! saves): text measures the same on every machine, and nothing here reads a file. Its outlines are read with `skrifa` and
//! filled by a small anti-aliased scanline rasterizer, so text is sharp at any zoom.
//!
//! Sizes are in points; a line is [`LINE_HEIGHT`] times the size tall; `\n` starts a new line.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::*;

use crate::plane::P;

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
/// Liberation Sans Regular (SIL Open Font License; see `assets/fonts/LICENSE-LiberationFonts.txt`), the font when the system gives none.
const BUNDLED_FONT: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");

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

/// The font the system offered, if any (see [`set_font`]).
static PREFERRED: OnceLock<&'static [u8]> = OnceLock::new();

/// Offer a font to use instead of the built-in one, as the bytes of a TrueType file (the shell finds it with the platform's help).
/// Heard only before any text is first measured or drawn, and only if it can be read; returns whether it will be used.
pub fn set_font(bytes: Vec<u8>) -> bool {
    if FACE.get().is_some() || FontRef::new(&bytes).is_err() {
        return false;
    }
    // The bytes live as long as the program: the parser borrows them.
    PREFERRED.set(Box::leak(bytes.into_boxed_slice())).is_ok()
}

fn face_from(bytes: &'static [u8]) -> Option<Face> {
    let font = FontRef::new(bytes).ok()?;
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(metrics.units_per_em).max(1.0);
    Some(Face { font, upem, ascent: metrics.ascent / upem, descent: -metrics.descent / upem, glyphs: Mutex::new(HashMap::new()) })
}

static FACE: OnceLock<Face> = OnceLock::new();

fn face() -> &'static Face {
    FACE.get_or_init(|| {
        PREFERRED.get().and_then(|bytes| face_from(bytes)).or_else(|| face_from(BUNDLED_FONT)).expect("the built-in font is a valid font")
    })
}

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

/// A character's advance in ems.
fn advance(ch: char) -> f32 {
    face().glyph(ch).advance
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

/// The line and the place in it (both counted from 0, in characters) that character `index` is at.
fn locate(text: &str, index: usize) -> (usize, usize) {
    let (mut row, mut col) = (0, 0);
    for c in text.chars().take(index) {
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (row, col)
}

/// Where the caret is when it is before character `index` of `text` (counted in characters; the end of the text
/// if `index` is past it), from the box's top-left: along its line, and the top of that line.
pub fn caret_at(text: &str, size: f32, index: usize) -> P {
    let size = size.max(0.0);
    let (row, col) = locate(text, index);
    let line = lines(text).nth(row).unwrap_or("");
    [line.chars().take(col).map(advance).sum::<f32>() * size, row as f32 * size * LINE_HEIGHT]
}

/// The characters before the start of line `row` of `text`.
fn line_start(text: &str, row: usize) -> usize {
    lines(text).take(row).map(|l| l.chars().count() + 1).sum()
}

/// Which character a point at `at` (from the box's top-left) is nearest to the front of, as a position for the
/// caret: the line it is on (above the first or below the last, those), and the gap between letters it is nearest.
pub fn index_at(text: &str, size: f32, at: P) -> usize {
    let size = size.max(0.01);
    let row = ((at[1] / (size * LINE_HEIGHT)).floor().max(0.0) as usize).min(lines(text).count().saturating_sub(1));
    let line = lines(text).nth(row).unwrap_or("");
    let target = at[0] / size;
    let (mut x, mut col) = (0.0, 0);
    for c in line.chars() {
        let w = advance(c);
        if x + w / 2.0 > target {
            break;
        }
        x += w;
        col += 1;
    }
    line_start(text, row) + col
}

/// The first and last position of the line that has the caret at `index`.
pub fn line_range(text: &str, index: usize) -> (usize, usize) {
    let (row, _) = locate(text, index);
    let start = line_start(text, row);
    (start, start + lines(text).nth(row).map_or(0, |l| l.chars().count()))
}

/// The position after moving the caret from `index` up or down a line, keeping to `goal_x` (points from the box's
/// left) as far as the other line goes. From the first line up it goes to the start; from the last line down, to
/// the end.
pub fn vertical(text: &str, size: f32, index: usize, down: bool, goal_x: f32) -> usize {
    let (row, _) = locate(text, index);
    let last = lines(text).count().saturating_sub(1);
    match (down, row) {
        (false, 0) => 0,
        (true, r) if r >= last => text.chars().count(),
        _ => {
            let to = if down { row + 1 } else { row - 1 };
            index_at(text, size, [goal_x, (to as f32 + 0.5) * size.max(0.01) * LINE_HEIGHT])
        }
    }
}

/// Where the caret goes after `new` was made from `old` by [`wrap`] (which only breaks lines, so the characters
/// are the same with some `\n` put in): the same place among the characters.
pub fn follow_wrap(old: &str, new: &str, index: usize) -> usize {
    let (old, new): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let (mut i, mut j) = (0, 0);
    while i < index.min(old.len()) && j < new.len() {
        if new[j] == old[i] {
            i += 1;
            j += 1;
        } else if new[j] == '\n' {
            j += 1;
        } else {
            break;
        }
    }
    j
}

/// The rectangles, `[x0, y0, x1, y1]` from the box's top-left, that cover characters `from` up to `to`: one for
/// each line they are on (a line break in them adds a little at the end of its line).
pub fn selection_rects(text: &str, size: f32, from: usize, to: usize) -> Vec<[f32; 4]> {
    let size = size.max(0.0);
    let mut out = Vec::new();
    for (row, line) in lines(text).enumerate() {
        let (start, len) = (line_start(text, row), line.chars().count());
        let (lo, hi) = (from.max(start), to.min(start + len));
        let breaks = from <= start + len && to > start + len && row + 1 < lines(text).count();
        if lo >= hi && !breaks {
            continue;
        }
        let along = |n: usize| line.chars().take(n).map(advance).sum::<f32>() * size;
        let x0 = along(lo.saturating_sub(start).min(len));
        let x1 = along(hi.saturating_sub(start).min(len)) + if breaks { 0.3 * size } else { 0.0 };
        out.push([x0, row as f32 * size * LINE_HEIGHT, x1.max(x0), (row as f32 + 1.0) * size * LINE_HEIGHT]);
    }
    out
}

/// The outline of the box holding `text` at `size`, with its top-left corner at `top_left`: the corners
/// clockwise from the top-left, closed.
pub fn outline(top_left: P, text: &str, size: f32) -> Vec<P> {
    let [w, h] = measure(text, size);
    let [x, y] = top_left;
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]
}

/// A text box laid out to sit inside the page `rect` (`[x0, y0, x1, y1]`, or no page to go by): its lines
/// broken where they would run off the right edge (a box too near the edge for that to leave a readable
/// width is moved left), and the whole box moved just far enough to lie inside. Returns the text as broken
/// and the box's outline.
pub fn fit_box(rect: Option<[f32; 4]>, top_left: P, text: &str, size: f32) -> (String, Vec<P>) {
    let Some(rect) = rect else { return (text.to_string(), outline(top_left, text, size)) };
    let page_width = rect[2] - rect[0];
    let least = (0.35 * page_width).min(14.0 * size);
    let left = top_left[0].min(rect[2] - least).max(rect[0]);
    let wrapped = wrap(text, size, rect[2] - left);
    let mut corners = outline([left, top_left[1]], &wrapped, size);
    if let (Some(first), Some(far)) = (corners.first().copied(), corners.get(2).copied()) {
        let dx = (rect[2] - far[0]).min(0.0).max(rect[0] - first[0]);
        let dy = (rect[3] - far[1]).min(0.0).max(rect[1] - first[1]);
        corners.iter_mut().for_each(|p| *p = [p[0] + dx, p[1] + dy]);
    }
    (wrapped, corners)
}

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
    fn the_caret_is_where_the_characters_before_it_end_and_a_press_finds_it_again() {
        let text = "Hello\nwide world";
        let size = 20.0;
        // At the front, in the middle of a line, at the end of one, on the next line.
        assert_eq!(caret_at(text, size, 0), [0.0, 0.0]);
        let (a, b) = (caret_at(text, size, 2), caret_at(text, size, 5));
        assert!(a[0] > 0.0 && b[0] > a[0] && a[1] == 0.0 && b[1] == 0.0);
        assert_eq!(caret_at(text, size, 6), [0.0, size * LINE_HEIGHT], "just after the line break: the start of the next line");
        let end = caret_at(text, size, 1000);
        assert_eq!(end, caret_at(text, size, text.chars().count()), "past the end is the end");
        assert_eq!(end[1], size * LINE_HEIGHT, "on the last line");
        assert!((end[0] - "wide world".chars().map(advance).sum::<f32>() * size).abs() < 1e-3);
        // A press at a caret's place finds the same index, nearer to either side of a letter picks that side.
        for index in 0..=text.chars().count() {
            let at = caret_at(text, size, index);
            assert_eq!(index_at(text, size, [at[0], at[1] + 3.0]), index, "index {index}");
        }
        // Above the box is the first line, below it the last, far right the end of the line.
        assert_eq!(index_at(text, size, [0.0, -50.0]), 0);
        assert_eq!(index_at(text, size, [1.0e6, 1.0e6]), text.chars().count());
        assert_eq!(index_at(text, size, [1.0e6, 3.0]), 5);
    }

    #[test]
    fn moving_up_and_down_keeps_to_the_column_and_stops_at_the_ends() {
        let text = "abcdef\nab\nabcdef";
        let size = 20.0;
        let x = caret_at(text, size, 5)[0];
        // Down from the fifth place of the first line: the short line's end, then the fifth place again.
        let short = vertical(text, size, 5, true, x);
        assert_eq!(short, 9, "the short line ends sooner");
        let third = vertical(text, size, short, true, x);
        assert_eq!(third, 10 + 5);
        // Past the last line, the end; before the first, the start.
        assert_eq!(vertical(text, size, third, true, x), text.chars().count());
        assert_eq!(vertical(text, size, 2, false, x), 0);
        assert_eq!(vertical(text, size, third, false, x), 7 + 2, "back up to the short line's end");
        assert_eq!(line_range(text, 8), (7, 9));
        assert_eq!(line_range(text, 0), (0, 6));
        assert_eq!(line_range(text, 1000), (10, 16));
    }

    #[test]
    fn a_wrap_that_breaks_lines_keeps_the_caret_among_the_same_characters() {
        let old = "aaa bbb ccc";
        let new = "aaa \nbbb \nccc";
        assert_eq!(follow_wrap(old, new, 0), 0);
        assert_eq!(follow_wrap(old, new, 4), 4, "before the first break there is nothing to skip");
        assert_eq!(follow_wrap(old, new, 5), 6, "one past the first break");
        assert_eq!(follow_wrap(old, new, 11), 13);
        assert_eq!(follow_wrap(old, old, 7), 7, "nothing broken, nothing moves");
        assert_eq!(follow_wrap("", "", 3), 0);
    }

    #[test]
    fn a_selection_is_a_rectangle_for_every_line_it_touches() {
        let text = "abc\ndef\nghi";
        let size = 10.0;
        let lh = size * LINE_HEIGHT;
        // Inside one line.
        let one = selection_rects(text, size, 1, 3);
        assert_eq!(one.len(), 1);
        assert!(one[0][0] > 0.0 && one[0][2] > one[0][0] && one[0][1] == 0.0 && one[0][3] == lh);
        // Across the whole text: three lines, the first two a little longer for their breaks.
        let all = selection_rects(text, size, 0, text.chars().count());
        assert_eq!(all.len(), 3);
        let width = |line: &str| line.chars().map(advance).sum::<f32>() * size;
        assert!((all[0][2] - width("abc") - 0.3 * size).abs() < 0.01);
        assert!((all[2][2] - width("ghi")).abs() < 0.01, "the last line has no break");
        // Nothing selected, nothing to draw.
        assert!(selection_rects(text, size, 2, 2).is_empty());
    }

    #[test]
    fn the_built_in_font_measures_like_arial_and_draws() {
        // Arial's "Hello" is 0.722 + 0.556 + 0.222 + 0.222 + 0.556 ems wide; Liberation Sans keeps those widths.
        let w = measure("Hello", 1000.0)[0];
        assert!((w - 2278.0).abs() < 20.0, "{w}");
        assert!(raster("Hello", 12.0, 2.0, [0, 0, 0, 255]).is_some());
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
    fn the_caret_past_the_last_character_is_at_the_end_of_the_last_line() {
        let end = |text: &str, size: f32| caret_at(text, size, text.chars().count());
        let [x, y] = end("ab\ncde", 20.0);
        assert!((x - measure("cde", 20.0)[0]).abs() < 1e-3 && (y - 20.0 * LINE_HEIGHT).abs() < 1e-3);
        assert_eq!(end("", 20.0), [0.0, 0.0]);
        assert_eq!(end("abc", 10.0)[1], 0.0);
        assert!(end("abc\n", 10.0)[0] == 0.0 && end("abc\n", 10.0)[1] > 0.0, "a new empty line");
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
