//! The on-screen keyboard: it covers the lowest part of the window, and what is typed there is brought above it and put back when it goes.

use super::*;
use crate::input::Mode;

/// A tablet with the text tool, and a box tapped near the bottom of the window (page y 640 of the 900 pixels the window has).
fn typing_low() -> Editor {
    let mut ed = editor();
    ed.set_mode(Mode::Tablet);
    ed.set_tool(Tool::Text);
    let at = on_page(&ed, 100.0, 640.0);
    pen_tap(&mut ed, at);
    ed.type_text("Hello");
    assert!(ed.is_typing());
    ed
}

/// Where the bottom of the box being typed in is on the screen, now.
fn box_bottom(ed: &Editor) -> f32 {
    let edit = ed.text_edit().expect("a box");
    let low = edit.outline().iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    ed.view.doc_to_screen([0.0, low])[1]
}

#[test]
fn what_is_typed_under_the_keyboard_is_brought_above_it_and_put_back_when_the_keyboard_goes() {
    let mut ed = typing_low();
    let (before, y0) = (box_bottom(&ed), ed.view.scroll_target_y());
    assert!(before > 600.0, "it is where the keyboard will be: {before}");
    ed.set_keyboard_covered(500.0);
    ed.step_view(0.016);
    settle(&mut ed);
    assert!(box_bottom(&ed) <= 900.0 - 500.0, "above the keyboard: {}", box_bottom(&ed));
    assert!(ed.view.scroll_target_y() > y0, "the view went down the page");
    // Typing more lines lifts it further, a line at a time.
    ed.type_text("\nand a second line\nand a third");
    ed.step_view(0.016);
    settle(&mut ed);
    assert!(box_bottom(&ed) <= 400.0, "still clear: {}", box_bottom(&ed));
    // The keyboard goes: the view is where it was.
    ed.set_keyboard_covered(0.0);
    ed.step_view(0.016);
    settle(&mut ed);
    assert!((ed.view.scroll_target_y() - y0).abs() < 0.5, "back to {y0}: {}", ed.view.scroll_target_y());
}

#[test]
fn a_person_who_scrolls_with_the_keyboard_up_is_not_pulled_back_and_what_they_scrolled_to_is_kept_when_it_goes() {
    let mut ed = typing_low();
    ed.set_keyboard_covered(500.0);
    ed.step_view(0.016);
    settle(&mut ed);
    let lifted = ed.view.scroll_target_y();
    ed.pan_px(0.0, 150.0);
    ed.step_view(0.016);
    settle(&mut ed);
    assert!(ed.view.scroll_target_y() < lifted, "they scrolled up, and nothing took it back");
    let theirs = ed.view.scroll_target_y();
    ed.set_keyboard_covered(0.0);
    ed.step_view(0.016);
    settle(&mut ed);
    assert!((ed.view.scroll_target_y() - theirs).abs() < 0.5, "where they put it is theirs");
}

#[test]
fn nothing_moves_for_a_keyboard_when_nothing_is_typed_or_what_is_typed_is_clear_and_bad_numbers_cover_nothing() {
    let mut ed = editor();
    let y0 = ed.view.scroll_target_y();
    ed.set_keyboard_covered(500.0);
    ed.step_view(0.016);
    assert_eq!(ed.view.scroll_target_y(), y0, "no box, no move");
    ed.set_keyboard_covered(0.0);
    ed.set_tool(Tool::Text);
    let at = on_page(&ed, 100.0, 100.0);
    pen_tap(&mut ed, at);
    ed.type_text("high up");
    let y1 = ed.view.scroll_target_y();
    ed.set_keyboard_covered(500.0);
    ed.step_view(0.016);
    assert_eq!(ed.view.scroll_target_y(), y1, "already clear");
    for bad in [f32::NAN, f32::INFINITY, -50.0] {
        ed.set_keyboard_covered(bad);
        ed.step_view(0.016);
        assert!(ed.keyboard_covered().is_finite() && ed.view.scroll_target_y().is_finite(), "{bad}");
    }
    assert!(ed.keyboard_covered() <= ed.view.viewport[1]);
}
