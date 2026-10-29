//! Typed text: measuring it and drawing it into a bitmap.
//!
//! The font is Arial where the system has it (see [`set_font`]), else Liberation Sans, which is built into the program and whose
//! letters are exactly as wide as Arial's (and so as Helvetica's, which is what other PDF readers use for the text this app
//! saves): text measures the same on every machine, and nothing here reads a file. Its outlines are read with `skrifa` and
//! filled by a small anti-aliased scanline rasterizer, so text is sharp at any zoom.
//!
//! Sizes are in points; a line is [`LINE_HEIGHT`] times the size tall; `\n` starts a new line.

mod font;
mod layout;
mod raster;

pub use font::set_font;
pub use layout::{caret_at, clean, fit_box, follow_wrap, index_at, line_range, measure, outline, selection_rects, vertical, wrap};
pub use raster::raster;

/// How tall a line is, as a multiple of the font size.
pub const LINE_HEIGHT: f32 = 1.25;
/// The fewest characters wide an empty box is measured as, so there is something to tap.
const MIN_EM: f32 = 0.6;
/// Most characters a text keeps, so a stray paste cannot make an enormous bitmap.
pub const MAX_CHARS: usize = 4_000;
/// The margin around the box in a bitmap, in pixels, so anti-aliased edges are not cut off.
pub const PAD: f32 = 2.0;

#[cfg(test)]
mod tests {
    use super::font::advance;
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
