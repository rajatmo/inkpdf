//! The select tool: the text of a page selected by a drag, a word, a line, handles and keys, copied, marked, and every device.

use std::sync::Arc;

use super::*;
use crate::ink::MarkupKind;
use crate::input::Mode;
use crate::pagetext::FakeText;
use crate::tool::ToolAction;
use crate::toolbar::ContextItem;
use crate::tools::Key;

/// Empty lines first, so that the text is below the toolbars: the page is under them at the top.
const TOP: usize = 8;
const LINES: [&str; TOP + 3] = ["", "", "", "", "", "", "", "", "Hello world", "The quick brown fox", "Hello again"];
/// Where the first line with text is, down the page.
const TOP_Y: f32 = TOP as f32 * 20.0;

/// An editor in the select tool, whose first page has `LINES` laid out on a grid (glyph `c` of line `l` is the box from `10 c, 20 l` to
/// `10 c + 10, 20 l + 16` on the page). The lines of the tests are counted from the first with text.
fn reading() -> Editor {
    let mut ed = editor();
    ed.set_text_files(vec![Some(0), Some(1)]);
    ed.add_text(0, Some(Arc::new(FakeText::new(&LINES))));
    ed.add_text(1, None);
    ed.set_tool(Tool::Select);
    ed
}

/// The middle of the glyph `col` of line `line` on page 0, on the screen.
fn glyph(ed: &Editor, line: usize, col: usize) -> [f32; 2] {
    on_page(ed, col as f32 * 10.0 + 5.0, (line + TOP) as f32 * 20.0 + 8.0)
}

fn drag(ed: &mut Editor, from: [f32; 2], to: [f32; 2]) {
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    pen_stroke(ed, TIP, &[from, mid, to]);
}

fn drag_g(ed: &mut Editor, from: (usize, usize), to: (usize, usize)) {
    let (a, b) = (glyph(ed, from.0, from.1), glyph(ed, to.0, to.1));
    drag(ed, a, b);
}

fn selected(ed: &Editor) -> Option<String> {
    ed.tools.select.text(&ed.ws)
}

#[test]
fn a_drag_from_one_letter_to_another_selects_the_text_between_and_copy_puts_it_on_the_clipboard() {
    let mut ed = reading();
    drag_g(&mut ed, (0, 6), (1, 8));
    assert_eq!(selected(&ed).as_deref(), Some("world\nThe quick"));
    assert!(ed.press(Key::Chord('c')) || ed.tools.select.copy_text(&mut ed.ws));
    assert_eq!(ed.clip.text.get().as_deref(), Some("world\nThe quick"));
    // Dragging the other way round selects the same.
    ed.press(Key::Escape);
    drag_g(&mut ed, (1, 8), (0, 6));
    assert_eq!(selected(&ed).as_deref(), Some("world\nThe quick"));
}

#[test]
fn a_drag_that_ends_past_the_text_still_has_an_end_and_one_that_begins_off_the_text_selects_nothing() {
    let mut ed = reading();
    let far = on_page(&ed, 500.0, 300.0 + TOP_Y);
    let start = glyph(&ed, 0, 0);
    drag(&mut ed, start, far);
    assert!(selected(&ed).is_some_and(|t| t.starts_with("Hello world\nThe quick")), "{:?}", selected(&ed));
    let (a, b) = (on_page(&ed, 500.0, 500.0 + TOP_Y), on_page(&ed, 520.0, 520.0 + TOP_Y));
    drag(&mut ed, a, b);
    assert_eq!(selected(&ed), None, "a press where there is no text lets go");
}

#[test]
fn a_second_press_takes_the_word_a_third_the_line_and_a_lone_tap_lets_go() {
    let mut ed = reading();
    let g = glyph(&ed, 1, 5);
    pen_stroke(&mut ed, TIP, &[g, g]);
    assert_eq!(selected(&ed), None, "one tap on a letter selects nothing");
    pen_stroke(&mut ed, TIP, &[g, g]);
    pen_stroke(&mut ed, TIP, &[g, g]);
    // (The three taps carry the same time stamps, so they are a triple.)
    assert_eq!(selected(&ed).as_deref(), Some("The quick brown fox"), "the third press takes the line");
    let mut ed = reading();
    for _ in 0..2 {
        pen_stroke(&mut ed, TIP, &[g, g]);
    }
    assert_eq!(selected(&ed).as_deref(), Some("quick"), "the second takes the word");
}

#[test]
fn the_handles_stretch_what_is_selected_from_either_end_and_only_they_do() {
    let mut ed = reading();
    drag_g(&mut ed, (0, 0), (0, 4));
    assert_eq!(selected(&ed).as_deref(), Some("Hello"));
    // The end handle is at the foot of the last box: pull it on to the end of the word after.
    let end = on_page(&ed, 50.0, 16.0 + TOP_Y);
    let to = glyph(&ed, 0, 10);
    drag(&mut ed, end, to);
    assert_eq!(selected(&ed).as_deref(), Some("Hello world"));
    // The start handle moves the beginning, the other end stays.
    let start = on_page(&ed, 0.0, 16.0 + TOP_Y);
    let to = glyph(&ed, 0, 6);
    drag(&mut ed, start, to);
    assert_eq!(selected(&ed).as_deref(), Some("world"));
}

#[test]
fn escape_lets_go_and_ctrl_a_takes_the_text_of_the_page_in_view() {
    let mut ed = reading();
    assert!(ed.press(Key::Chord('a')));
    assert_eq!(selected(&ed).as_deref(), Some("Hello world\nThe quick brown fox\nHello again"));
    assert!(ed.press(Key::Escape));
    assert_eq!(selected(&ed), None);
    assert!(!ed.press(Key::Escape), "nothing to let go of");
}

#[test]
fn the_three_marks_make_a_markup_of_the_lines_selected_and_it_is_one_step_that_can_be_undone() {
    for (kind, action) in [
        (MarkupKind::Highlight, ToolAction::Mark(MarkupKind::Highlight)),
        (MarkupKind::Underline, ToolAction::Mark(MarkupKind::Underline)),
        (MarkupKind::StrikeOut, ToolAction::Mark(MarkupKind::StrikeOut)),
    ] {
        let mut ed = reading();
        drag_g(&mut ed, (0, 6), (1, 2));
        let bar = ed.context_bar().expect("the bar is there with the text selection");
        let at = bar.centre_of(ContextItem::Action(action));
        pen_stroke(&mut ed, TIP, &[at, at]);
        assert_eq!(ed.ink.len(), 1, "{kind:?}");
        let s = &ed.ink.strokes()[0];
        assert_eq!(s.markup, Some(kind));
        // One box for each of the two lines, over the letters selected, on the page.
        let boxes: Vec<[f32; 4]> = s.quads().collect();
        let y = 12.0 + TOP_Y;
        assert_eq!(boxes, vec![[12.0 + 60.0, y, 12.0 + 110.0, y + 16.0], [12.0, y + 20.0, 12.0 + 30.0, y + 36.0]]);
        assert_eq!(selected(&ed), None, "the markup is what shows now");
        assert!(ed.ink.is_dirty());
        ed.undo();
        assert_eq!(ed.ink.len(), 0);
        ed.redo();
        assert_eq!(ed.ink.len(), 1);
    }
}

#[test]
fn the_marks_and_copy_are_dim_until_text_is_selected() {
    let mut ed = reading();
    let state = ed.context_view().expect("the bar");
    assert!(!state.selection);
    assert!(state.actions.iter().flatten().all(|(_, on)| !on));
    drag_g(&mut ed, (0, 0), (0, 4));
    let state = ed.context_view().expect("the bar");
    assert!(state.selection && state.actions.iter().flatten().all(|(_, on)| *on));
    // Copy from the bar works like the key.
    let at = ed.context_bar().expect("bar").centre_of(ContextItem::Copy);
    pen_stroke(&mut ed, TIP, &[at, at]);
    assert_eq!(ed.clip.text.get().as_deref(), Some("Hello"));
}

#[test]
fn a_press_on_a_markup_selects_it_to_delete_and_a_drag_over_one_still_selects_the_text() {
    let mut ed = reading();
    drag_g(&mut ed, (0, 0), (0, 4));
    let at = ed.context_bar().expect("bar").centre_of(ContextItem::Action(ToolAction::Mark(MarkupKind::Highlight)));
    pen_stroke(&mut ed, TIP, &[at, at]);
    let id = ed.ink.strokes()[0].id;
    let on = glyph(&ed, 0, 2);
    pen_stroke(&mut ed, TIP, &[on, on]);
    assert_eq!(ed.selection, vec![id], "a tap on the markup selects it");
    assert!(ed.press(Key::Delete));
    assert_eq!(ed.ink.len(), 0);
    ed.undo();
    assert_eq!(ed.ink.len(), 1, "deleting it is a step too");
    // A drag that begins on the marked words selects them as text.
    drag_g(&mut ed, (0, 0), (0, 2));
    assert_eq!(selected(&ed).as_deref(), Some("Hel"));
}

#[test]
fn what_is_selected_is_let_go_when_the_page_has_other_text_after_the_pages_were_edited() {
    let mut ed = reading();
    drag_g(&mut ed, (0, 0), (0, 4));
    assert!(selected(&ed).is_some());
    // The pages swap: page 0 is now drawn from the other page of the file.
    ed.set_text_files(vec![Some(1), Some(0)]);
    assert!(!ed.context_view().expect("bar").selection, "the text selected is not on that page any more");
    assert_eq!(selected(&ed), None);
    assert!(!ed.press(Key::Chord('c')), "nothing to copy");
}

#[test]
fn the_mouse_alone_selects_in_the_desktop_mode_and_a_finger_only_pans() {
    let mut ed = reading();
    ed.set_mode(Mode::Desktop);
    ed.mouse_moved(glyph(&ed, 0, 0));
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(glyph(&ed, 0, 4));
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(selected(&ed).as_deref(), Some("Hello"));
    ed.tools.select.clear(&mut ed.ws);
    let (a, b) = (glyph(&ed, 0, 0), glyph(&ed, 0, 4));
    ed.pointer_event(touch(7, Phase::Down, a, 8.0));
    ed.pointer_event(touch(7, Phase::Move, b, 8.0));
    ed.pointer_event(touch(7, Phase::Up, b, 8.0));
    assert_eq!(selected(&ed), None, "a finger does not select text");
}

#[test]
fn a_page_with_no_text_yet_selects_nothing_and_breaks_nothing() {
    let mut ed = editor();
    ed.set_tool(Tool::Select);
    let (a, b) = (on_page(&ed, 10.0, 200.0), on_page(&ed, 100.0, 240.0));
    drag(&mut ed, a, b);
    assert_eq!(selected(&ed), None);
    assert!(!ed.press(Key::Chord('a')));
    assert!(ed.is_idle());
}

#[test]
fn the_desktops_ctrl_a_and_ctrl_c_are_the_texts_with_this_tool() {
    let mut ed = reading();
    ed.set_mode(Mode::Desktop);
    assert!(ed.shortcut("a", false));
    assert!(ed.shortcut("c", false));
    assert_eq!(ed.clip.text.get().as_deref(), Some("Hello world\nThe quick brown fox\nHello again"));
    // With the tool put away they are the items' again: nothing is selected to copy.
    ed.set_tool(Tool::Pen);
    assert_eq!(selected(&ed), None, "putting the tool away lets go of the text");
}

#[test]
fn shift_and_a_press_on_a_letter_stretches_the_selection_to_there_and_a_press_without_shift_begins_a_new_one() {
    let mut ed = reading();
    drag_g(&mut ed, (0, 0), (0, 4));
    assert_eq!(selected(&ed).as_deref(), Some("Hello"));
    ed.handle(crate::input::InputEvent::Shift(true));
    let to = glyph(&ed, 1, 8);
    pen_stroke(&mut ed, TIP, &[to, to]);
    assert_eq!(selected(&ed).as_deref(), Some("Hello world\nThe quick"), "from where it began to the letter pressed");
    // A drag that begins with Shift on goes on stretching.
    let from = glyph(&ed, 1, 12);
    let end = glyph(&ed, 2, 4);
    drag(&mut ed, from, end);
    assert_eq!(selected(&ed).as_deref(), Some("Hello world\nThe quick brown fox\nHello"));
    ed.handle(crate::input::InputEvent::Shift(false));
    let other = glyph(&ed, 1, 6);
    let end = glyph(&ed, 1, 8);
    drag(&mut ed, other, end);
    assert_eq!(selected(&ed).as_deref(), Some("ick"), "without Shift it begins again");
}
