//! The text of a page, as the tools and the editor see it: a trait over what a PDF engine knows (where each glyph is, which words and
//! lines they make, in which order they read), and no engine type. Coordinates are **page view space**: points, the origin at the
//! top-left of the page as displayed (rotation applied), y down. The engine side is `pdf` (the adapter that implements this).

use std::ops::Range;

#[allow(dead_code, reason = "the text tools (the select tool, the search card) and the conformance suite are its users")]
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

    /// The word the glyph is in, as a range of glyph numbers (`start`, `end` exclusive).
    fn word_at(&self, glyph: usize) -> Option<(usize, usize)>;

    /// The line the glyph is on, as a range of glyph numbers (`start`, `end` exclusive).
    fn line_at(&self, glyph: usize) -> Option<(usize, usize)>;

    /// The boxes (`[x0, y0, x1, y1]`) that cover a range of glyphs, one per line, to draw a selection with.
    fn line_rects(&self, range: Range<usize>) -> Vec<[f32; 4]>;

    /// The box of each of the glyphs.
    fn glyph_rects(&self, glyphs: &[usize]) -> Vec<[f32; 4]>;

    /// The glyphs that lie in a rectangle (`[x0, y0, x1, y1]`).
    fn glyphs_in(&self, rect: [f32; 4]) -> Vec<usize>;

    /// The text of some glyphs, in reading order, with the spaces and line breaks a reader would put between them.
    fn column_text(&self, glyphs: &[usize]) -> String;

    /// The text of a range of glyphs.
    fn text_of(&self, range: Range<usize>) -> String;

    /// The whole page's text.
    fn plain_text(&self) -> String;

    /// Where `needle` is, as ranges of glyphs: at most `limit` of them, matched with or without case, and as whole words or not.
    fn find(&self, needle: &str, case_sensitive: bool, whole_words: bool, limit: usize) -> Vec<Range<usize>>;
}
