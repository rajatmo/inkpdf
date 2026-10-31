//! The card of an item an extension made (the program's own test kind stands in for one): what it shows, what is written in it and sent, the name
//! written on it, its buttons and the delete button, with every device, and what each of them makes of the undo history.

use super::*;
use crate::toolbar::CardItem;
use crate::tools::Key;
use crate::tools::items::TestBox;

/// A box holding `data` (the first line is the note, the rest its replies), its top-left corner at (`x`, `y`) on page 0.
fn add_box(ed: &mut Editor, x: f32, y: f32, data: &str) -> u64 {
    let bounds = [12.0 + x, 12.0 + y, 32.0 + x, 32.0 + y];
    ed.ink.add_item(0, Style { rgba: [255, 204, 0, 255], width: 1.0 }, TestBox::ID, data.to_string(), bounds).expect("an item")
}

fn held(ed: &Editor, id: u64) -> String {
    ed.ink.get(id).and_then(|s| s.item.as_ref()).map(|i| i.data.clone()).expect("the item")
}

fn open(ed: &mut Editor, id: u64) {
    ed.card.open = Some(id);
    ed.card.field = crate::field::Field::default();
    ed.redraw = true;
}

#[test]
fn the_card_shows_what_the_kind_says_and_is_there_only_while_its_item_is() {
    let mut ed = editor();
    assert!(ed.item_card_layout().is_none() && ed.item_card_view().is_none() && !ed.is_typing());
    let id = add_box(&mut ed, 100.0, 100.0, "first\nsecond");
    open(&mut ed, id);
    let view = ed.item_card_view().expect("open");
    assert_eq!(
        (view.title.as_str(), view.rows.len(), view.hint.as_deref(), view.actions.as_slice()),
        ("A test box", 2, Some("say"), &["Clear".to_string()][..])
    );
    assert_eq!(view.rows[1].1, "second");
    assert!(ed.is_typing(), "the keyboard is the card's while it is open");
    // Taken away, the item takes its card with it, and brought back by an undo it has it again (the card was never put away).
    ed.ink.remove(&[id]);
    assert!(ed.item_card_view().is_none() && !ed.is_typing(), "the item is gone: so is the card");
    ed.ink.undo();
    assert!(ed.item_card_view().is_some(), "the item is back and its card, which was never put away, with it");
}

#[test]
fn what_is_written_is_sent_by_enter_or_the_send_button_as_one_undo_step_each_and_nothing_empty_is_sent() {
    let mut ed = editor();
    let id = add_box(&mut ed, 100.0, 100.0, "first");
    open(&mut ed, id);
    ed.press(Key::Text("a reply"));
    assert_eq!(ed.item_card_view().expect("open").text, "a reply");
    ed.press(Key::Text("\n"));
    assert_eq!(held(&ed, id), "first\na reply");
    assert!(ed.item_card_view().expect("open").text.is_empty(), "the field is clear for the next");
    let steps = ed.ink.can_undo();
    ed.press(Key::Text("   "));
    ed.press(Key::Text("\n"));
    assert_eq!(held(&ed, id), "first\na reply", "spaces are not a reply");
    // The send button, with the tip.
    ed.press(Key::Text("and another"));
    let send = ed.item_card_layout().expect("card").centre_of(CardItem::Send);
    pen_tap(&mut ed, send);
    assert_eq!(held(&ed, id), "first\na reply\nand another");
    assert!(steps);
    // Each is one undo step.
    assert!(ed.ink.undo());
    assert_eq!(held(&ed, id), "first\na reply");
    assert!(ed.ink.undo());
    assert_eq!(held(&ed, id), "first");
    assert_eq!(ed.ink.len(), 1, "nothing was drawn on the page");
}

#[test]
fn every_part_of_the_card_works_with_the_mouse_the_tip_and_a_finger_and_a_press_on_it_never_reaches_the_page() {
    let mut ed = editor();
    ed.set_mode(crate::input::Mode::Desktop);
    let id = add_box(&mut ed, 100.0, 100.0, "first\nsecond");
    open(&mut ed, id);
    let card = ed.item_card_layout().expect("card");
    // The kind's button, with the mouse: it clears what the box holds.
    mouse_click_at(&mut ed, card.centre_of(CardItem::Action(0)));
    assert_eq!(held(&ed, id), "");
    assert!(ed.ink.undo());
    assert_eq!(held(&ed, id), "first\nsecond");
    // The name is pressed (it has the keyboard), typed, and asked to be remembered; Enter gives the keyboard back to the field.
    pen_tap(&mut ed, card.centre_of(CardItem::Name));
    assert!(ed.item_card_view().expect("open").typing_name);
    ed.press(Key::Text("Ada Lovelace"));
    assert_eq!(ed.settings().author, "Ada Lovelace");
    assert!(ed.requests.has(&Request::SettingsChanged));
    ed.press(Key::Text("\n"));
    assert!(!ed.item_card_view().expect("open").typing_name);
    let mut fresh = editor();
    fresh.apply_settings(&ed.settings());
    assert_eq!(fresh.card.name.text, "Ada Lovelace", "the name comes back");
    // The field with the mouse, and the page untouched by any of it.
    mouse_click_at(&mut ed, card.centre_of(CardItem::Field));
    ed.press(Key::Text("hi"));
    assert_eq!(ed.item_card_view().expect("open").text, "hi");
    assert_eq!(ed.ink.len(), 1, "none of the presses drew on the page");
    // A finger (in an editor the pen has not been near) puts it away.
    let mut touched = editor();
    let other = add_box(&mut touched, 100.0, 100.0, "first");
    open(&mut touched, other);
    let close = touched.item_card_layout().expect("card").centre_of(CardItem::Close);
    finger_tap(&mut touched, close);
    assert!(!touched.item_card_open(), "a finger puts it away");
    // The delete button, with the tip, takes the item away as one step.
    open(&mut ed, id);
    pen_tap(&mut ed, card.centre_of(CardItem::Delete));
    assert!(ed.ink.get(id).is_none() && !ed.item_card_open());
    assert!(ed.ink.undo() && ed.ink.get(id).is_some(), "and the undo brings it back");
}

#[test]
fn the_keyboard_can_do_all_of_it_escape_puts_it_away_and_delete_in_an_empty_field_takes_the_item_away() {
    let mut ed = editor();
    ed.set_mode(crate::input::Mode::Desktop);
    let id = add_box(&mut ed, 100.0, 100.0, "first");
    open(&mut ed, id);
    ed.press(Key::Text("x"));
    ed.press(Key::Delete);
    assert!(ed.ink.get(id).is_some(), "Delete in a field with something in it is a Delete of the field");
    ed.press(Key::Backspace);
    ed.press(Key::Delete);
    assert!(ed.ink.get(id).is_none() && !ed.item_card_open(), "an empty field: Delete is for the item");
    assert!(ed.ink.undo());
    open(&mut ed, id);
    ed.press(Key::Escape);
    assert!(!ed.item_card_open() && ed.ink.get(id).is_some(), "Escape puts it away and keeps the item");
    // A page key still scrolls the document while the card is open.
    open(&mut ed, id);
    assert!(ed.press(Key::Page(true)) || ed.item_card_open());
}

#[test]
fn there_is_one_card_at_the_right_at_a_time_the_one_opened_last_and_it_is_not_a_press_on_the_page() {
    let mut ed = editor();
    let id = add_box(&mut ed, 100.0, 100.0, "first");
    open(&mut ed, id);
    assert!(ed.item_card_layout().is_some());
    // The search card opened after it puts it away.
    ed.open_search();
    assert!(ed.search_open() && !ed.item_card_open(), "the search card, opened last, is the one that is up");
    // A card opened by a tool (a press on a note) puts the search card away.
    open(&mut ed, id);
    ed.step_view(0.016);
    assert!(ed.item_card_open() && !ed.search_open(), "and the card of an item opened after it puts it away");
    let layout = ed.item_card_layout().expect("card");
    assert!(
        ed.over_interface([(layout.card[0] + layout.card[2]) / 2.0, (layout.card[1] + layout.card[3]) / 2.0]),
        "a press on it is not a press on the page"
    );
}
#[test]
fn what_an_item_says_is_found_by_the_search_and_nothing_else_is() {
    let mut ed = editor();
    add_box(&mut ed, 100.0, 100.0, "remember the milk\nand the eggs");
    add_text(&mut ed, 100.0, 300.0, "milk too");
    ed.open_search();
    ed.press(Key::Text("milk"));
    assert_eq!(ed.search.hits().len(), 2, "the box's words and the text box's");
    ed.press(Key::Text("y"));
    assert!(ed.search.hits().is_empty(), "{:?}", ed.search.hits().iter().map(|h| &h.snippet).collect::<Vec<_>>());
}
