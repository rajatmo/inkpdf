//! The reader: the wheel and the keys of any PDF program, the Select tool's drag on empty page, and the cursor the mouse shows (which is the
//! rules' answer for what a press would do).

use crate::input::{CursorKind, InputEvent, Key, KeyInput, Mode};
use crate::links::{Link, Target};

use super::*;

/// The editor's window, with the mouse over the middle of the first page.
fn over_the_page() -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Select);
    ed.mouse_moved(on_page(&ed, 300.0, 400.0));
    ed
}

#[test]
fn the_wheel_with_ctrl_zooms_about_the_pointer_and_stops_following_the_width() {
    let mut ed = over_the_page();
    ed.fit_mode = true;
    let at = ed.cursor;
    let before = (ed.view.zoom, ed.view.screen_to_doc(at));
    ed.handle(InputEvent::Zoom(2.0));
    assert!((ed.view.zoom / before.0 - 1.21).abs() < 1e-3, "two notches are two tenths: {}", ed.view.zoom);
    let after = ed.view.screen_to_doc(at);
    // (Only along the page: a page narrower than the window is kept in the middle of it.)
    assert!((after[1] - before.1[1]).abs() < 0.5, "what was under the pointer still is: {before:?} {after:?}");
    assert!(!ed.fit_mode, "the page no longer follows the width of the window");
    ed.handle(InputEvent::Zoom(-2.0));
    assert!((ed.view.zoom - before.0).abs() < 1e-3, "and back");
    ed.handle(InputEvent::Zoom(f32::NAN));
    assert!((ed.view.zoom - before.0).abs() < 1e-3, "nonsense does nothing");
}

#[test]
fn the_wheel_with_ctrl_does_nothing_while_the_overview_is_up() {
    let mut ed = over_the_page();
    ed.open_overview();
    let zoom = ed.view.zoom;
    ed.handle(InputEvent::Zoom(3.0));
    assert_eq!(ed.view.zoom, zoom);
}

#[test]
fn ctrl_and_plus_minus_zero_and_one_zoom_fit_the_width_and_show_the_page_at_its_size() {
    let mut ed = editor();
    let start = ed.view.zoom;
    assert!(ed.shortcut("+", false) && ed.view.zoom > start);
    assert!(ed.shortcut("=", false), "the plus key without Shift is the equals key");
    let bigger = ed.view.zoom;
    assert!(ed.shortcut("-", false) && ed.view.zoom < bigger);
    assert!(ed.shortcut("0", false));
    assert!(ed.fit_mode && (ed.view.zoom - ed.view.fit_width_zoom(&ed.layout)).abs() < 1e-3, "the width of the window");
    assert!(ed.shortcut("1", false));
    assert!(!ed.fit_mode && (ed.view.zoom - ed.dpr).abs() < 1e-3, "a point is a pixel");
    // At a desk only: a tablet has no keyboard to rely on.
    let mut slate = editor();
    slate.set_mode(Mode::Tablet);
    assert!(!slate.shortcut("+", false));
}

#[test]
fn page_keys_home_and_end_scroll_the_document_with_every_tool_and_never_while_typing() {
    for tool in [Tool::Select, Tool::Pen, Tool::Geometry, Tool::Lasso, Tool::Eraser] {
        // (Four pages: room for a whole screen of scrolling.)
        let mut ed = editor();
        ed.load(Layout::new(&[[612.0, 792.0]; 4]), vec![]);
        ed.fit_mode = false;
        ed.view.zoom = 1.0;
        ed.pan_px(0.0, 0.0);
        ed.set_tool(tool);
        assert!(ed.press(Key::Page(true)), "{tool:?}");
        settle(&mut ed);
        let screen = 0.9 * ed.view.viewport[1] / ed.view.zoom;
        assert!((ed.view.scroll[1] - screen).abs() < 1.0, "{tool:?}: a screen down: {}", ed.view.scroll[1]);
        assert!(ed.press(Key::Page(false)));
        settle(&mut ed);
        assert!(ed.view.scroll[1].abs() < 1.0, "{tool:?}: and a screen up");
        assert!(ed.press(Key::End(false)));
        settle(&mut ed);
        assert!(ed.view.at_end(&ed.layout), "{tool:?}: the end");
        assert!(ed.press(Key::Home(false)));
        settle(&mut ed);
        assert!(ed.view.scroll[1].abs() < 1.0, "{tool:?}: the top");
        assert!(ed.press(Key::Arrow(crate::input::Dir::Down, false)));
        settle(&mut ed);
        assert!(ed.view.scroll[1] > 1.0, "{tool:?}: the arrows scroll a little");
    }
    // A text box being typed into keeps its keys: the document stays where it is.
    let mut ed = editor();
    ed.set_tool(Tool::Text);
    let at = on_page(&ed, 100.0, 100.0);
    mouse_click_at(&mut ed, at);
    assert!(ed.is_typing());
    assert!(!ed.press(Key::Page(true)));
    settle(&mut ed);
    assert!(ed.view.scroll[1].abs() < 1.0);
}

#[test]
fn the_arrows_nudge_a_selection_before_they_scroll_and_a_tablet_has_no_reader_keys() {
    let mut ed = editor();
    ed.set_tool(Tool::Pen);
    let a = on_page(&ed, 100.0, 100.0);
    pen_stroke(&mut ed, TIP, &[a, [a[0] + 80.0, a[1] + 30.0]]);
    ed.shortcut("a", false);
    let before = ed.view.scroll[1];
    assert!(ed.press(Key::Arrow(crate::input::Dir::Down, false)));
    settle(&mut ed);
    assert_eq!(ed.view.scroll[1], before, "the arrow moved what was selected, not the page");
    let mut slate = editor();
    slate.set_mode(Mode::Tablet);
    assert!(!slate.press(Key::Page(true)));
}

#[test]
fn the_select_tools_drag_on_empty_page_pans_with_the_mouse_and_the_stylus_and_a_tap_only_lets_go() {
    // The mouse: the left button.
    let mut ed = over_the_page();
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(on_page(&ed, 300.0, 300.0));
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert!(ed.view.scroll[1] > 90.0 && ed.view.scroll[1] < 110.0, "the page followed the hand: {}", ed.view.scroll[1]);
    assert_eq!(ed.ink.len(), 0, "and nothing was drawn");
    // The stylus's tip.
    let mut ed = editor();
    ed.set_tool(Tool::Select);
    let from = on_page(&ed, 300.0, 400.0);
    pen_stroke(&mut ed, TIP, &[from, [from[0], from[1] - 120.0]]);
    assert!(ed.view.scroll[1] > 110.0 && ed.view.scroll[1] < 130.0, "{}", ed.view.scroll[1]);
    // A tap does not move it.
    let mut ed = over_the_page();
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(ed.view.scroll[1], 0.0);
    // With a drawing tool the same drag draws and the page stays.
    let mut ed = editor();
    ed.set_tool(Tool::Pen);
    ed.mouse_moved(on_page(&ed, 300.0, 400.0));
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(on_page(&ed, 300.0, 300.0));
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!((ed.view.scroll[1], ed.ink.len()), (0.0, 1), "ink cannot wait: the pen draws");
}

#[test]
fn the_middle_button_pans_in_the_reader_and_loops_in_a_drawing_tool() {
    let mut ed = over_the_page();
    ed.mouse_button(ButtonState::Pressed, MouseButton::Middle);
    ed.mouse_moved(on_page(&ed, 300.0, 250.0));
    ed.mouse_button(ButtonState::Released, MouseButton::Middle);
    assert!(ed.view.scroll[1] > 140.0, "panned: {}", ed.view.scroll[1]);
    let mut ed = editor();
    ed.set_tool(Tool::Pen);
    ed.mouse_moved(on_page(&ed, 300.0, 400.0));
    ed.mouse_button(ButtonState::Pressed, MouseButton::Middle);
    ed.mouse_moved(on_page(&ed, 300.0, 250.0));
    ed.mouse_button(ButtonState::Released, MouseButton::Middle);
    assert_eq!(ed.view.scroll[1], 0.0, "the page stays: the middle button is the barrel button there");
}

#[test]
fn the_cursor_follows_the_rules_for_the_tool_the_mode_and_what_is_under_the_mouse() {
    let mut ed = over_the_page();
    assert_eq!(ed.cursor_kind(), CursorKind::Grab, "the reader's empty page can be dragged");
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    assert_eq!(ed.cursor_kind(), CursorKind::Grabbing, "and is, while it is");
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    for (tool, want) in [
        (Tool::Pen, CursorKind::Dot),
        (Tool::Geometry, CursorKind::Crosshair),
        (Tool::Eraser, CursorKind::Eraser),
        (Tool::Lasso, CursorKind::Crosshair),
        (Tool::Text, CursorKind::Crosshair),
    ] {
        ed.set_tool(tool);
        assert_eq!(ed.cursor_kind(), want, "{tool:?}");
    }
    // Over the toolbar it is the arrow, whatever the tool.
    ed.set_tool(Tool::Pen);
    ed.mouse_moved(ed.toolbar().centre_of(Item::Undo));
    assert_eq!(ed.cursor_kind(), CursorKind::Arrow);
    // The mouse of a tablet only scrolls.
    let mut slate = over_the_page();
    slate.set_mode(Mode::Tablet);
    assert_eq!(slate.cursor_kind(), CursorKind::Arrow);
    // The pages overview has the pointer: the arrow.
    let mut ed = over_the_page();
    ed.open_overview();
    assert_eq!(ed.cursor_kind(), CursorKind::Arrow);
}

#[test]
fn a_link_is_a_hand_in_the_reader_and_paper_to_a_drawing_tool() {
    let mut ed = over_the_page();
    let spot = ed.view.screen_to_doc(ed.cursor);
    ed.links.push(Link { rect: [spot[0] - 30.0, spot[1] - 10.0, spot[0] + 30.0, spot[1] + 10.0], target: Target::Page(1) });
    assert_eq!(ed.cursor_kind(), CursorKind::Hand);
    ed.set_tool(Tool::Pen);
    assert_eq!(ed.cursor_kind(), CursorKind::Dot, "ink cannot wait: a drawing tool does not read");
    // Over ink the Lasso shows what it will do: take hold of it.
    ed.links.clear();
    ed.set_tool(Tool::Lasso);
    assert_eq!(ed.cursor_kind(), CursorKind::Crosshair);
    ed.set_tool(Tool::Pen);
    let a = ed.cursor;
    pen_stroke(&mut ed, TIP, &[[a[0] - 20.0, a[1]], [a[0] + 20.0, a[1]]]);
    ed.set_tool(Tool::Lasso);
    ed.mouse_moved(a);
    assert_eq!(ed.cursor_kind(), CursorKind::Move);
}

#[test]
fn the_new_events_survive_a_recording() {
    let events = [InputEvent::Zoom(1.5), InputEvent::Key(KeyInput::Page(true)), InputEvent::Key(KeyInput::Page(false))];
    for event in events {
        let mut ed = over_the_page();
        ed.handle(event.clone());
        ed.handle(event);
    }
}

/// One finger taps at `at`, quickly.
#[test]
fn a_double_tap_fits_the_page_to_the_width_and_the_next_one_goes_back_and_a_single_tap_does_nothing_more() {
    let mut ed = editor();
    ed.pan_px(0.0, -300.0);
    let (zoom, scroll) = (ed.view.zoom, ed.view.scroll);
    let at = [600.0, 450.0];
    finger_tap(&mut ed, at);
    assert!(!ed.fit_mode && ed.view.zoom == zoom, "one tap fits nothing: it never waits for a second");
    finger_tap(&mut ed, at);
    assert!(ed.fit_mode && (ed.view.zoom - ed.view.fit_width_zoom(&ed.layout)).abs() < 1e-3, "two fit the width: {}", ed.view.zoom);
    finger_tap(&mut ed, at);
    finger_tap(&mut ed, at);
    assert!(!ed.fit_mode && (ed.view.zoom - zoom).abs() < 1e-3, "the next two go back: {}", ed.view.zoom);
    assert!((ed.view.scroll[1] - scroll[1]).abs() < 1.0, "to the place it was: {:?} {:?}", ed.view.scroll, scroll);
}

#[test]
fn a_pinch_in_between_makes_the_remembered_view_forgotten_and_a_far_second_tap_is_not_a_double() {
    let mut ed = editor();
    let at = [600.0, 450.0];
    finger_tap(&mut ed, at);
    finger_tap(&mut ed, at);
    assert!(ed.fit_mode);
    ed.fit_mode = false;
    ed.view.zoom = 3.0;
    finger_tap(&mut ed, at);
    finger_tap(&mut ed, at);
    assert!(ed.fit_mode, "after a pinch a double tap fits again, it does not go back to a zoom that was left");
    // Two taps far apart are two taps.
    let mut ed = editor();
    finger_tap(&mut ed, at);
    finger_tap(&mut ed, [900.0, 700.0]);
    assert!(!ed.fit_mode);
}
