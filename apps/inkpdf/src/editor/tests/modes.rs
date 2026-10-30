//! The two modes. At a desk (desktop) the mouse stands in for a stylus (left button the tip, right the eraser end,
//! middle the barrel button) and the keyboard has shortcuts; held as a slate (tablet) the mouse only scrolls and
//! the stylus also places text. The finger pans and zooms in both.

use super::*;
use crate::geometry::Shape;

fn with(tool: Tool, mode: Mode) -> Editor {
    let mut ed = editor();
    ed.set_tool(tool);
    ed.chrome.panel = None;
    ed.set_mode(mode);
    ed
}

/// Press `button` at the first point, move through the rest, release at the last.
fn mouse_drag(ed: &mut Editor, button: MouseButton, pts: &[[f32; 2]]) {
    ed.mouse_moved(pts[0]);
    ed.mouse_button(ButtonState::Pressed, button);
    for p in &pts[1..] {
        ed.mouse_moved(*p);
    }
    ed.mouse_button(ButtonState::Released, button);
}

fn wave(ed: &Editor) -> Vec<[f32; 2]> {
    vec![on_page(ed, 100.0, 300.0), on_page(ed, 160.0, 340.0), on_page(ed, 220.0, 300.0), on_page(ed, 280.0, 340.0)]
}

#[test]
fn a_desk_is_the_default_and_the_mode_can_change() {
    let mut ed = editor();
    assert_eq!(ed.mode(), Mode::Desktop);
    ed.set_mode(Mode::Tablet);
    assert_eq!(ed.mode(), Mode::Tablet);
    assert_eq!(Mode::parse("Tablet"), Some(Mode::Tablet));
    assert_eq!(Mode::parse("desktop"), Some(Mode::Desktop));
    assert_eq!(Mode::parse("auto"), None);
}

#[test]
fn at_a_desk_the_mouse_draws_with_the_pen_tool_erases_with_the_right_button_and_lassos_with_the_middle() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let path = wave(&ed);
    mouse_drag(&mut ed, MouseButton::Left, &path);
    assert_eq!(ed.ink.len(), 1, "the left button draws");
    assert!(ed.ink.is_dirty());
    // The right button is the eraser end: dragged across the stroke it takes it away.
    let across = [on_page(&ed, 90.0, 320.0), on_page(&ed, 300.0, 320.0)];
    mouse_drag(&mut ed, MouseButton::Right, &across);
    assert_eq!(ed.ink.len(), 0, "the right button erases");
    // The middle button is the barrel button: a loop round a stroke selects it.
    let id = add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
    let ring: Vec<[f32; 2]> =
        [[50.0, 250.0], [350.0, 250.0], [350.0, 350.0], [50.0, 350.0], [50.0, 250.0]].iter().map(|p| on_page(&ed, p[0], p[1])).collect();
    mouse_drag(&mut ed, MouseButton::Middle, &ring);
    assert_eq!(ed.selection, vec![id], "the middle button lassos");
    assert_eq!(ed.ink.len(), 1, "and draws nothing");
}

#[test]
fn at_a_desk_the_mouse_makes_shapes_and_moves_the_selection_with_the_shape_tool() {
    let mut ed = with(Tool::Geometry, Mode::Desktop);
    ed.styles.shape = Shape::Rectangle;
    let corners = [on_page(&ed, 100.0, 200.0), on_page(&ed, 250.0, 320.0)];
    mouse_drag(&mut ed, MouseButton::Left, &corners);
    assert_eq!(ed.ink.len(), 1);
    assert!(ed.ink.strokes()[0].figure.is_some(), "a shape, not handwriting");
    assert_eq!(ed.selection.len(), 1, "selected, ready to be edited");
}

#[test]
fn at_a_desk_the_pointer_tool_follows_the_mouse_with_a_dot_and_a_trail() {
    let mut ed = with(LASER, Mode::Desktop);
    ed.mouse_moved(on_page(&ed, 200.0, 300.0));
    assert!(laser_dot(&ed).is_some(), "the dot follows the hovering mouse");
    let path = wave(&ed);
    mouse_drag(&mut ed, MouseButton::Left, &path);
    assert!(!laser_trail(&ed, 1000.0).is_empty() || laser_fading(&ed), "a pressed mouse leaves a trail");
    assert_eq!(ed.ink.len(), 0, "and draws nothing");
    ed.mouse_left();
    assert_eq!(laser_dot(&ed), None, "the dot goes with the mouse");
}

#[test]
fn the_tablet_mouse_only_scrolls_and_the_wheel_scrolls_in_both_modes() {
    for mode in [Mode::Desktop, Mode::Tablet] {
        let mut ed = with(Tool::Pen, mode);
        let before = ed.view.scroll;
        ed.wheel(Wheel::Lines(0.0, -3.0));
        settle(&mut ed);
        assert_ne!(ed.view.scroll, before, "{mode:?}: the wheel scrolls");
    }
    let mut ed = with(Tool::Pen, Mode::Tablet);
    let path = wave(&ed);
    mouse_drag(&mut ed, MouseButton::Left, &path);
    mouse_drag(&mut ed, MouseButton::Right, &path);
    assert_eq!(ed.ink.len(), 0, "the tablet's mouse draws nothing");
}

#[test]
fn a_finger_pans_in_both_modes_and_never_draws() {
    for mode in [Mode::Desktop, Mode::Tablet] {
        let mut ed = with(Tool::Pen, mode);
        let (a, b) = (on_page(&ed, 300.0, 600.0), on_page(&ed, 300.0, 300.0));
        ed.pointer_event(touch(1, Phase::Down, a, 30.0));
        ed.pointer_event(touch(1, Phase::Move, b, 30.0));
        ed.pointer_event(touch(1, Phase::Up, b, 30.0));
        assert_eq!(ed.ink.len(), 0, "{mode:?}");
    }
}

#[test]
fn a_second_button_pressed_during_a_drag_changes_nothing() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let path = wave(&ed);
    ed.mouse_moved(path[0]);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(path[1]);
    // The right button goes down and up in the middle of the stroke: not an eraser, not the end of the stroke.
    ed.mouse_button(ButtonState::Pressed, MouseButton::Right);
    ed.mouse_button(ButtonState::Released, MouseButton::Right);
    ed.mouse_moved(path[2]);
    ed.mouse_moved(path[3]);
    assert_eq!(ed.ink.len(), 0, "the stroke is still being drawn");
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(ed.ink.len(), 1, "one stroke, drawn whole");
}

#[test]
fn losing_the_focus_ends_what_the_mouse_was_doing() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let path = wave(&ed);
    ed.mouse_moved(path[0]);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(path[1]);
    ed.mouse_moved(path[2]);
    ed.focus_lost();
    assert_eq!(ed.ink.len(), 1, "what was drawn is kept");
    assert!(!ed.tools.busy());
    // The release that never came does not leave the next press ignored.
    mouse_drag(&mut ed, MouseButton::Left, &path);
    assert_eq!(ed.ink.len(), 2);
}

#[test]
fn a_lost_release_does_not_make_the_next_press_of_that_button_be_ignored() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let path = wave(&ed);
    ed.mouse_moved(path[0]);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(path[1]);
    // The release never arrives. The next press is a new one, on the toolbar as much as on the page.
    let laser = ed.toolbar().centre_of(Item::Tool(LASER));
    ed.mouse_moved(laser);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(ed.tool, LASER, "the toolbar button was pressed");
    assert!(!ed.tools.busy(), "and the old stroke is over");
}

#[test]
fn the_other_buttons_do_nothing_on_the_toolbar_or_with_a_panel_open() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let laser = ed.toolbar().centre_of(Item::Tool(LASER));
    mouse_drag(&mut ed, MouseButton::Right, &[laser, laser]);
    mouse_drag(&mut ed, MouseButton::Middle, &[laser, laser]);
    assert_eq!(ed.tool, Tool::Pen, "only the left button presses a button");
    ed.chrome.panel = Some(PanelState::default());
    let path = wave(&ed);
    mouse_drag(&mut ed, MouseButton::Right, &path);
    assert!(ed.chrome.panel.is_some(), "a right button does not even put the panel away");
}

#[test]
fn changing_the_mode_in_the_middle_of_a_stroke_ends_it_cleanly() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let path = wave(&ed);
    ed.mouse_moved(path[0]);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved(path[1]);
    ed.mouse_moved(path[2]);
    ed.set_mode(Mode::Tablet);
    assert_eq!(ed.ink.len(), 1, "the stroke was ended and kept");
    assert!(!ed.tools.busy());
    // The release that follows is for a gesture that no longer exists.
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(ed.ink.len(), 1);
}

/// Click empty page at (x, y), type `words`, press Esc.
fn type_with_mouse(ed: &mut Editor, words: &str) {
    let at = on_page(ed, 100.0, 300.0);
    mouse_click_at(ed, at);
    ed.type_text(words);
    ed.escape();
}

fn type_with_stylus(ed: &mut Editor, words: &str) {
    let at = on_page(ed, 100.0, 300.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[at]));
    ed.pointer_event(pen(Phase::Up, TIP, &[at]));
    ed.type_text(words);
    ed.escape();
}

fn texts(ed: &Editor) -> usize {
    ed.ink.strokes().iter().filter(|s| s.text.is_some()).count()
}

#[test]
fn text_is_placed_with_the_mouse_at_a_desk_and_with_the_stylus_as_a_tablet_and_never_the_other_way_round() {
    let mut desk = with(Tool::Text, Mode::Desktop);
    type_with_mouse(&mut desk, "desk");
    assert_eq!(texts(&desk), 1, "the desktop's mouse places text");
    let mut desk = with(Tool::Text, Mode::Desktop);
    type_with_stylus(&mut desk, "no");
    assert_eq!(texts(&desk), 0, "the desktop's stylus does nothing to the page with the text tool");

    let mut slate = with(Tool::Text, Mode::Tablet);
    type_with_stylus(&mut slate, "slate");
    assert_eq!(texts(&slate), 1, "the tablet's stylus places text");
    let mut slate = with(Tool::Text, Mode::Tablet);
    type_with_mouse(&mut slate, "no");
    assert_eq!(texts(&slate), 0, "the tablet's mouse only scrolls");
}

#[test]
fn a_tablets_stylus_selects_and_moves_text_the_way_a_desks_mouse_does() {
    let mut ed = with(Tool::Text, Mode::Tablet);
    type_with_stylus(&mut ed, "move me");
    let id = ed.ink.strokes().iter().find(|s| s.text.is_some()).map(|s| s.id).expect("a text box");
    let from = ed.ink.get(id).expect("it").pts[0];
    let (a, b) = (ed.view.doc_to_screen([from[0] + 4.0, from[1] + 4.0]), ed.view.doc_to_screen([from[0] + 104.0, from[1] + 84.0]));
    ed.pointer_event(pen(Phase::Down, TIP, &[a]));
    ed.pointer_event(pen(Phase::Move, TIP, &[b]));
    ed.pointer_event(pen(Phase::Up, TIP, &[b]));
    let to = ed.ink.get(id).expect("still there").pts[0];
    assert!((to[0] - from[0] - 100.0).abs() < 1.0 && (to[1] - from[1] - 80.0).abs() < 1.0, "moved by the drag: {from:?} to {to:?}");
}

#[test]
fn shortcuts_work_at_a_desk_only() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
    assert!(ed.shortcut("z", false));
    assert_eq!(ed.ink.len(), 0, "Ctrl+Z undoes");
    assert!(ed.shortcut("y", false));
    assert_eq!(ed.ink.len(), 1, "Ctrl+Y redoes");
    assert!(ed.shortcut("Z", false));
    assert_eq!(ed.ink.len(), 0, "the capital letter is Ctrl+Z too");
    assert!(ed.shortcut("z", true));
    assert_eq!(ed.ink.len(), 1, "Ctrl+Shift+Z redoes");
    assert!(ed.shortcut("s", false));
    assert!(ed.requests.has(&Request::Save), "Ctrl+S saves");
    assert!(!ed.shortcut("q", false), "other letters are not shortcuts");

    let mut slate = with(Tool::Pen, Mode::Tablet);
    add_stroke(&mut slate, &[[100.0, 300.0], [300.0, 300.0]]);
    assert!(!slate.shortcut("z", false));
    assert_eq!(slate.ink.len(), 1, "a tablet has no shortcuts");
    assert!(!slate.shortcut("s", false) && !slate.requests.has(&Request::Save));

    let mut asked = with(Tool::Pen, Mode::Desktop);
    asked.ask_unsaved();
    assert!(!asked.shortcut("s", false), "nothing but the prompt hears the keyboard while it is up");
}

#[test]
fn undo_by_shortcut_while_typing_takes_back_the_typing_first() {
    let mut ed = with(Tool::Text, Mode::Desktop);
    let at = on_page(&ed, 100.0, 300.0);
    mouse_click_at(&mut ed, at);
    ed.type_text("typed");
    assert!(ed.is_typing());
    assert!(ed.shortcut("z", false));
    assert!(!ed.is_typing(), "the typing was thrown away");
    assert_eq!(texts(&ed), 0);
}

fn mode_button(ed: &Editor) -> [f32; 2] {
    ed.toolbar().centre_of(Item::ModeSwitch)
}

#[test]
fn the_mode_button_switches_the_mode_with_every_device_and_shows_it() {
    let mut ed = editor();
    assert_eq!((ed.mode(), ed.toolbar_state().mode), (Mode::Desktop, Mode::Desktop));
    let button = mode_button(&ed);
    mouse_click_at(&mut ed, button);
    assert_eq!((ed.mode(), ed.toolbar_state().mode), (Mode::Tablet, Mode::Tablet), "the mouse");
    pen_stroke(&mut ed, TIP, &[button, button]);
    assert_eq!(ed.mode(), Mode::Desktop, "the stylus");
    ed.palm = crate::input::PalmGuard::default();
    ed.pointer_event(touch(2, Phase::Down, button, 30.0));
    ed.pointer_event(touch(2, Phase::Up, button, 30.0));
    assert_eq!(ed.mode(), Mode::Tablet, "a finger");
    assert_eq!(ed.ink.len(), 0, "and none of those presses drew anything");
    assert!(!ed.ink.is_dirty(), "a mode is not a change to the document");
}

#[test]
fn the_button_works_with_every_tool_and_not_while_the_prompt_is_up() {
    for tool in [Tool::Pen, Tool::Geometry, Tool::Text, LASER] {
        let mut ed = with(tool, Mode::Desktop);
        let button = mode_button(&ed);
        mouse_click_at(&mut ed, button);
        assert_eq!(ed.mode(), Mode::Tablet, "{tool:?}");
    }
    let mut asked = editor();
    asked.ask_unsaved();
    let button = mode_button(&asked);
    mouse_click_at(&mut asked, button);
    assert_eq!(asked.mode(), Mode::Desktop, "only the prompt hears the mouse while it is up");
}

#[test]
fn after_the_switch_the_devices_do_what_the_new_mode_says() {
    let mut ed = with(Tool::Pen, Mode::Desktop);
    let path = wave(&ed);
    mouse_drag(&mut ed, MouseButton::Left, &path);
    assert_eq!(ed.ink.len(), 1, "at a desk the mouse draws");
    let button = mode_button(&ed);
    mouse_click_at(&mut ed, button);
    mouse_drag(&mut ed, MouseButton::Left, &path);
    assert_eq!(ed.ink.len(), 1, "as a tablet it does not");
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.ink.len(), 2, "the stylus still does");
    assert!(!ed.shortcut("z", false), "and there are no shortcuts");
    mouse_click_at(&mut ed, button);
    mouse_drag(&mut ed, MouseButton::Left, &path);
    assert_eq!(ed.ink.len(), 3, "back at the desk, the mouse draws again");
    assert!(ed.shortcut("z", false) && ed.ink.len() == 2, "and Ctrl+Z works");
}

#[test]
fn switching_while_typing_keeps_the_text_and_places_it_with_the_stylus_afterwards() {
    let mut ed = with(Tool::Text, Mode::Desktop);
    let at = on_page(&ed, 100.0, 300.0);
    mouse_click_at(&mut ed, at);
    ed.type_text("kept");
    let button = mode_button(&ed);
    pen_stroke(&mut ed, TIP, &[button, button]);
    assert_eq!((ed.mode(), texts(&ed)), (Mode::Tablet, 1), "what was typed is a text box, not lost");
    assert!(!ed.is_typing());
    let elsewhere = on_page(&ed, 300.0, 500.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[elsewhere]));
    ed.pointer_event(pen(Phase::Up, TIP, &[elsewhere]));
    ed.type_text("also");
    ed.escape();
    assert_eq!(texts(&ed), 2, "the tablet's stylus places the next one");
}
