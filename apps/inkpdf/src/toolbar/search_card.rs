//! The search card: a field to type in, what to match (the case, whole words), "3 of 17" with a button for the one before and the
//! one after, a close button, and under them the places found, each with a line of the text round it (the current one is lit; a press on
//! one goes to it). It floats at the right, under the bars, so the page stays in view. Plain data, as the other cards: the editor holds the
//! state and builds a [`SearchView`], and the shell draws what this makes.

use std::ops::Range;

use crate::ink::rect_contains;
use crate::scene::Capsule;
use crate::text;

use super::Toolbar;
use super::icons::icons;
use super::theme::{Align, Label, Ui};

/// Rows of results the card shows at most.
pub const ROWS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchItem {
    /// The place the query is typed.
    Field,
    Case,
    Words,
    Previous,
    Next,
    Close,
    /// A row of the list (counted from the first one shown).
    Row(usize),
}

/// One row of the results.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchRow {
    pub page: u32,
    /// The text round the place found, on one line, and where the place is in it (in characters).
    pub snippet: String,
    pub at: Range<usize>,
    pub current: bool,
}

/// Everything the card's look depends on.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchView {
    pub query: String,
    /// The caret, as a byte index on a character of the query.
    pub caret: usize,
    pub case: bool,
    pub words: bool,
    /// How many places were found, and which is current (counted from 0).
    pub total: usize,
    pub current: usize,
    /// How many pages have been read, of how many (the rest are still being read).
    pub read: (usize, usize),
    pub rows: Vec<SearchRow>,
}

/// `text` cut short, with an ellipsis, so that it is no wider than `room` at `size`.
fn elide(text: &str, size: f32, room: f32) -> String {
    if text::measure(text, size)[0] <= room {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let shorter: String = chars.iter().chain(&['…']).collect();
        if text::measure(&shorter, size)[0] <= room {
            return shorter;
        }
    }
    String::new()
}

#[derive(Clone, Copy, Debug)]
pub struct SearchLayout {
    pub card: [f32; 4],
    field: [f32; 4],
    case: [f32; 4],
    words: [f32; 4],
    previous: [f32; 4],
    next: [f32; 4],
    close: [f32; 4],
    /// Where the line "3 of 17" goes: the middle of the space between the options and the arrows.
    status: [f32; 4],
    rows: [[f32; 4]; ROWS],
    row_count: usize,
    unit: f32,
    px: f32,
}

impl SearchLayout {
    /// The card in a window of `viewport` pixels, at the right, with its top `top` pixels down (just under the bars), with room for `rows`
    /// rows of results.
    pub fn new(toolbar: &Toolbar, viewport: [f32; 2], top: f32, rows: usize) -> SearchLayout {
        let px = toolbar.dpr;
        let u = toolbar.unit.clamp(20.0 * px, 34.0 * px);
        let margin = 12.0 * px;
        let width = (17.0 * u).min((viewport[0] - 2.0 * margin).max(8.0 * u));
        let x1 = (viewport[0] - margin).max(width);
        let x0 = x1 - width;
        let (pad, h, gap) = (0.3 * u, 1.1 * u, 0.2 * u);
        let y0 = top.max(0.0);
        let row_a = y0 + pad;
        let row_b = row_a + h + gap;
        let close = [x1 - pad - h, row_a, x1 - pad, row_a + h];
        let field = [x0 + pad, row_a, close[0] - gap, row_a + h];
        let case = [x0 + pad, row_b, x0 + pad + h, row_b + h];
        let words = [case[2] + gap, row_b, case[2] + gap + h, row_b + h];
        let next = [x1 - pad - h, row_b, x1 - pad, row_b + h];
        let previous = [next[0] - gap - h, row_b, next[0] - gap, row_b + h];
        let status = [words[2] + gap, row_b, previous[0] - gap, row_b + h];
        let row_count = rows.min(ROWS);
        let mut list = [[0.0; 4]; ROWS];
        let first = row_b + h + pad;
        let row_h = 0.95 * u;
        for (i, r) in list.iter_mut().enumerate().take(row_count) {
            *r = [x0 + pad, first + i as f32 * row_h, x1 - pad, first + (i + 1) as f32 * row_h];
        }
        let bottom = if row_count > 0 { list[row_count - 1][3] + pad } else { row_b + h + pad };
        SearchLayout { card: [x0, y0, x1, bottom], field, case, words, previous, next, close, status, rows: list, row_count, unit: u, px }
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        rect_contains(self.card, p)
    }

    pub fn hit(&self, p: [f32; 2]) -> Option<SearchItem> {
        let parts = [
            (self.close, SearchItem::Close),
            (self.field, SearchItem::Field),
            (self.case, SearchItem::Case),
            (self.words, SearchItem::Words),
            (self.previous, SearchItem::Previous),
            (self.next, SearchItem::Next),
        ];
        if let Some((_, item)) = parts.iter().find(|(r, _)| rect_contains(*r, p)) {
            return Some(*item);
        }
        self.rows[..self.row_count].iter().position(|r| rect_contains(*r, p)).map(SearchItem::Row)
    }

    /// The middle of a part, for tests that press it.
    #[cfg(test)]
    pub fn centre_of(&self, item: SearchItem) -> [f32; 2] {
        let r = match item {
            SearchItem::Field => self.field,
            SearchItem::Case => self.case,
            SearchItem::Words => self.words,
            SearchItem::Previous => self.previous,
            SearchItem::Next => self.next,
            SearchItem::Close => self.close,
            SearchItem::Row(i) => self.rows[i.min(ROWS - 1)],
        };
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    }

    pub fn draw(&self, view: &SearchView, ui: &mut Ui) {
        let (u, px) = (self.unit, self.px);
        ui.floating(self.card, 0.4 * u, px);
        let all = icons();
        let line = (u * 0.04).max(1.4 * px);
        let mid = |r: [f32; 4]| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
        let say = |ui: &mut Ui, text: &str, size: f32, color: [u8; 4], at: [f32; 2], align: Align| {
            if !text.is_empty() {
                ui.labels.push(Label { text: text.to_string(), size, color, at, align, alpha: 1.0 });
            }
        };
        // The field: what is typed (or what to type), and the caret.
        ui.panel(self.field, 0.3 * u, ui.pal.neutral_wash);
        let size = 0.5 * u;
        let room = (self.field[2] - self.field[0] - 0.5 * u).max(u);
        let y = mid(self.field)[1];
        let x0 = self.field[0] + 0.25 * u;
        if view.query.is_empty() {
            say(ui, "Search", size, ui.pal.ink_dim, [x0, y], Align::Left);
            ui.lines.push(Capsule::new([x0, y - 0.4 * u], [x0, y + 0.4 * u], 0.07 * u, ui.pal.accent));
        } else {
            let range = visible_name(&view.query, view.caret, |s| text::measure(s, size)[0] <= room);
            say(ui, &view.query[range.clone()], size, ui.pal.ink, [x0, y], Align::Left);
            let caret_x = x0 + text::measure(&view.query[range.start..view.caret.clamp(range.start, range.end)], size)[0];
            ui.lines.push(Capsule::new([caret_x, y - 0.4 * u], [caret_x, y + 0.4 * u], 0.07 * u, ui.pal.accent));
        }
        let [cx, cy] = mid(self.close);
        all.close.draw(&mut ui.lines, cx, cy, 0.3 * u, line, ui.pal.ink);
        // What to match.
        for (rect, icon, on) in [(self.case, &all.match_case, view.case), (self.words, &all.whole_word, view.words)] {
            if on {
                ui.panel(rect, 0.3 * u, ui.pal.accent_wash);
            }
            let [cx, cy] = mid(rect);
            icon.draw(&mut ui.lines, cx, cy, 0.34 * u, line, if on { ui.pal.accent } else { ui.pal.ink });
        }
        // How many, and the arrows.
        let found = if view.query.is_empty() {
            String::new()
        } else if view.total == 0 {
            "No results".to_string()
        } else {
            format!("{} of {}", view.current + 1, view.total)
        };
        let reading = if view.read.0 < view.read.1 && !view.query.is_empty() {
            format!("  (reading {} of {})", view.read.0, view.read.1)
        } else {
            String::new()
        };
        let status = format!("{found}{reading}");
        let fits = elide(&status, 0.44 * u, self.status[2] - self.status[0]);
        say(ui, &fits, 0.44 * u, ui.pal.ink_dim, [self.status[0], mid(self.status)[1]], Align::Left);
        let can = view.total > 0;
        for (rect, icon) in [(self.previous, &all.earlier), (self.next, &all.later)] {
            let [cx, cy] = mid(rect);
            icon.draw(&mut ui.lines, cx, cy, 0.3 * u, line, if can { ui.pal.ink } else { ui.pal.ink_dim });
        }
        // The places found.
        for (rect, row) in self.rows[..self.row_count].iter().zip(&view.rows) {
            if row.current {
                ui.panel(*rect, 0.25 * u, ui.pal.accent_wash);
            }
            let y = mid(*rect)[1];
            let label_x = rect[0] + 0.2 * u;
            say(ui, &format!("{}", row.page + 1), 0.4 * u, ui.pal.ink_dim, [label_x, y], Align::Left);
            let text_x = rect[0] + 1.5 * u;
            let room = (rect[2] - text_x - 0.2 * u).max(u);
            let size = 0.44 * u;
            let shown = elide(&row.snippet, size, room);
            // The place itself is marked behind its words (if it is among those shown).
            let before: String = row.snippet.chars().take(row.at.start).collect();
            let inside: String = row.snippet.chars().skip(row.at.start).take(row.at.len()).collect();
            let from = text::measure(&before, size)[0];
            let width = text::measure(&inside, size)[0];
            if from + width <= room && width > 0.0 {
                ui.panel([text_x + from, y - 0.34 * u, text_x + from + width, y + 0.34 * u], 0.12 * u, ui.pal.search_current);
            }
            say(ui, &shown, size, ui.pal.ink, [text_x, y], Align::Left);
        }
    }
}

/// The part of `name` to show so that the caret is in it and it fits (`fits` says whether a text is narrow enough): the
/// text before the caret loses its beginning first, and what is after it its end. Returns a range of bytes.
pub fn visible_name(name: &str, caret: usize, fits: impl Fn(&str) -> bool) -> std::ops::Range<usize> {
    let caret = caret.min(name.len());
    let mut start = 0;
    while start < caret && !fits(&name[start..caret]) {
        start += name[start..].chars().next().map_or(1, char::len_utf8);
    }
    let mut end = caret;
    while end < name.len() {
        let next = end + name[end..].chars().next().map_or(1, char::len_utf8);
        if !fits(&name[start..next]) {
            break;
        }
        end = next;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [([f32; 2], f32); 5] =
        [([1650.0, 1350.0], 1.5), ([1100.0, 900.0], 1.0), ([800.0, 600.0], 1.0), ([480.0, 800.0], 1.0), ([360.0, 640.0], 1.0)];

    fn layout(viewport: [f32; 2], dpr: f32, rows: usize) -> SearchLayout {
        SearchLayout::new(&Toolbar::layout(viewport, dpr), viewport, 90.0 * dpr, rows)
    }

    #[test]
    fn every_part_is_inside_the_card_inside_the_window_and_none_overlap() {
        for (viewport, dpr) in WINDOWS {
            let l = layout(viewport, dpr, ROWS);
            assert!(l.card[0] >= 0.0 && l.card[2] <= viewport[0] + 0.5, "the card is in the window at {viewport:?}: {:?}", l.card);
            let parts = [l.field, l.case, l.words, l.previous, l.next, l.close];
            for r in parts.iter().chain(&l.rows[..l.row_count]) {
                assert!(r[0] >= l.card[0] && r[2] <= l.card[2] && r[1] >= l.card[1] && r[3] <= l.card[3], "{r:?} in {:?} at {viewport:?}", l.card);
            }
            for (i, a) in parts.iter().enumerate() {
                for b in &parts[i + 1..] {
                    assert!(
                        a[2] <= b[0] + 0.01 || b[2] <= a[0] + 0.01 || a[3] <= b[1] + 0.01 || b[3] <= a[1] + 0.01,
                        "{a:?} overlaps {b:?} at {viewport:?}"
                    );
                }
            }
            // Big enough to press with a stylus tip.
            for r in [l.case, l.words, l.previous, l.next, l.close] {
                assert!(r[2] - r[0] >= 20.0 * dpr * 1.0 && r[3] - r[1] >= 20.0 * dpr, "{r:?} is small at {viewport:?}");
            }
        }
    }

    #[test]
    fn a_press_finds_the_part_it_is_on_and_only_what_is_there() {
        let l = layout([1100.0, 900.0], 1.0, 3);
        for item in [
            SearchItem::Field,
            SearchItem::Case,
            SearchItem::Words,
            SearchItem::Previous,
            SearchItem::Next,
            SearchItem::Close,
            SearchItem::Row(0),
            SearchItem::Row(2),
        ] {
            assert_eq!(l.hit(l.centre_of(item)), Some(item), "{item:?}");
            assert!(l.contains(l.centre_of(item)));
        }
        assert_eq!(l.hit([2.0, 2.0]), None);
        assert!(!l.contains([2.0, 2.0]));
        assert_eq!(l.rows.len(), ROWS);
        assert!(layout([1100.0, 900.0], 1.0, 99).row_count == ROWS, "no more rows than there is room for");
        assert!(layout([1100.0, 900.0], 1.0, 0).card[3] < l.card[3], "no rows, a shorter card");
    }

    fn view(rows: usize) -> SearchView {
        SearchView {
            query: "hello".into(),
            caret: 5,
            case: false,
            words: true,
            total: 17,
            current: 2,
            read: (3, 10),
            rows: (0..rows).map(|i| SearchRow { page: i as u32, snippet: "say hello to the world".into(), at: 4..9, current: i == 1 }).collect(),
        }
    }

    #[test]
    fn it_says_what_was_typed_how_many_were_found_and_shows_each_row() {
        let l = layout([1100.0, 900.0], 1.0, 3);
        let mut ui = Ui::default();
        l.draw(&view(3), &mut ui);
        let words: Vec<&str> = ui.labels.iter().map(|t| t.text.as_str()).collect();
        assert!(words.contains(&"hello"), "{words:?}");
        // What to match is shown by icons, not words: the options' buttons draw lines, and lit ones differ from dim ones.
        assert!(!words.iter().any(|t| matches!(*t, "Aa" | "W")), "{words:?}");
        let mut lit = Ui::default();
        l.draw(&SearchView { case: true, words: false, ..view(3) }, &mut lit);
        assert_ne!(format!("{:?}", lit.panels), format!("{:?}", ui.panels), "the option that is on is washed");
        assert!(words.iter().any(|t| t.starts_with("3 of 17") && t.contains("reading 3 of 10")), "{words:?}");
        assert_eq!(words.iter().filter(|t| **t == "say hello to the world").count(), 3);
        assert!(ui.labels.iter().all(|t| t.at.iter().all(|v| v.is_finite())));
        assert!(ui.lines.iter().all(|c| c.a.iter().chain(&c.b).all(|v| v.is_finite())));
        // An empty query offers to be typed in; nothing found says so.
        let (mut empty, mut none) = (Ui::default(), Ui::default());
        l.draw(&SearchView { query: String::new(), caret: 0, total: 0, rows: vec![], ..view(0) }, &mut empty);
        l.draw(&SearchView { total: 0, rows: vec![], ..view(0) }, &mut none);
        assert!(empty.labels.iter().any(|t| t.text == "Search"));
        assert!(none.labels.iter().any(|t| t.text.starts_with("No results")));
    }

    #[test]
    fn a_long_query_shows_the_part_with_the_caret_and_a_dark_card_uses_no_light_colour() {
        let l = layout([480.0, 800.0], 1.0, 2);
        let long = "x".repeat(200);
        let mut ui = Ui::default();
        l.draw(&SearchView { query: long.clone(), caret: 200, ..view(2) }, &mut ui);
        let shown = ui.labels.iter().find(|t| t.text.starts_with('x')).expect("the query").text.len();
        assert!(shown > 0 && shown < 200, "{shown} of the characters fit");
        let mut dark = Ui::with_palette(crate::palette::DARK);
        l.draw(&view(2), &mut dark);
        assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel), "no white panel in the dark card");
    }
}
