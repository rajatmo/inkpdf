//! The text of a page, as the tools and the editor see it: a trait over what a PDF engine knows (where each glyph is, which words and
//! lines they make, in which order they read), and no engine type. Coordinates are **page view space**: points, the origin at the
//! top-left of the page as displayed (rotation applied), y down. The engine side is `pdf` (the adapter that implements this).

use std::ops::Range;

/// One page's text. Glyphs are numbered in reading order; a range of numbers is a run of text.
pub trait PageTextView: Send + Sync {
    /// How many glyphs the page has.
    fn glyph_count(&self) -> usize;

    /// Whether the page has no text at all (a scan, or a blank page).
    fn is_empty(&self) -> bool {
        self.glyph_count() == 0
    }

    /// The glyph nearest to a point, if the page has any.
    fn nearest(&self, x: f32, y: f32) -> Option<usize>;

    /// The word the glyph is in, as its first and last glyph (both included).
    fn word_at(&self, glyph: usize) -> Option<(usize, usize)>;

    /// The line the glyph is on, as its first and last glyph (both included).
    fn line_at(&self, glyph: usize) -> Option<(usize, usize)>;

    /// The boxes (`[x0, y0, x1, y1]`) that cover a range of glyphs, one per line, to draw a selection with.
    fn line_rects(&self, range: Range<usize>) -> Vec<[f32; 4]>;

    /// The box of each of the glyphs.
    fn glyph_rects(&self, glyphs: &[usize]) -> Vec<[f32; 4]>;

    /// The glyphs that lie in a rectangle (`[x0, y0, x1, y1]`).
    #[allow(dead_code, reason = "a marquee over text is not built; the conformance suite checks it")]
    fn glyphs_in(&self, rect: [f32; 4]) -> Vec<usize>;

    /// The text of some glyphs, in reading order, with the spaces and line breaks a reader would put between them.
    fn column_text(&self, glyphs: &[usize]) -> String;

    /// The text of a range of glyphs.
    fn text_of(&self, range: Range<usize>) -> String;

    /// The whole page's text.
    #[allow(dead_code, reason = "the search card reads it (X5)")]
    fn plain_text(&self) -> String;

    /// Where `needle` is, as ranges of glyphs: at most `limit` of them, matched with or without case, and as whole words or not.
    fn find(&self, needle: &str, case_sensitive: bool, whole_words: bool, limit: usize) -> Vec<Range<usize>>;
}

/// A line of context round a hit, for a list of results: the text of the range with up to `context` glyphs on each side, on one line
/// (line breaks become spaces), with the hit's own place in it (as character offsets, for drawing it stronger).
pub fn snippet(view: &dyn PageTextView, hit: Range<usize>, context: usize) -> (String, Range<usize>) {
    let n = view.glyph_count();
    if hit.start >= n || hit.start >= hit.end {
        return (String::new(), 0..0);
    }
    let (start, end) = (hit.start.min(n), hit.end.min(n));
    let from = start.saturating_sub(context);
    let to = (end + context).min(n);
    let flat = |text: String| text.replace(['\n', '\r'], " ");
    let text = flat(view.text_of(from..to));
    // (Where the hit begins: what precedes it and its first glyph, less that glyph: a line break before it counts.)
    let at = flat(view.text_of(from..start + 1)).chars().count().saturating_sub(1);
    let len = flat(view.text_of(start..end)).chars().count();
    (text, at..at + len)
}

/// A page of text laid out on a grid, for tests: every character is a glyph 10 points wide and 16 high, line `i` is at `y = 20 i`, and
/// `column_text` and `text_of` give the characters back with a line break between lines. Reading order is the order of the characters.
#[cfg(test)]
pub struct FakeText {
    lines: Vec<Vec<char>>,
    /// The first glyph of each line.
    starts: Vec<usize>,
}

#[cfg(test)]
impl FakeText {
    pub fn new(lines: &[&str]) -> Self {
        let lines: Vec<Vec<char>> = lines.iter().map(|l| l.chars().collect()).collect();
        let mut starts = Vec::new();
        let mut n = 0;
        for l in &lines {
            starts.push(n);
            n += l.len();
        }
        Self { lines, starts }
    }

    fn at(&self, glyph: usize) -> Option<(usize, usize)> {
        let line = self.starts.iter().rposition(|&s| s <= glyph)?;
        let col = glyph - self.starts[line];
        (col < self.lines[line].len()).then_some((line, col))
    }

    fn rect(&self, glyph: usize) -> [f32; 4] {
        let (line, col) = self.at(glyph).unwrap_or((0, 0));
        [col as f32 * 10.0, line as f32 * 20.0, col as f32 * 10.0 + 10.0, line as f32 * 20.0 + 16.0]
    }

    fn chars_of(&self, range: Range<usize>) -> String {
        let mut out = String::new();
        let mut last_line = None;
        for g in range {
            let Some((line, col)) = self.at(g) else { continue };
            if last_line.is_some_and(|l| l != line) {
                out.push('\n');
            }
            last_line = Some(line);
            out.push(self.lines[line][col]);
        }
        out
    }
}

#[cfg(test)]
impl PageTextView for FakeText {
    fn glyph_count(&self) -> usize {
        self.lines.iter().map(Vec::len).sum()
    }

    fn nearest(&self, x: f32, y: f32) -> Option<usize> {
        let n = self.glyph_count();
        (0..n).min_by(|&a, &b| {
            let d = |g: usize| {
                let r = self.rect(g);
                let (dx, dy) = ((r[0] + r[2]) / 2.0 - x, (r[1] + r[3]) / 2.0 - y);
                dx * dx + dy * dy
            };
            d(a).total_cmp(&d(b))
        })
    }

    fn word_at(&self, glyph: usize) -> Option<(usize, usize)> {
        let (line, col) = self.at(glyph)?;
        let l = &self.lines[line];
        let (mut a, mut z) = (col, col);
        while a > 0 && l[a - 1] != ' ' {
            a -= 1;
        }
        while z + 1 < l.len() && l[z + 1] != ' ' {
            z += 1;
        }
        Some((self.starts[line] + a, self.starts[line] + z))
    }

    fn line_at(&self, glyph: usize) -> Option<(usize, usize)> {
        let (line, _) = self.at(glyph)?;
        Some((self.starts[line], self.starts[line] + self.lines[line].len() - 1))
    }

    fn line_rects(&self, range: Range<usize>) -> Vec<[f32; 4]> {
        let mut out: Vec<[f32; 4]> = Vec::new();
        let mut last = None;
        for g in range.filter(|&g| self.at(g).is_some()) {
            let (line, _) = self.at(g).unwrap_or((0, 0));
            let r = self.rect(g);
            match (&mut out.last_mut(), last) {
                (Some(box_), Some(l)) if l == line => {
                    box_[0] = box_[0].min(r[0]);
                    box_[2] = box_[2].max(r[2]);
                }
                _ => out.push(r),
            }
            last = Some(line);
        }
        out
    }

    fn glyph_rects(&self, glyphs: &[usize]) -> Vec<[f32; 4]> {
        glyphs.iter().map(|&g| self.rect(g)).collect()
    }

    fn glyphs_in(&self, rect: [f32; 4]) -> Vec<usize> {
        (0..self.glyph_count())
            .filter(|&g| {
                let r = self.rect(g);
                r[0] < rect[2] && r[2] > rect[0] && r[1] < rect[3] && r[3] > rect[1]
            })
            .collect()
    }

    fn column_text(&self, glyphs: &[usize]) -> String {
        let mut sorted = glyphs.to_vec();
        sorted.sort_unstable();
        let mut out = String::new();
        let mut last = None;
        for g in sorted {
            let Some((line, col)) = self.at(g) else { continue };
            if last.is_some_and(|l| l != line) {
                out.push('\n');
            }
            last = Some(line);
            out.push(self.lines[line][col]);
        }
        out
    }

    fn text_of(&self, range: Range<usize>) -> String {
        self.chars_of(range)
    }

    fn plain_text(&self) -> String {
        self.chars_of(0..self.glyph_count())
    }

    fn find(&self, needle: &str, case_sensitive: bool, whole_words: bool, limit: usize) -> Vec<Range<usize>> {
        let fold = |c: char| if case_sensitive { c } else { c.to_lowercase().next().unwrap_or(c) };
        let needle: Vec<char> = needle.chars().map(fold).collect();
        let mut out = Vec::new();
        if needle.is_empty() {
            return out;
        }
        for (line, chars) in self.lines.iter().enumerate() {
            let hay: Vec<char> = chars.iter().copied().map(fold).collect();
            let mut i = 0;
            while i + needle.len() <= hay.len() && out.len() < limit {
                if hay[i..i + needle.len()] == needle[..] {
                    let edge = |j: Option<usize>| j.is_none_or(|j| !chars[j].is_alphanumeric());
                    let whole = edge(i.checked_sub(1)) && edge((i + needle.len() < chars.len()).then_some(i + needle.len()));
                    if !whole_words || whole {
                        out.push(self.starts[line] + i..self.starts[line] + i + needle.len());
                        i += needle.len();
                        continue;
                    }
                }
                i += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> FakeText {
        FakeText::new(&["Hello world", "The quick brown fox", "Hello again"])
    }

    #[test]
    fn the_fake_page_answers_like_a_real_one() {
        let t = page();
        assert_eq!(t.glyph_count(), 11 + 19 + 11);
        assert_eq!(t.word_at(0).map(|(a, z)| t.text_of(a..z + 1)), Some("Hello".into()));
        assert_eq!(t.line_at(15).map(|(a, z)| t.text_of(a..z + 1)), Some("The quick brown fox".into()));
        assert_eq!(t.nearest(3.0, 8.0), Some(0));
        assert_eq!(t.line_rects(5..16).len(), 2, "a range over a line break is two boxes");
        assert_eq!(t.plain_text(), "Hello world\nThe quick brown fox\nHello again");
        assert_eq!(t.find("hello", false, false, 10), vec![0..5, 30..35]);
        assert!(t.find("hello", true, false, 10).is_empty());
        assert_eq!(t.find("hello", false, false, 1).len(), 1, "up to the limit");
        assert!(t.find("row", false, true, 10).is_empty() && t.find("brown", false, true, 10).len() == 1);
        assert!(t.find("", false, false, 10).is_empty());
    }

    #[test]
    fn a_snippet_is_the_hit_with_its_surroundings_on_one_line_and_says_where_the_hit_is() {
        let t = page();
        let (text, at) = snippet(&t, 6..11, 4);
        assert_eq!(text, "llo world The ");
        assert_eq!(text.chars().skip(at.start).take(at.len()).collect::<String>(), "world");
        // Over a line break, and at the ends of the page: never out of range.
        let (text, _) = snippet(&t, 9..13, 2);
        assert!(!text.contains('\n'));
        assert_eq!(snippet(&t, 0..3, 50).0.chars().next(), Some('H'));
        assert_eq!(snippet(&t, 100..200, 5), (String::new(), 0..0));
    }
}
