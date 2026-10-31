//! The card that says something went wrong, where the system has no message box: what it says, that it does not time out, that every device puts
//! it away, and that a press on it does not reach the page.

use super::*;
use crate::tools::Key;

fn close_of(ed: &Editor) -> [f32; 2] {
    let l = ed.error_layout().expect("the card");
    [(l.close[0] + l.close[2]) / 2.0, (l.close[1] + l.close[3]) / 2.0]
}

#[test]
fn it_says_the_heading_and_the_reason_in_lines_that_fit_and_a_new_problem_replaces_the_old() {
    let mut ed = editor();
    assert!(!ed.error_open() && ed.error_view().is_none() && ed.error_layout().is_none());
    ed.show_error("Could not save:\nThe disk is full, and the file that was to be written is a long way from fitting on what is left of it.");
    let view = ed.error_view().expect("the card");
    assert_eq!(view.heading, "Could not save:");
    assert!(!view.lines.is_empty() && view.lines.len() <= crate::toolbar::error_lines(), "{:?}", view.lines);
    assert!(view.lines.join(" ").starts_with("The disk is full"));
    let layout = ed.error_layout().expect("the card");
    assert!(layout.card[3] <= ed.view.viewport[1] && layout.card[2] <= ed.view.viewport[0], "inside the window");
    ed.show_error("Could not open x.pdf:\nnot a PDF");
    assert_eq!(ed.error_view().expect("the card").heading, "Could not open x.pdf:", "one at a time");
    ed.show_error("");
    assert!(ed.error_view().expect("the card").lines.is_empty(), "a heading with no reason is a card with none");
    for _ in 0..50 {
        ed.step_view(1.0);
    }
    assert!(ed.error_open(), "it does not go by itself");
}

#[test]
fn the_close_button_puts_it_away_with_the_mouse_the_tip_and_a_finger_and_so_does_escape() {
    let mut ed = editor();
    ed.set_mode(crate::input::Mode::Desktop);
    ed.show_error("Could not save:\nno");
    let at = close_of(&ed);
    mouse_click_at(&mut ed, at);
    assert!(!ed.error_open(), "the left button");
    ed.show_error("Could not save:\nno");
    let at = close_of(&ed);
    pen_tap(&mut ed, at);
    assert!(!ed.error_open(), "the tip");
    ed.show_error("Could not save:\nno");
    assert!(ed.press(Key::Escape) && !ed.error_open(), "the keyboard");
    let mut touched = editor();
    touched.show_error("Could not save:\nno");
    let at = close_of(&touched);
    finger_tap(&mut touched, at);
    assert!(!touched.error_open(), "a finger");
}

#[test]
fn a_press_on_the_card_that_is_not_on_its_button_does_nothing_and_does_not_draw_on_the_page() {
    let mut ed = editor();
    ed.set_mode(crate::input::Mode::Desktop);
    ed.show_error("Could not save:\nThe disk is full");
    let l = ed.error_layout().expect("the card");
    let blank = [l.card[0] + 3.0, l.card[3] - 3.0];
    assert!(ed.over_interface(blank));
    mouse_click_at(&mut ed, blank);
    pen_tap(&mut ed, blank);
    assert!(ed.error_open(), "only the button puts it away");
    assert_eq!(ed.ink.len(), 0, "nothing was drawn under it");
    let page = on_page(&ed, 300.0, 400.0);
    assert!(!ed.over_interface(page), "the page is still the page");
}
