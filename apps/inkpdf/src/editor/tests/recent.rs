//! The PDFs opened last: the clock button of the document's pill and the card it shows, with every device, and a document shown again as it was left.

use super::*;
use crate::recent::Place;
use crate::toolbar::RecentRow;
use crate::tools::Key;

fn rows(n: usize) -> Vec<RecentRow> {
    (0..n).map(|i| RecentRow { name: format!("paper {i}.pdf"), folder: "C:\\papers".into(), current: i == 0 }).collect()
}

fn with_list(n: usize) -> Editor {
    let mut ed = editor();
    ed.set_recent(rows(n));
    ed
}

/// The document y a place puts at the top of the window (a place in the gap above a page can be told as the end of the page before).
fn top_of(ed: &Editor, place: Place) -> f32 {
    ed.layout.page(place.page).expect("the page").y + place.dy
}

fn button(ed: &Editor) -> [f32; 2] {
    ed.toolbar().centre_of(Item::Recent)
}

fn row(ed: &Editor, i: usize) -> [f32; 2] {
    ed.recent_layout().expect("the card").centre_of(i)
}

#[test]
fn the_button_shows_the_card_and_a_press_on_a_row_asks_for_that_document_with_every_device() {
    type Tap = fn(&mut Editor, [f32; 2]);
    let taps: [(&str, Tap); 3] = [("the stylus", pen_tap), ("a finger", finger_tap), ("the mouse", mouse_click_at)];
    for (device, tap) in taps {
        let mut ed = with_list(4);
        assert!(ed.recent_layout().is_none() && ed.recent_view().is_none(), "put away until it is asked for");
        let at = button(&ed);
        tap(&mut ed, at);
        assert!(ed.recent_open(), "{device} shows it");
        let view = ed.recent_view().expect("the card");
        assert_eq!((view.rows.len(), view.pressed), (4, None));
        assert!(ed.toolbar_state().recent, "the button says it is shown");
        let at = row(&ed, 2);
        tap(&mut ed, at);
        assert!(!ed.recent_open(), "{device}: a row pressed puts the card away");
        assert!(ed.requests().take(&Request::OpenRecent(2)), "{device} asks for the third document");
        assert!(ed.ink.len() == 0, "{device} drew nothing through the card");
    }
}

#[test]
fn the_button_again_esc_or_a_press_anywhere_else_puts_the_card_away_and_asks_for_nothing() {
    let mut ed = with_list(3);
    let at = button(&ed);
    pen_tap(&mut ed, at);
    let at = button(&ed);
    pen_tap(&mut ed, at);
    assert!(!ed.recent_open(), "the button again");
    let at = button(&ed);
    pen_tap(&mut ed, at);
    assert!(ed.press(Key::Escape) && !ed.recent_open(), "Esc");
    assert!(!ed.press(Key::Escape) || !ed.recent_open());
    let at = button(&ed);
    mouse_click_at(&mut ed, at);
    let page = on_page(&ed, 500.0, 600.0);
    assert!(!ed.recent_layout().expect("the card").contains(page), "a place on the page, beside the card");
    mouse_click_at(&mut ed, page);
    assert!(!ed.recent_open(), "a press on the page");
    let at = button(&ed);
    mouse_click_at(&mut ed, at);
    let at = ed.toolbar().centre_of(Item::Pages);
    mouse_click_at(&mut ed, at);
    assert!(!ed.recent_open() && ed.overview_open(), "another button does what it does, and the card is gone");
    assert!(!ed.requests().has(&Request::OpenRecent(0)));
}

#[test]
fn a_press_that_leaves_its_row_opens_nothing_and_the_corner_of_the_card_is_not_a_row() {
    let mut ed = with_list(3);
    let at = button(&ed);
    pen_tap(&mut ed, at);
    let (a, b) = (row(&ed, 0), row(&ed, 1));
    ed.pointer_event(pen(Phase::Down, TIP, &[a]));
    assert_eq!(ed.recent_view().expect("the card").pressed, Some(0), "the row shows it is pressed");
    ed.pointer_event(pen(Phase::Move, TIP, &[b]));
    ed.pointer_event(pen(Phase::Up, TIP, &[b]));
    assert!(ed.recent_open() && !ed.requests().has(&Request::OpenRecent(0)) && !ed.requests().has(&Request::OpenRecent(1)));
    let card = ed.recent_layout().expect("the card").card;
    pen_tap(&mut ed, [card[0] + 2.0, card[1] + 2.0]);
    assert!(ed.recent_open(), "a press on the card that is on no row leaves it up");
    assert!(ed.ink.len() == 0, "and is not a stroke on the page under it");
}

#[test]
fn with_no_documents_the_button_is_dim_and_shows_nothing_and_a_list_that_empties_puts_the_card_away() {
    let mut ed = editor();
    let state = ed.toolbar_state();
    assert!(
        !state.has_recent
            && !crate::toolbar::Look::resting(&state, &ed.toolbar()).enabled[ed.toolbar().slot(Item::Recent).expect("the button")].eq(&1.0)
    );
    let at = button(&ed);
    pen_tap(&mut ed, at);
    assert!(!ed.recent_open() && ed.recent_layout().is_none());
    ed.set_recent(rows(2));
    assert!(ed.toolbar_state().has_recent);
    let at = button(&ed);
    pen_tap(&mut ed, at);
    assert!(ed.recent_open());
    ed.set_recent(Vec::new());
    assert!(!ed.recent_open() && ed.recent_layout().is_none());
}

#[test]
fn the_card_and_the_cards_at_the_right_are_not_up_together_and_a_modal_layer_or_a_new_document_puts_it_away() {
    let mut ed = with_list(3);
    ed.open_search();
    let at = button(&ed);
    pen_tap(&mut ed, at);
    assert!(ed.recent_open() && !ed.search_open(), "it puts the search card away");
    ed.shortcut("f", false);
    assert!(!ed.recent_open() && ed.search_open(), "and the search card puts it away");
    ed.close_search();
    let at = button(&ed);
    pen_tap(&mut ed, at);
    ed.open_overview();
    assert!(!ed.recent_open());
    ed.press(Key::Escape);
    assert!(!ed.overview_open() && ed.recent_layout().is_none(), "it does not come back with the page");
    let at = button(&ed);
    pen_tap(&mut ed, at);
    ed.start_presentation();
    ed.stop_presentation();
    assert!(!ed.recent_open());
    let at = button(&ed);
    pen_tap(&mut ed, at);
    ed.ask_unsaved();
    assert!(!ed.recent_open());
    let mut ed = with_list(3);
    let at = button(&ed);
    pen_tap(&mut ed, at);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.recent_open(), "a document opened");
    assert_eq!(ed.recent_view(), None);
}

#[test]
fn the_card_is_under_the_back_and_forward_pill_and_in_a_folded_pill_the_button_folds_it_and_the_card_hangs_from_its_round_button() {
    let mut ed = with_list(10);
    ed.jumping(|e| e.ws.scroll_to_page(1));
    let trail = ed.trail_layout().expect("the pill").card;
    let at = button(&ed);
    pen_tap(&mut ed, at);
    let card = ed.recent_layout().expect("the card").card;
    assert!(card[1] >= trail[3], "{card:?} under {trail:?}");
    // A phone: the document's pill is a round button.
    let mut phone = Editor::reference([400.0, 800.0], 1.0, crate::motion::Prefs::REDUCED);
    phone.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0]]), vec![]);
    phone.set_recent(rows(10));
    assert!(phone.toolbar().slot(Item::Recent).is_none(), "folded away");
    let at = phone.toolbar().centre_of(Item::Fold(crate::toolbar::Pill::Document));
    finger_tap(&mut phone, at);
    let at = phone.toolbar().centre_of(Item::Recent);
    finger_tap(&mut phone, at);
    assert!(phone.recent_open() && phone.chrome.pill.is_none(), "the pill folds again, and the card is shown");
    let (card, round) = (phone.recent_layout().expect("the card"), phone.toolbar().round_of(crate::toolbar::Pill::Document));
    assert!(card.card[1] > round[3] && card.card[2] <= 400.0 && card.card[3] <= 800.0, "{:?}", card.card);
    assert_eq!(card.row_count(), 10);
    let at = card.centre_of(9);
    finger_tap(&mut phone, at);
    assert!(phone.requests().take(&Request::OpenRecent(9)));
}

#[test]
fn where_the_document_is_looked_at_is_a_place_it_can_be_shown_at_again() {
    let mut ed = editor();
    ed.load(Layout::new(&[[612.0, 792.0]; 5]), vec![]);
    ed.fit_mode = false;
    ed.zoom_about(1.5, [0.0, 0.0]);
    ed.pan_px(-40.0, -2000.0);
    let place = ed.place();
    assert!(place.page >= 1 && !place.fit && place.zoom == 1.5, "{place:?}");
    let (zoom, scroll) = (ed.view.zoom, ed.view.scroll);
    // The document opened again begins fitted to the width at its top, and is then shown as it was left.
    ed.load(Layout::new(&[[612.0, 792.0]; 5]), vec![]);
    assert!(ed.fit_mode && ed.view.scroll[1] != scroll[1]);
    ed.show_place(place);
    assert_eq!((ed.view.zoom, ed.fit_mode), (zoom, false));
    assert!((ed.view.scroll[0] - scroll[0]).abs() < 0.01 && (ed.view.scroll[1] - scroll[1]).abs() < 0.01, "{:?} vs {scroll:?}", ed.view.scroll);
    assert!(!ed.view.animating(), "at once, with no glide");
    assert_eq!(ed.place(), place);
}

#[test]
fn a_zoom_that_followed_the_width_follows_the_width_of_the_window_it_is_shown_in_now() {
    let mut ed = editor();
    ed.load(Layout::new(&[[612.0, 792.0]; 5]), vec![]);
    ed.pan_px(0.0, -1500.0);
    let place = ed.place();
    assert!(place.fit);
    let mut narrow = Editor::reference([600.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    narrow.load(Layout::new(&[[612.0, 792.0]; 5]), vec![]);
    let fitted = narrow.view.zoom;
    narrow.show_place(place);
    assert!(narrow.fit_mode && narrow.view.zoom == fitted && fitted != place.zoom, "the narrow window's own zoom, not the wide one's");
    assert!((top_of(&narrow, narrow.place()) - top_of(&narrow, place)).abs() < 0.01, "the same line of the same page is at the top");
}

#[test]
fn a_place_in_a_document_that_has_changed_is_kept_inside_it_and_a_place_that_is_not_one_changes_nothing() {
    let mut ed = editor();
    ed.load(Layout::new(&[[612.0, 792.0]; 3]), vec![]);
    // The file has fewer pages than it had: its last page, from the top.
    ed.show_place(Place { page: 40, dy: 300.0, x: 0.0, zoom: 1.0, fit: false });
    assert_eq!(ed.view.current_page(&ed.layout), 2, "{:?}", ed.place());
    assert!(top_of(&ed, ed.place()) <= ed.layout.page(2).expect("the page").y, "from its top: {:?}", ed.place());
    // Further down a page than the page is long, or beside it: inside the document all the same.
    ed.show_place(Place { page: 1, dy: 1.0e7, x: -1.0e7, zoom: 1.0e9, fit: false });
    let v = &ed.view;
    assert!(v.zoom.is_finite() && v.scroll.iter().all(|s| s.is_finite()));
    assert!(v.scroll[1] <= ed.layout.height() && v.scroll[0] >= -ed.layout.width());
    let before = (ed.view.zoom, ed.view.scroll, ed.fit_mode);
    ed.show_place(Place { page: 0, dy: f32::NAN, x: 0.0, zoom: 1.0, fit: false });
    assert_eq!((ed.view.zoom, ed.view.scroll), (before.0, before.1));
    // With no pages there is nowhere to be.
    let mut empty = Editor::reference([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    empty.show_place(Place { page: 0, dy: 0.0, x: 0.0, zoom: 1.0, fit: true });
    assert!(empty.view.zoom.is_finite());
}

#[test]
fn while_the_slides_show_the_place_is_the_page_shown_as_it_will_be_when_they_are_left() {
    let mut ed = editor();
    ed.load(Layout::new(&[[612.0, 792.0]; 5]), vec![]);
    ed.start_presentation();
    ed.press(Key::Page(true));
    ed.press(Key::Page(true));
    let place = ed.place();
    assert_eq!((place.page, place.fit), (2, true), "the third page, and the zoom that followed the width before the slides");
    let slide = (ed.view.zoom, ed.view.scroll);
    ed.show_place(Place { page: 0, dy: 0.0, x: 0.0, zoom: 1.0, fit: false });
    assert_eq!((ed.view.zoom, ed.view.scroll), slide, "a slide is not left by it");
    ed.stop_presentation();
    settle(&mut ed);
    assert!(
        (top_of(&ed, ed.place()) - top_of(&ed, place)).abs() < 0.5,
        "what was remembered is where leaving the slides comes to: {:?} vs {place:?}",
        ed.place()
    );
}
