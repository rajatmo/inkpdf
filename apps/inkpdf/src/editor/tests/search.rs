//! The search card: opened by a button or Ctrl+F, typed into from the keyboard, what it finds in the pages' text and in typed text boxes, the
//! view going to the place, and every device pressing its buttons.

use std::sync::Arc;

use super::*;
use crate::input::Mode;
use crate::pagetext::FakeText;
use crate::toolbar::SearchItem;
use crate::tools::{Dir, Key};

const PAGE0: [&str; 3] = ["Hello world", "The quick brown fox", "Hello again"];
const PAGE1: [&str; 2] = ["Nothing here", "HELLO there, shello"];

fn reading() -> Editor {
    let mut ed = editor();
    ed.set_text_files(vec![Some(0), Some(1)]);
    ed.add_text(0, Some(Arc::new(FakeText::new(&PAGE0))));
    ed.add_text(1, Some(Arc::new(FakeText::new(&PAGE1))));
    ed
}

fn type_query(ed: &mut Editor, query: &str) {
    ed.open_search();
    ed.press(Key::Text(query));
}

fn hits(ed: &Editor) -> Vec<(u32, String)> {
    ed.search.hits().iter().map(|h| (h.page, h.snippet.clone())).collect()
}

#[test]
fn typing_finds_every_place_in_reading_order_with_its_line_of_text() {
    let mut ed = reading();
    type_query(&mut ed, "hello");
    let found = hits(&ed);
    assert_eq!(found.len(), 4, "{found:?}");
    assert_eq!(found.iter().map(|h| h.0).collect::<Vec<_>>(), [0, 0, 1, 1], "'hello' in 'shello' too, case folded");
    assert!(found[0].1.starts_with("Hello world"));
    let view = ed.search_view().expect("open");
    assert_eq!((view.total, view.current, view.rows.len()), (4, 0, 4));
    assert!(view.rows[0].current && !view.rows[1].current);
}

#[test]
fn the_options_are_the_case_and_whole_words_and_are_asked_to_be_remembered() {
    let mut ed = reading();
    type_query(&mut ed, "hello");
    ed.press_search_item(SearchItem::Case);
    assert_eq!(ed.search.hits().len(), 1, "only the lower case one, inside 'shello'");
    ed.press_search_item(SearchItem::Case);
    ed.press_search_item(SearchItem::Words);
    assert_eq!(ed.search.hits().len(), 3, "not the one inside 'shello'");
    let saved = ed.settings();
    assert!(saved.search_words && !saved.search_case);
    let mut fresh = reading();
    fresh.apply_settings(&saved);
    type_query(&mut fresh, "hello");
    assert_eq!(fresh.search.hits().len(), 3, "the option came back");
}

#[test]
fn enter_and_the_arrows_step_through_the_places_round_and_round_and_the_view_goes_to_each() {
    let mut ed = reading();
    type_query(&mut ed, "hello");
    ed.press(Key::Text("\n"));
    assert_eq!(ed.search.current(), 1);
    ed.press(Key::Arrow(Dir::Down, false));
    ed.press(Key::Arrow(Dir::Down, false));
    assert_eq!(ed.search.current(), 3);
    ed.press(Key::Arrow(Dir::Down, false));
    assert_eq!(ed.search.current(), 0, "round to the first");
    ed.press(Key::Arrow(Dir::Up, false));
    assert_eq!(ed.search.current(), 3, "and back round to the last");
    settle(&mut ed);
    let on = ed.view.doc_to_screen([ed.search.hits()[3].rects[0][0], ed.search.hits()[3].rects[0][1]]);
    assert!(on[1] > 0.0 && on[1] < ed.view.viewport[1], "the place is in view: {on:?}");
}

#[test]
fn the_field_takes_edits_the_caret_and_the_clipboard_and_escape_closes_the_card() {
    let mut ed = reading();
    type_query(&mut ed, "quik");
    assert_eq!(ed.search.hits().len(), 0);
    ed.press(Key::Arrow(Dir::Left, false));
    ed.press(Key::Text("c"));
    assert_eq!((ed.search.query.as_str(), ed.search.hits().len()), ("quick", 1));
    ed.press(Key::Home(false));
    ed.press(Key::Delete);
    assert_eq!(ed.search.query, "uick");
    ed.press(Key::End(false));
    ed.press(Key::Backspace);
    assert_eq!(ed.search.query, "uic");
    ed.press(Key::Chord('c'));
    assert_eq!(ed.text_clipboard.get().as_deref(), Some("uic"));
    ed.press(Key::Chord('x'));
    assert_eq!(ed.search.query, "");
    ed.text_clipboard.set("brown\nsecond line");
    ed.press(Key::Chord('v'));
    assert_eq!(ed.search.query, "brown", "the first line of the clipboard");
    assert!(ed.is_typing(), "the keyboard is the card's");
    assert!(ed.press(Key::Escape));
    assert!(!ed.search_open() && !ed.is_typing());
    assert_eq!(ed.search.hits().len(), 0, "the places are forgotten, the query kept");
    ed.open_search();
    assert_eq!((ed.search.query.as_str(), ed.search.hits().len()), ("brown", 1), "opened again it has what was typed");
}

#[test]
fn a_query_with_nothing_to_find_empty_or_hostile_is_no_places_and_no_trouble() {
    let mut ed = reading();
    type_query(&mut ed, "");
    assert!(ed.search.hits().is_empty());
    for hostile in ["\u{0}", "a\u{301}", "😀", &"x".repeat(5000), "\t\r"] {
        ed.press(Key::Text(hostile));
    }
    assert!(ed.search.query.chars().count() <= 200, "the query is capped");
    ed.press(Key::Backspace);
    ed.press(Key::Text("\u{7f}"));
    assert!(ed.search_view().is_some());
}

#[test]
fn typed_text_boxes_are_searched_too_and_new_text_arriving_adds_places() {
    let mut ed = reading();
    add_text(&mut ed, 100.0, 400.0, "a hello in a box");
    type_query(&mut ed, "hello");
    assert_eq!(ed.search.hits().len(), 5, "the box is one more");
    assert!(ed.search.hits().iter().any(|h| h.snippet.contains("in a box")));
    // A page whose text has not been read has none yet; when it is, its places appear without moving the current one.
    let mut late = editor();
    late.set_text_files(vec![Some(0), Some(1)]);
    late.add_text(1, Some(Arc::new(FakeText::new(&PAGE1))));
    type_query(&mut late, "hello");
    assert_eq!(late.search.hits().len(), 2);
    assert_eq!(late.search_view().expect("open").read, (1, 2), "one of two pages read so far");
    late.add_text(0, Some(Arc::new(FakeText::new(&PAGE0))));
    late.step_view(0.016);
    assert_eq!(late.search.hits().len(), 4);
}

#[test]
fn what_is_found_follows_the_text_boxes_being_changed_and_undone() {
    let mut ed = reading();
    type_query(&mut ed, "zebra");
    assert!(ed.search.hits().is_empty());
    let id = add_text(&mut ed, 100.0, 400.0, "a zebra");
    ed.step_view(0.016);
    assert_eq!(ed.search.hits().len(), 1);
    ed.ink.remove(&[id]);
    ed.step_view(0.016);
    assert!(ed.search.hits().is_empty());
}

#[test]
fn every_part_of_the_card_works_with_the_mouse_the_tip_and_a_finger() {
    let mut ed = reading();
    ed.set_mode(Mode::Desktop);
    // The magnifier on the toolbar opens it, with every device.
    let at = ed.toolbar().centre_of(Item::Search);
    mouse_click_at(&mut ed, at);
    assert!(ed.search_open());
    pen_stroke(&mut ed, TIP, &[at, at]);
    assert!(!ed.search_open(), "the same button puts it away");
    // (A finger, in an editor the pen has not been near.)
    let mut finger = reading();
    let at = finger.toolbar().centre_of(Item::Search);
    finger.pointer_event(touch(3, Phase::Down, at, 8.0));
    finger.pointer_event(touch(3, Phase::Up, at, 8.0));
    assert!(finger.search_open(), "a finger opens it");
    let close = finger.search_layout().expect("card").centre_of(SearchItem::Close);
    finger.pointer_event(touch(4, Phase::Down, close, 8.0));
    finger.pointer_event(touch(4, Phase::Up, close, 8.0));
    assert!(!finger.search_open(), "and closes it");
    ed.open_search();
    ed.press(Key::Text("hello"));
    let card = ed.search_layout().expect("card");
    // Next, previous and a row, with the tip.
    let next = card.centre_of(SearchItem::Next);
    pen_stroke(&mut ed, TIP, &[next, next]);
    assert_eq!(ed.search.current(), 1);
    let previous = card.centre_of(SearchItem::Previous);
    pen_stroke(&mut ed, TIP, &[previous, previous]);
    assert_eq!(ed.search.current(), 0);
    let row = card.centre_of(SearchItem::Row(2));
    pen_stroke(&mut ed, TIP, &[row, row]);
    assert_eq!(ed.search.current(), 2, "a row goes to its place");
    // The options with the mouse, the close button with a finger.
    let case = card.centre_of(SearchItem::Case);
    mouse_click_at(&mut ed, case);
    assert!(ed.search.case);
    let close = card.centre_of(SearchItem::Close);
    pen_stroke(&mut ed, TIP, &[close, close]);
    assert!(!ed.search_open());
    assert_eq!(ed.ink.len(), 0, "none of the presses drew on the page");
}

#[test]
fn ctrl_f_opens_it_in_the_desktop_mode_only_and_it_does_not_let_a_press_on_it_reach_the_page() {
    let mut ed = reading();
    ed.set_mode(Mode::Tablet);
    assert!(!ed.shortcut("f", false), "a tablet has no keyboard to rely on: the button is how");
    ed.set_mode(Mode::Desktop);
    assert!(ed.shortcut("f", false) && ed.search_open());
    let inside = ed.search_layout().expect("card").centre_of(SearchItem::Field);
    mouse_click_at(&mut ed, inside);
    assert_eq!(ed.ink.len(), 0);
    assert!(ed.shortcut("f", false) && !ed.search_open(), "Ctrl+F again puts it away");
}

#[test]
fn a_new_document_puts_the_card_away_and_the_dark_look_draws_it_in_the_dark_palette() {
    let mut ed = reading();
    type_query(&mut ed, "hello");
    let mut light = crate::toolbar::Ui::default();
    ed.draw_interface(&mut light);
    assert!(light.labels.iter().any(|l| l.text == "hello"), "the query is drawn as text");
    ed.set_appearance(crate::settings::Appearance::Dark);
    ed.step_view(0.016);
    let mut dark = crate::toolbar::Ui::with_palette(ed.palette());
    ed.draw_interface(&mut dark);
    assert!(dark.panels.iter().all(|p| p.color != crate::palette::LIGHT.panel), "no white panel");
    ed.load(crate::view::Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.search_open());
}
