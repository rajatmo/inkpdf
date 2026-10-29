//! Typed text as numbers: what to keep of it, how wide and tall it is, where the caret goes and what a press hits, how it wraps and
//! how a selection is covered. Nothing is drawn here.

use crate::plane::P;

use super::font::advance;
use super::{LINE_HEIGHT, MAX_CHARS, MIN_EM};

/// The characters to show for typed text: tabs become spaces, other control characters go, and it
/// stops at [`MAX_CHARS`].
pub fn clean(text: &str) -> String {
    text.chars().map(|c| if c == '\t' { ' ' } else { c }).filter(|c| *c == '\n' || !c.is_control()).take(MAX_CHARS).collect()
}

/// The lines of `text`.
pub(super) fn lines(text: &str) -> impl Iterator<Item = &str> {
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
