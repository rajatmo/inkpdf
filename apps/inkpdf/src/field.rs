//! A piece of text being edited, and what the keyboard does to it: typed characters, Backspace and Delete, the caret's arrows, Home and End (with
//! Shift they select), and select all, copy, cut and paste. A text box on the page, the search card's field and a field of a form are all one of
//! these, so they edit alike; each adds what is its own (a box's lines going up and down, a search's places, a form's limit).
//! Everything is counted in characters.

use crate::clipboard::TextClipboard;
use crate::input::{Dir, Key};
use crate::text;

/// What a field takes: how much, and whether a new line is a character of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub max_chars: usize,
    pub multiline: bool,
}

/// What a key did to a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edited {
    /// Nothing: the field has no use for the key.
    Unused,
    /// The caret or the selection moved (or text was copied): the text is as it was.
    Moved,
    /// The text changed.
    Changed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    /// Where the caret is, as the number of characters before it.
    pub caret: usize,
    /// The other end of the selected text, if some is selected: the characters between it and the caret.
    pub anchor: Option<usize>,
}

impl Field {
    /// A field holding `text`, the caret after it.
    pub fn new(text: String) -> Self {
        let caret = text.chars().count();
        Self { text, caret, anchor: None }
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// The selected text, as the first and last character: `None` if nothing is selected.
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.anchor.filter(|&a| a != self.caret).map(|a| (a.min(self.caret), a.max(self.caret)))
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection().map(|(lo, hi)| self.text.chars().skip(lo).take(hi - lo).collect())
    }

    /// The byte of the text that character `index` starts at.
    pub fn byte(&self, index: usize) -> usize {
        self.text.char_indices().nth(index).map_or(self.text.len(), |(b, _)| b)
    }

    /// Put `with` in the place of characters `lo` to `hi`, the caret after it, nothing selected.
    pub fn replace(&mut self, lo: usize, hi: usize, with: &str) {
        let (a, b) = (self.byte(lo), self.byte(hi));
        self.text.replace_range(a..b, with);
        self.caret = lo + with.chars().count();
        self.anchor = None;
    }

    /// The caret goes to character `to`; with `extend` the text between where it was and there is selected.
    pub fn move_to(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = to.min(self.len());
    }

    /// Type `typed` at the caret, in the place of what is selected (see [`text::clean`]; a new line only where the rules allow it, and no more
    /// than they allow). `true` if the text changed.
    pub fn type_text(&mut self, typed: &str, rules: Rules) -> bool {
        let (lo, hi) = self.selection().unwrap_or((self.caret, self.caret));
        let room = rules.max_chars.saturating_sub(self.len() - (hi - lo));
        let cleaned: String = text::clean(typed).chars().filter(|&c| rules.multiline || c != '\n').take(room).collect();
        if cleaned.is_empty() && lo == hi {
            return false;
        }
        self.replace(lo, hi, &cleaned);
        true
    }

    /// Take away what is selected, or else the character before the caret (a line break goes whole).
    pub fn backspace(&mut self) -> bool {
        match self.selection() {
            Some((lo, hi)) => self.replace(lo, hi, ""),
            None if self.caret > 0 => self.replace(self.caret - 1, self.caret, ""),
            None => return false,
        }
        true
    }

    /// Take away what is selected, or else the character after the caret.
    pub fn delete_forward(&mut self) -> bool {
        match self.selection() {
            Some((lo, hi)) => self.replace(lo, hi, ""),
            None if self.caret < self.len() => self.replace(self.caret, self.caret + 1, ""),
            None => return false,
        }
        true
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.len();
    }

    /// Whatever the keyboard does to the field: `key` is a typed character or one of the editing keys (the others are [`Edited::Unused`]: the
    /// owner's, such as Escape, Enter or the arrows that go up and down). In a field of one line, with nothing selected, copy and cut take the
    /// whole text, and paste takes the first line of the clipboard.
    pub fn key(&mut self, key: Key<'_>, clipboard: &mut dyn TextClipboard, rules: Rules) -> Edited {
        let changed = |did: bool| if did { Edited::Changed } else { Edited::Moved };
        match key {
            Key::Text(typed) => changed(self.type_text(typed, rules)),
            Key::Backspace => changed(self.backspace()),
            Key::Delete => changed(self.delete_forward()),
            // Without Shift, an arrow first lets go of the selection at the end it points to.
            Key::Arrow(dir @ (Dir::Left | Dir::Right), extend) => {
                let to = match (self.selection().filter(|_| !extend), dir) {
                    (Some((lo, _)), Dir::Left) => lo,
                    (Some((_, hi)), _) => hi,
                    (None, Dir::Left) => self.caret.saturating_sub(1),
                    (None, _) => self.caret + 1,
                };
                self.move_to(to, extend);
                Edited::Moved
            }
            Key::Home(extend) => {
                self.move_to(text::line_range(&self.text, self.caret).0, extend);
                Edited::Moved
            }
            Key::End(extend) => {
                self.move_to(text::line_range(&self.text, self.caret).1, extend);
                Edited::Moved
            }
            Key::Chord('a') => {
                self.select_all();
                Edited::Moved
            }
            Key::Chord(letter @ ('c' | 'x')) => {
                let words = self.selected_text().or_else(|| (!rules.multiline).then(|| self.text.clone()));
                let Some(words) = words else { return Edited::Unused };
                clipboard.set(&words);
                if letter == 'x' {
                    if self.selection().is_none() {
                        self.select_all();
                    }
                    return changed(self.backspace());
                }
                Edited::Moved
            }
            Key::Chord('v') => match clipboard.get() {
                Some(words) => {
                    let words = if rules.multiline { words } else { words.lines().next().unwrap_or_default().to_string() };
                    changed(self.type_text(&words, rules))
                }
                None => Edited::Unused,
            },
            _ => Edited::Unused,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::MemoryClipboard;

    const ONE: Rules = Rules { max_chars: 8, multiline: false };
    const MANY: Rules = Rules { max_chars: 100, multiline: true };

    fn field(text: &str) -> Field {
        Field::new(text.to_string())
    }

    #[test]
    fn typing_replaces_the_selection_stays_within_the_rules_and_a_new_line_only_goes_where_it_is_allowed() {
        let mut f = field("abc");
        f.move_to(0, true);
        assert!(f.type_text("xy", ONE));
        assert_eq!((f.text.as_str(), f.caret), ("xy", 2), "the selected text was replaced");
        assert!(f.type_text("1\n2\t3456789", ONE));
        assert_eq!(f.text, "xy12 345", "no new line in one line, a tab is a space, and no more than 8 characters");
    }

    #[test]
    fn a_field_of_one_line_takes_no_more_than_it_may_and_a_field_of_many_takes_new_lines() {
        let mut f = field("abcdefgh");
        assert!(!f.type_text("x", ONE), "a full field takes nothing");
        let mut g = field("");
        assert!(g.type_text("one\ntwo", MANY));
        assert_eq!(g.text, "one\ntwo");
        assert!(g.backspace() && g.backspace() && g.backspace() && g.backspace(), "a character at a time, and a new line whole");
        assert_eq!(g.text, "one");
    }

    #[test]
    fn the_caret_moves_by_characters_and_an_arrow_first_lets_go_of_the_selection_at_the_end_it_points_to() {
        let mut clip = MemoryClipboard::default();
        let mut f = field("héllo");
        assert_eq!(f.key(Key::Arrow(Dir::Left, false), &mut clip, ONE), Edited::Moved);
        assert_eq!(f.caret, 4);
        f.key(Key::Home(true), &mut clip, ONE);
        assert_eq!((f.caret, f.selection()), (0, Some((0, 4))), "Shift and Home select to the start");
        f.key(Key::Arrow(Dir::Right, false), &mut clip, ONE);
        assert_eq!((f.caret, f.selection()), (4, None), "an arrow lets go at the end it points to");
        f.key(Key::End(false), &mut clip, ONE);
        f.key(Key::Arrow(Dir::Right, false), &mut clip, ONE);
        assert_eq!(f.caret, 5, "and not past the end");
    }

    #[test]
    fn copy_cut_and_paste_work_on_the_selection_and_in_one_line_on_the_whole_text_and_the_first_line_of_the_clipboard() {
        let mut clip = MemoryClipboard::default();
        let mut f = field("hello world");
        f.move_to(5, false);
        f.move_to(0, true);
        f.key(Key::Chord('c'), &mut clip, MANY);
        assert_eq!(clip.get().as_deref(), Some("hello"));
        let mut one = field("brown");
        assert_eq!(one.key(Key::Chord('x'), &mut clip, ONE), Edited::Changed);
        assert_eq!((one.text.as_str(), clip.get().as_deref()), ("", Some("brown")), "nothing selected: the whole text is cut");
        clip.set("first\nsecond");
        one.key(Key::Chord('v'), &mut clip, ONE);
        assert_eq!(one.text, "first", "the first line of the clipboard");
        let mut many = field("");
        many.key(Key::Chord('v'), &mut clip, MANY);
        assert_eq!(many.text, "first\nsecond");
        assert_eq!(many.key(Key::Chord('c'), &mut clip, MANY), Edited::Unused, "nothing selected in a field of many lines: nothing to copy");
        assert_eq!(many.key(Key::Escape, &mut clip, MANY), Edited::Unused);
    }
}
