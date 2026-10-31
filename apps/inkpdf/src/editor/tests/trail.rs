//! Browsing through the document like the web: the pill that opens at the first jump, going back and forward, and what forgets the places.

use super::*;
use crate::input::Mode;
use crate::links::{Link, Target};
use crate::toolbar::TrailItem;
use crate::tools::Key;

/// A link over a band of page 0 (below the bars) that goes to `target`.
fn with_link(ed: &mut Editor, target: Target) {
    ed.links = vec![Link { rect: [112.0, 302.0, 312.0, 322.0], target }];
}

fn follow(ed: &mut Editor) {
    let on = on_page(ed, 150.0, 300.0);
    finger_tap(ed, on);
    settle(ed);
}

fn top(ed: &Editor) -> f32 {
    ed.view.scroll_target_y()
}

#[test]
fn nothing_is_there_until_a_link_is_followed_and_then_the_pill_opens_and_back_and_forward_go_between_the_places() {
    let mut ed = editor();
    with_link(&mut ed, Target::Page(1));
    assert!(ed.trail_layout().is_none() && ed.trail_view().is_none(), "no pill before the first jump");
    let home = top(&ed);
    follow(&mut ed);
    let there = top(&ed);
    assert!(there > home + 500.0, "the link took the reader to page 2: {home} -> {there}");
    let view = ed.trail_view().expect("the pill opened at the first jump");
    assert_eq!((view.can_back, view.can_forward), (true, false));
    assert!(ed.go_trail(true), "back");
    settle(&mut ed);
    assert!((top(&ed) - home).abs() < 1.0, "back where the reader was: {} vs {home}", top(&ed));
    assert_eq!(ed.trail_view().map(|v| (v.can_back, v.can_forward)), Some((false, true)), "the pill stays, with back dim and forward lit");
    assert!(!ed.go_trail(true), "nowhere further back");
    assert!(ed.go_trail(false), "forward");
    settle(&mut ed);
    assert!((top(&ed) - there).abs() < 1.0);
    assert!(!ed.go_trail(false), "nowhere further forward");
}

#[test]
fn a_place_is_kept_as_it_was_left_so_scrolling_before_going_on_and_back_again_comes_back_to_where_the_reader_was() {
    let mut ed = editor();
    with_link(&mut ed, Target::Page(1));
    ed.pan_px(0.0, -120.0);
    let left_at = top(&ed);
    follow(&mut ed);
    ed.go_trail(true);
    settle(&mut ed);
    assert!((top(&ed) - left_at).abs() < 1.0, "back to where the link was followed from, not to the top of the page");
}

#[test]
fn a_new_jump_after_going_back_forgets_what_was_ahead_and_scrolling_by_hand_is_not_a_jump() {
    let mut ed = editor();
    with_link(&mut ed, Target::Page(1));
    follow(&mut ed);
    ed.go_trail(true);
    settle(&mut ed);
    assert!(ed.trail_view().is_some_and(|v| v.can_forward));
    // Scrolling is nobody's business but the reader's: it adds no place.
    ed.pan_px(0.0, -50.0);
    ed.scroll_by_px(0.0, -200.0);
    settle(&mut ed);
    assert!(ed.trail_view().is_some_and(|v| v.can_forward && !v.can_back), "still the same two places");
    // A jump made from here: what was ahead is gone, and the place left is a place to go back to.
    ed.jumping(|e| e.ws.scroll_to_page(1));
    settle(&mut ed);
    assert!(ed.trail_view().is_some_and(|v| !v.can_forward && v.can_back), "going on from a place after going back forgets the way forward");
}
#[test]
fn alt_and_an_arrow_and_the_buttons_of_the_pill_go_with_every_device_and_none_of_it_draws_on_the_page() {
    let mut ed = editor();
    ed.set_mode(Mode::Desktop);
    with_link(&mut ed, Target::Page(1));
    let home = top(&ed);
    follow(&mut ed);
    // The keyboard (Alt and an arrow).
    assert!(ed.press(Key::Trail(false)));
    settle(&mut ed);
    assert!((top(&ed) - home).abs() < 1.0);
    assert!(ed.press(Key::Trail(true)));
    settle(&mut ed);
    // The left button of the mouse.
    let pill = ed.trail_layout().expect("the pill");
    mouse_click_at(&mut ed, pill.centre_of(TrailItem::Back));
    settle(&mut ed);
    assert!((top(&ed) - home).abs() < 1.0, "the mouse");
    // The stylus's tip.
    pen_tap(&mut ed, pill.centre_of(TrailItem::Forward));
    settle(&mut ed);
    assert!(top(&ed) > home + 500.0, "the tip");
    assert_eq!(ed.ink.len(), 0, "none of it drew on the page");
    // A finger, in an editor the pen has not been near.
    let mut touched = editor();
    with_link(&mut touched, Target::Page(1));
    follow(&mut touched);
    let at = touched.trail_layout().expect("the pill").centre_of(TrailItem::Back);
    finger_tap(&mut touched, at);
    settle(&mut touched);
    assert!((top(&touched) - home).abs() < 1.0, "a finger");
    assert!(touched.over_interface(at), "a press on the pill is not a press on the page");
}

#[test]
fn editing_the_pages_or_opening_a_document_forgets_the_places() {
    let mut ed = editor();
    with_link(&mut ed, Target::Page(1));
    follow(&mut ed);
    assert!(ed.trail_view().is_some());
    ed.insert_page_after(0);
    assert!(ed.trail_view().is_none() && !ed.go_trail(true), "a page was added: the places are not where they were");
    follow(&mut ed);
    ed.load(crate::view::Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(ed.trail_view().is_none(), "a new document has no places");
}

#[test]
fn a_page_pressed_in_the_overview_and_a_search_result_pressed_are_jumps_too_and_stepping_through_results_is_not() {
    let mut ed = editor();
    ed.set_text_files(vec![Some(0), Some(1)]);
    ed.add_text(0, Some(std::sync::Arc::new(crate::pagetext::FakeText::new(&["Hello world"]))));
    ed.add_text(1, Some(std::sync::Arc::new(crate::pagetext::FakeText::new(&["Hello again", "far away"]))));
    // (Zoomed in, so that the second page is out of the window: a result that is in view already takes the reader nowhere.)
    ed.view.zoom = 2.0;
    ed.pan_px(0.0, 0.0);
    ed.open_search();
    ed.press(Key::Text("hello"));
    // (Next and previous walk through the places found: that is reading, not jumping.)
    ed.search_next();
    settle(&mut ed);
    assert!(ed.trail_view().is_none(), "stepping through results adds no place");
    // (Stepping went to the second result; the first one pressed takes the reader back up the page.)
    ed.press_search_item(crate::toolbar::SearchItem::Row(0));
    settle(&mut ed);
    assert!(ed.trail_view().is_some_and(|v| v.can_back), "a result pressed that took the reader to another page is a place to go back from");
}

#[test]
fn the_places_kept_are_bounded_and_bad_numbers_move_nothing() {
    let mut ed = editor();
    for i in 0..300 {
        with_link(&mut ed, Target::Page(i % 2));
        // Alternate between the two pages: each jump is a place.
        let to = if i % 2 == 0 { 1 } else { 0 };
        ed.jumping(|e| e.ws.scroll_to_page(to));
        settle(&mut ed);
    }
    let mut back = 0;
    while ed.go_trail(true) {
        back += 1;
        settle(&mut ed);
    }
    assert!(back <= 100, "at most 100 places are kept: {back}");
    assert!(ed.view.scroll_target_y().is_finite());
}
