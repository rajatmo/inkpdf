//! Editing the text of a box: the caret and where it goes (the arrows, Home and End, a press in the box), selecting
//! (with Shift, with a drag, with Ctrl+A), and the system clipboard.

use crate::tools::{Dir, Key};

use super::*;

fn with_text() -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Text);
    ed.chrome.panel = None;
    ed
}

fn drag(ed: &mut Editor, from: [f32; 2], to: [f32; 2]) {
    mouse_drag(ed, &[from, [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0], to]);
}

fn key(ed: &mut Editor, key: Key<'_>) {
    assert!(ed.press(key), "{key:?} was heard");
}

fn left(ed: &mut Editor, n: usize) {
    (0..n).for_each(|_| key(ed, Key::Arrow(Dir::Left, false)));
}

/// The text being edited, and where the caret is in it.
fn edited(ed: &Editor) -> (String, usize) {
    ed.text_edit().map(|e| (e.text.clone(), e.caret)).expect("a box is being edited")
}

fn selected(ed: &Editor) -> Option<(usize, usize)> {
    ed.text_edit().and_then(|e| e.selection())
}

/// A new box begun at (100, 200) and typed into.
fn typing(words: &str) -> Editor {
    let mut ed = with_text();
    click_on_page(&mut ed, 100.0, 200.0);
    ed.type_text(words);
    ed
}

#[test]
fn the_arrows_home_and_end_move_the_caret_and_typing_goes_in_at_the_caret() {
    let mut ed = typing("Hello world");
    assert_eq!(edited(&ed), ("Hello world".to_string(), 11), "the caret starts at the end");
    left(&mut ed, 5);
    assert_eq!(edited(&ed).1, 6);
    ed.type_text("big ");
    assert_eq!(edited(&ed), ("Hello big world".to_string(), 10));
    key(&mut ed, Key::Home(false));
    assert_eq!(edited(&ed).1, 0);
    ed.type_text(">");
    assert_eq!(edited(&ed), (">Hello big world".to_string(), 1));
    key(&mut ed, Key::End(false));
    assert_eq!(edited(&ed).1, 16);
    // Right stops at the end, Left at the start.
    key(&mut ed, Key::Arrow(Dir::Right, false));
    assert_eq!(edited(&ed).1, 16);
    left(&mut ed, 40);
    assert_eq!(edited(&ed).1, 0);
    // Backspace takes the character before the caret, Delete the one after; at the ends they have nothing to take.
    key(&mut ed, Key::Backspace);
    assert_eq!(edited(&ed), (">Hello big world".to_string(), 0));
    key(&mut ed, Key::Delete);
    assert_eq!(edited(&ed), ("Hello big world".to_string(), 0));
    key(&mut ed, Key::End(false));
    key(&mut ed, Key::Delete);
    key(&mut ed, Key::Backspace);
    assert_eq!(edited(&ed), ("Hello big worl".to_string(), 14));
    // A character that is not one byte long is still one step for the caret, and one for Backspace.
    ed.type_text("é");
    left(&mut ed, 1);
    ed.type_text("x");
    assert_eq!(edited(&ed), ("Hello big worlxé".to_string(), 15));
    key(&mut ed, Key::Arrow(Dir::Right, false));
    key(&mut ed, Key::Backspace);
    assert_eq!(edited(&ed), ("Hello big worlx".to_string(), 15));
    // It all ends as one box, and undo takes the whole edit back.
    ed.finish_text();
    assert_eq!(ed.ink.len(), 1);
}

#[test]
fn up_and_down_keep_to_the_column_through_a_short_line() {
    let mut ed = typing("abcdef\nab\nabcdef");
    // From the end of the last line, up to the short line's end, up again to the same column as the start.
    left(&mut ed, 1);
    assert_eq!(edited(&ed).1, 15);
    key(&mut ed, Key::Arrow(Dir::Up, false));
    assert_eq!(edited(&ed).1, 9, "the short line ends sooner");
    key(&mut ed, Key::Arrow(Dir::Up, false));
    assert_eq!(edited(&ed).1, 5, "and the column is remembered");
    key(&mut ed, Key::Arrow(Dir::Up, false));
    assert_eq!(edited(&ed).1, 0, "the first line up is the start");
    key(&mut ed, Key::Arrow(Dir::Down, false));
    key(&mut ed, Key::Arrow(Dir::Down, false));
    key(&mut ed, Key::Arrow(Dir::Down, false));
    assert_eq!(edited(&ed).1, 16, "the last line down is the end");
}

#[test]
fn shift_selects_and_what_is_typed_takes_the_place_of_the_selection() {
    let mut ed = typing("abc def ghi");
    key(&mut ed, Key::Home(false));
    for _ in 0..3 {
        key(&mut ed, Key::Arrow(Dir::Right, true));
    }
    assert_eq!(selected(&ed), Some((0, 3)));
    // An arrow without Shift lets go of the selection at the end it points to.
    key(&mut ed, Key::Arrow(Dir::Left, false));
    assert_eq!((selected(&ed), edited(&ed).1), (None, 0));
    key(&mut ed, Key::End(true));
    assert_eq!(selected(&ed), Some((0, 11)));
    key(&mut ed, Key::Arrow(Dir::Right, false));
    assert_eq!((selected(&ed), edited(&ed).1), (None, 11));
    // Select a word back from the end and type over it.
    for _ in 0..3 {
        key(&mut ed, Key::Arrow(Dir::Left, true));
    }
    ed.type_text("XYZ!");
    assert_eq!(edited(&ed), ("abc def XYZ!".to_string(), 12));
    // Backspace and Delete take the selection, not a character.
    key(&mut ed, Key::Home(false));
    key(&mut ed, Key::End(true));
    key(&mut ed, Key::Backspace);
    assert_eq!(edited(&ed), (String::new(), 0));
}

#[test]
fn ctrl_a_c_x_and_v_work_on_the_text_with_the_system_clipboard() {
    let mut ed = typing("one two");
    assert!(ed.shortcut("a", false));
    assert_eq!(selected(&ed), Some((0, 7)));
    assert!(ed.shortcut("c", false));
    assert_eq!(ed.clip.text.get().as_deref(), Some("one two"));
    // Paste over the selection, then again at the caret.
    ed.type_text("Z");
    assert!(ed.shortcut("v", false));
    assert_eq!(edited(&ed), ("Zone two".to_string(), 8));
    // Cut: what is selected goes to the clipboard and leaves the text.
    key(&mut ed, Key::Arrow(Dir::Left, true));
    key(&mut ed, Key::Arrow(Dir::Left, true));
    key(&mut ed, Key::Arrow(Dir::Left, true));
    assert!(ed.shortcut("x", false));
    assert_eq!((edited(&ed), ed.clip.text.get().as_deref()), (("Zone ".to_string(), 5), Some("two")));
    // Nothing selected: copying and cutting change nothing.
    assert!(ed.shortcut("c", false) && ed.shortcut("x", false));
    assert_eq!((edited(&ed), ed.clip.text.get().as_deref()), (("Zone ".to_string(), 5), Some("two")));
    // Pasted text is cleaned like typed text: a tab is a space, other control characters go.
    ed.clip.text.set("a\tb\u{7}c\nd");
    ed.shortcut("v", false);
    assert_eq!(edited(&ed).0, "Zone a bc\nd");
}

#[test]
fn a_press_in_the_box_being_edited_puts_the_caret_there_and_a_drag_selects_and_a_press_outside_ends_it() {
    let mut ed = with_text();
    click_on_page(&mut ed, 100.0, 200.0);
    ed.type_text("Hello world");
    let size = ed.text_edit().map(|e| e.size).expect("size");
    // Press just after the first letter: the caret goes between the first and second.
    let first = crate::text::caret_at("Hello world", size, 1)[0];
    let p = on_page(&ed, 100.0 + first, 205.0);
    mouse_click_at(&mut ed, p);
    assert_eq!(edited(&ed).1, 1);
    assert_eq!(ed.ink.len(), 0, "still the same edit, nothing stored");
    assert!(ed.text_edit().is_some_and(|e| e.target.is_none()));
    // A drag from one place to another selects what is between.
    let (a, b) = (crate::text::caret_at("Hello world", size, 2)[0], crate::text::caret_at("Hello world", size, 8)[0]);
    let (pa, pb) = (on_page(&ed, 100.0 + a, 205.0), on_page(&ed, 100.0 + b, 205.0));
    drag(&mut ed, pa, pb);
    assert_eq!((selected(&ed), edited(&ed).1), (Some((2, 8)), 8));
    // And backwards.
    drag(&mut ed, pb, pa);
    assert_eq!((selected(&ed), edited(&ed).1), (Some((2, 8)), 2));
    // A press elsewhere ends the edit and stores the box (and starts the next).
    click_on_page(&mut ed, 100.0, 500.0);
    assert_eq!(ed.ink.len(), 1);
    assert!(ed.text_edit().is_some_and(|e| e.target.is_none() && e.text.is_empty()));
}

#[test]
fn as_a_tablet_the_stylus_puts_the_caret_and_the_touch_keyboard_types() {
    let mut ed = with_text();
    ed.set_mode(Mode::Tablet);
    let corner = on_page(&ed, 100.0, 200.0);
    pen_stroke(&mut ed, TIP, &[corner, corner]);
    ed.type_text("Hello world");
    let size = ed.text_edit().map(|e| e.size).expect("size");
    let x = crate::text::caret_at("Hello world", size, 5)[0];
    let p = on_page(&ed, 100.0 + x, 205.0);
    pen_stroke(&mut ed, TIP, &[p, p]);
    assert_eq!(edited(&ed).1, 5);
    ed.type_text(",");
    assert_eq!(edited(&ed).0, "Hello, world");
    // A keyboard on the tablet types, so Ctrl+A and Ctrl+C work on the text; with no box being edited they do nothing.
    assert!(ed.shortcut("a", false));
    assert_eq!(selected(&ed), Some((0, 12)));
    assert!(ed.shortcut("c", false));
    assert_eq!(ed.clip.text.get().as_deref(), Some("Hello, world"));
    // The caret is part of the box: the arrows are the keyboard's, and the stylus's drag selects.
    let (a, b) = (
        on_page(&ed, 100.0 + crate::text::caret_at("Hello, world", size, 0)[0], 205.0),
        on_page(&ed, 100.0 + crate::text::caret_at("Hello, world", size, 5)[0], 205.0),
    );
    pen_stroke(&mut ed, TIP, &[a, [(a[0] + b[0]) / 2.0, a[1]], b]);
    assert_eq!(selected(&ed), Some((0, 5)));
}

#[test]
fn breaking_lines_while_typing_in_the_middle_keeps_the_caret_among_the_same_characters() {
    let mut ed = with_text();
    ed.styles.text_style.width = 60.0;
    ed.restyle_tool();
    click_on_page(&mut ed, 100.0, 160.0);
    let words = "word ".repeat(30);
    ed.type_text(&words);
    let (text, _) = edited(&ed);
    assert!(text.contains('\n'), "long enough to be broken");
    assert_eq!(text.replace('\n', ""), words);
    // Ten characters back, type: the characters before the caret are the first ones and what was typed.
    left(&mut ed, 10);
    ed.type_text("XY");
    let (text, caret) = edited(&ed);
    let plain: String = text.replace('\n', "");
    let before: String = text.chars().take(caret).filter(|&c| c != '\n').collect();
    assert_eq!(plain.len(), words.len() + 2);
    assert!(before.ends_with("XY") && before.len() == words.len() - 10 + 2, "the caret is just after what was typed: {before:?}");
}

#[test]
fn the_words_of_a_copied_box_go_on_the_system_clipboard_and_words_from_elsewhere_make_a_box() {
    let mut ed = with_text();
    let id = add_text(&mut ed, 100.0, 200.0, "Hello");
    ed.selection = vec![id];
    assert!(ed.copy_selection());
    assert_eq!(ed.clip.text.get().as_deref(), Some("Hello"));
    // Pasting the box we copied pastes the box, not the words as well.
    assert!(ed.paste());
    assert_eq!(ed.ink.len(), 2);
    // Words that came from some other program become a box of their own, in the text tool's size.
    ed.clip.text.set("from elsewhere");
    ed.selection.clear();
    assert!(ed.paste());
    assert_eq!(ed.ink.len(), 3);
    let made = ed.ink.strokes().last().expect("the new box");
    assert_eq!(made.text.as_ref().map(|t| t.text.replace('\n', " ")), Some("from elsewhere".to_string()));
    assert_eq!(ed.selection, [made.id]);
    // Nothing to paste (blank words) makes nothing; another tool has no use for words.
    ed.clip.text.set("   ");
    ed.selection.clear();
    let before = ed.ink.len();
    ed.clip.items.clear();
    assert!(!ed.paste());
    ed.clip.text.set("words");
    ed.set_tool(Tool::Pen);
    assert!(!ed.paste());
    assert_eq!(ed.ink.len(), before);
}

#[test]
fn a_new_size_or_colour_chosen_while_editing_keeps_the_caret_where_it_was() {
    let mut ed = typing("one two three");
    left(&mut ed, 6);
    ed.styles.text_style.width = 40.0;
    ed.restyle_tool();
    assert_eq!(edited(&ed), ("one two three".to_string(), 7));
    ed.type_text("-");
    assert_eq!(edited(&ed).0, "one two- three");
}
