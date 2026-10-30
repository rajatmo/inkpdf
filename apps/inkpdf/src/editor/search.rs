//! Search: the card opened by the magnifier (or Ctrl+F), the query typed in it, the places it finds in the text of the pages and in the
//! text boxes typed on them, and the view going to the one that is current. It is not modal: the page stays in view and the pen still
//! works on it; only the keyboard is the card's while it is open. What is found is worked out again whenever the query, the options, the text
//! read so far or the ink changes.

use std::ops::Range;

use crate::pagetext::snippet;
use crate::toolbar::{SearchItem, SearchLayout, SearchRow, SearchView};
use crate::tools::{Dir, Key};

use super::{Editor, Request};

/// Places kept at most, and found on one page at most.
pub const HIT_CAP: usize = 5000;
const PER_PAGE: usize = 500;
/// Characters of the text kept on each side of a place, for its row.
const CONTEXT: usize = 28;
/// The longest query.
const MAX_QUERY: usize = 200;
/// Pixels from the top of the window that the toolbars cover, which a place is brought below.
const BARS_PX: f32 = 140.0;

/// A place the query was found.
pub struct Hit {
    pub page: u32,
    /// The boxes to light, in document space (a place over a line break is two).
    pub rects: Vec<[f32; 4]>,
    pub snippet: String,
    pub at: Range<usize>,
}

/// What a result was worked out from: when it changes, the result is old.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    text: u64,
    ink: u64,
    query: String,
    case: bool,
    words: bool,
}

#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub query: String,
    /// The caret, as a byte index on a character of the query.
    pub caret: usize,
    pub case: bool,
    pub words: bool,
    hits: Vec<Hit>,
    current: usize,
    /// The first row of the list that is shown.
    first_row: usize,
    stamp: Option<Stamp>,
}

impl Search {
    /// Put the card away and forget what it found (the query is kept, for the next time it is opened).
    pub fn close(&mut self) {
        self.open = false;
        self.hits.clear();
        self.stamp = None;
        self.current = 0;
        self.first_row = 0;
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    pub fn current(&self) -> usize {
        self.current
    }
}

/// Where `needle` is in `hay` (characters), as ranges of characters: the same rules as the page's own search.
fn find_in(hay: &str, needle: &str, case: bool, words: bool, limit: usize) -> Vec<Range<usize>> {
    let fold = |c: char| if case { c } else { c.to_lowercase().next().unwrap_or(c) };
    let (hay, needle): (Vec<char>, Vec<char>) = (hay.chars().collect(), needle.chars().collect());
    let (folded, needle): (Vec<char>, Vec<char>) = (hay.iter().copied().map(fold).collect(), needle.into_iter().map(fold).collect());
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= folded.len() && out.len() < limit {
        if folded[i..i + needle.len()] == needle[..] {
            let edge = |j: Option<usize>| j.is_none_or(|j| !hay[j].is_alphanumeric());
            let whole = edge(i.checked_sub(1)) && edge((i + needle.len() < hay.len()).then_some(i + needle.len()));
            if !words || whole {
                out.push(i..i + needle.len());
                i += needle.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

impl Editor {
    pub fn search_open(&self) -> bool {
        self.search.open
    }

    /// Open the card (with the caret in the field) or put it away.
    pub fn toggle_search(&mut self) {
        if self.search.open {
            self.close_search();
        } else {
            self.open_search();
        }
    }

    pub fn open_search(&mut self) {
        if self.search.open {
            return;
        }
        // (What was being typed is made part of the document first: the keyboard is the card's from now on.)
        self.settle();
        self.chrome.panel = None;
        self.search.open = true;
        self.search.caret = self.search.query.len();
        self.refresh_search();
        self.ws.redraw = true;
    }

    pub fn close_search(&mut self) {
        if self.search.open {
            self.search.close();
            self.ws.redraw = true;
        }
    }

    /// The card, where it is (under the bars, at the right), if it is open.
    pub fn search_layout(&self) -> Option<SearchLayout> {
        if !self.search.open || self.chrome.prompt.is_some() {
            return None;
        }
        let bar = self.toolbar();
        let mut top = self.context_bar().map_or(bar.bottom(), |c| c.rect[3]) + 8.0 * self.ws.dpr;
        // (Under the theme card, which is at the same edge, while that is open.)
        if let Some(theme) = self.theme_layout() {
            top = top.max(theme.card[3] + 8.0 * self.ws.dpr);
        }
        Some(SearchLayout::new(&bar, self.ws.view.viewport, top, self.search.hits.len().min(crate::toolbar::SEARCH_ROWS)))
    }

    /// What the card is drawn from.
    pub fn search_view(&self) -> Option<SearchView> {
        if !self.search.open {
            return None;
        }
        let s = &self.search;
        let rows = s
            .hits
            .iter()
            .enumerate()
            .skip(s.first_row)
            .take(crate::toolbar::SEARCH_ROWS)
            .map(|(i, h)| SearchRow { page: h.page, snippet: h.snippet.clone(), at: h.at.clone(), current: i == s.current })
            .collect();
        Some(SearchView {
            query: s.query.clone(),
            caret: s.caret,
            case: s.case,
            words: s.words,
            total: s.hits.len(),
            current: s.current,
            read: self.ws.text.progress(),
            rows,
        })
    }

    /// Work the places out again if what they depend on has changed (the text read so far arrives while the card is open).
    pub(super) fn refresh_search(&mut self) {
        if !self.search.open {
            return;
        }
        let now = Stamp {
            text: self.ws.text.revision(),
            ink: self.ws.ink.revision(),
            query: self.search.query.clone(),
            case: self.search.case,
            words: self.search.words,
        };
        if self.search.stamp.as_ref() == Some(&now) {
            return;
        }
        let first_time = self.search.stamp.as_ref().is_none_or(|s| s.query != now.query || s.case != now.case || s.words != now.words);
        // Keep the place that is current if it is still found (what is read later adds places before and after it).
        let keep = self.search.hits.get(self.search.current).map(|h| (h.page, h.rects.first().map(|r| (r[0], r[1]))));
        self.search.hits = self.find_all(&now.query);
        self.search.stamp = Some(now);
        let n = self.search.hits.len();
        self.search.current = if first_time {
            // The first place at or after the page in view.
            let page = self.ws.view.current_page(&self.ws.layout) as u32;
            self.search.hits.iter().position(|h| h.page >= page).unwrap_or(0)
        } else {
            keep.and_then(|(page, at)| self.search.hits.iter().position(|h| h.page == page && h.rects.first().map(|r| (r[0], r[1])) == at))
                .unwrap_or(self.search.current.min(n.saturating_sub(1)))
        };
        self.keep_current_listed();
        if first_time {
            self.show_current();
        }
        self.ws.redraw = true;
    }

    /// Every place the query is found, in reading order: in the text of the pages read so far, and in the text boxes typed on them.
    fn find_all(&self, query: &str) -> Vec<Hit> {
        let mut hits = Vec::new();
        if query.is_empty() {
            return hits;
        }
        let (case, words) = (self.search.case, self.search.words);
        for page in 0..self.ws.layout.len() {
            let (Some(text), Some(r)) = (self.ws.text.page(page), self.ws.layout.page(page)) else { continue };
            for range in text.find(query, case, words, PER_PAGE) {
                let rects = text.line_rects(range.clone()).into_iter().map(|b| [b[0] + r.x, b[1] + r.y, b[2] + r.x, b[3] + r.y]).collect();
                let (snippet, at) = snippet(&**text, range, CONTEXT);
                hits.push(Hit { page: page as u32, rects, snippet, at });
            }
            if hits.len() >= HIT_CAP {
                break;
            }
        }
        for s in self.ws.ink.strokes() {
            let Some(block) = &s.text else { continue };
            for range in find_in(&block.text, query, case, words, PER_PAGE) {
                let chars: Vec<char> = block.text.chars().collect();
                let from = range.start.saturating_sub(CONTEXT);
                let to = (range.end + CONTEXT).min(chars.len());
                let snippet: String = chars[from..to].iter().map(|c| if c.is_control() { ' ' } else { *c }).collect();
                hits.push(Hit { page: s.page, rects: vec![s.bbox()], snippet, at: range.start - from..range.end - from });
            }
        }
        // Reading order: by page, then down the page, then across.
        hits.sort_by(|a, b| {
            let key = |h: &Hit| h.rects.first().map_or((0.0, 0.0), |r| (r[1], r[0]));
            (a.page, key(a).0.round() as i64, key(a).1.round() as i64).cmp(&(b.page, key(b).0.round() as i64, key(b).1.round() as i64))
        });
        hits.truncate(HIT_CAP);
        hits
    }

    /// Scroll the list of rows so that the current place is in it.
    fn keep_current_listed(&mut self) {
        let rows = crate::toolbar::SEARCH_ROWS;
        let s = &mut self.search;
        if s.current < s.first_row {
            s.first_row = s.current;
        } else if s.current >= s.first_row + rows {
            s.first_row = s.current + 1 - rows;
        }
        s.first_row = s.first_row.min(s.hits.len().saturating_sub(rows));
    }

    /// Take the view to the current place, if it is not in view.
    fn show_current(&mut self) {
        let Some(hit) = self.search.hits.get(self.search.current) else { return };
        let Some(first) = hit.rects.first().copied() else { return };
        let top = BARS_PX * self.ws.dpr;
        self.ws.view.scroll_to_show(&self.ws.layout, first, top);
        self.ws.redraw = true;
    }

    /// The place after the current one (round to the first), and the one before.
    pub fn search_next(&mut self) {
        self.step_search(1);
    }

    pub fn search_previous(&mut self) {
        self.step_search(-1);
    }

    fn step_search(&mut self, by: isize) {
        let n = self.search.hits.len();
        if n == 0 {
            return;
        }
        self.search.current = (self.search.current as isize + by).rem_euclid(n as isize) as usize;
        self.keep_current_listed();
        self.show_current();
    }

    fn go_to_search_row(&mut self, row: usize) {
        let at = self.search.first_row + row;
        if at < self.search.hits.len() {
            self.search.current = at;
            self.show_current();
            self.ws.redraw = true;
        }
    }

    /// A part of the card pressed (and let go on): what it does.
    pub(super) fn press_search_item(&mut self, item: SearchItem) {
        match item {
            // (The caret goes to the end: the field is one line and short.)
            SearchItem::Field => self.search.caret = self.search.query.len(),
            SearchItem::Case => {
                self.search.case = !self.search.case;
                self.ws.requests.push(Request::SettingsChanged);
            }
            SearchItem::Words => {
                self.search.words = !self.search.words;
                self.ws.requests.push(Request::SettingsChanged);
            }
            SearchItem::Previous => self.search_previous(),
            SearchItem::Next => self.search_next(),
            SearchItem::Close => self.close_search(),
            SearchItem::Row(row) => self.go_to_search_row(row),
        }
        self.refresh_search();
        self.ws.redraw = true;
    }

    /// The keyboard while the card is open: typing in the field, the caret, Enter and the arrows to move between places, Esc to close.
    /// `true` if the key was used.
    pub(super) fn search_key(&mut self, key: Key<'_>) -> bool {
        let used = match key {
            Key::Text(typed) => {
                for c in typed.chars() {
                    match c {
                        '\n' | '\r' => self.search_next(),
                        c if !c.is_control() => self.type_in_search(c),
                        _ => {}
                    }
                }
                true
            }
            Key::Backspace => {
                if let Some((at, _)) = self.search.query[..self.search.caret].char_indices().next_back() {
                    self.search.query.remove(at);
                    self.search.caret = at;
                }
                true
            }
            Key::Delete => {
                if self.search.caret < self.search.query.len() {
                    self.search.query.remove(self.search.caret);
                }
                true
            }
            Key::Arrow(Dir::Left, _) => {
                self.search.caret = self.search.query[..self.search.caret].char_indices().next_back().map_or(0, |(at, _)| at);
                true
            }
            Key::Arrow(Dir::Right, _) => {
                let rest = &self.search.query[self.search.caret..];
                self.search.caret += rest.chars().next().map_or(0, char::len_utf8);
                true
            }
            Key::Arrow(Dir::Up, _) => {
                self.search_previous();
                true
            }
            Key::Arrow(Dir::Down, _) => {
                self.search_next();
                true
            }
            Key::Home(_) => {
                self.search.caret = 0;
                true
            }
            Key::End(_) => {
                self.search.caret = self.search.query.len();
                true
            }
            Key::Escape => {
                self.close_search();
                true
            }
            // Select all of the field is its whole text to copy or to take away; paste puts the clipboard's first line in.
            Key::Chord('c') => {
                self.ws.text_clipboard.set(&self.search.query);
                true
            }
            Key::Chord('x') => {
                self.ws.text_clipboard.set(&self.search.query);
                self.search.query.clear();
                self.search.caret = 0;
                true
            }
            Key::Chord('v') => {
                if let Some(text) = self.ws.text_clipboard.get() {
                    for c in text.lines().next().unwrap_or_default().chars().filter(|c| !c.is_control()) {
                        self.type_in_search(c);
                    }
                }
                true
            }
            Key::Chord(_) => false,
        };
        self.refresh_search();
        self.ws.redraw = true;
        used
    }

    fn type_in_search(&mut self, c: char) {
        if self.search.query.chars().count() < MAX_QUERY {
            self.search.query.insert(self.search.caret, c);
            self.search.caret += c.len_utf8();
        }
    }
}
