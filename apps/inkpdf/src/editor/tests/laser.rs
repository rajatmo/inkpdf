use super::*;

/// An editor with the laser chosen.
fn lasering() -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Laser);
    ed
}

fn path(ed: &Editor) -> Vec<[f32; 2]> {
    (0..10).map(|i| on_page(ed, 100.0 + i as f32 * 20.0, 300.0 + i as f32 * 5.0)).collect()
}

#[test]
fn the_mouse_is_timed_by_the_clock_the_editor_was_given() {
    // A mouse standing in for the stylus has no timestamps of its own: it takes the time from the editor's clock.
    let mut ed = lasering();
    ed.set_clock(|| 5000.0);
    let p = path(&ed);
    ed.mouse_moved(p[0]);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    for &at in &p[1..] {
        ed.mouse_moved(at);
    }
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert!(ed.laser().pieces(5000.0 + 10.0).count() >= 9, "the trail was stamped 5000");
    assert_eq!(ed.laser().pieces(5000.0 + crate::laser::FADE_MS + 1.0).count(), 0, "and fades as 5000 ages");
}

#[test]
fn the_pen_with_the_laser_leaves_a_trail_and_draws_nothing() {
    let mut ed = lasering();
    let p = path(&ed);
    pen_stroke(&mut ed, TIP, &p);
    assert_eq!(ed.ink.len(), 0, "a laser draws no ink");
    assert!(!ed.ink.is_dirty(), "and changes nothing to save");
    assert!(ed.laser().pieces(1000.0).count() >= 9, "a piece between each pair of points");
    assert!(ed.is_idle(), "the stroke is over");
}

#[test]
fn the_trail_is_where_the_pen_went_in_the_document() {
    let mut ed = lasering();
    let p = path(&ed);
    pen_stroke(&mut ed, TIP, &p);
    let first = ed.laser().pieces(1000.0).next().expect("a piece");
    let at = ed.view.doc_to_screen(first.0);
    assert!((at[0] - p[0][0]).abs() < 0.01 && (at[1] - p[0][1]).abs() < 0.01, "{at:?} vs {:?}", p[0]);
    // Scrolling the page moves the trail with it.
    ed.pan_px(0.0, -100.0);
    let moved = ed.view.doc_to_screen(first.0);
    assert!((moved[1] - (p[0][1] - 100.0)).abs() < 0.01);
}

#[test]
fn the_eraser_end_and_the_barrel_button_only_point_too() {
    let mut ed = lasering();
    let id = add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 330.0]]);
    let across = [on_page(&ed, 200.0, 250.0), on_page(&ed, 200.0, 315.0), on_page(&ed, 200.0, 380.0)];
    pen_stroke(&mut ed, ERASER, &across);
    assert!(ed.ink.get(id).is_some(), "the eraser end did not erase");
    let around =
        [on_page(&ed, 60.0, 260.0), on_page(&ed, 340.0, 260.0), on_page(&ed, 340.0, 380.0), on_page(&ed, 60.0, 380.0), on_page(&ed, 60.0, 260.0)];
    pen_stroke(&mut ed, BARREL, &around);
    assert!(ed.selection.is_empty(), "the barrel button did not lasso");
    assert!(ed.laser().pieces(1000.0).count() > 0);
}

#[test]
fn a_hovering_pen_is_followed_by_a_dot_that_goes_when_the_pen_does() {
    let mut ed = lasering();
    let at = on_page(&ed, 200.0, 300.0);
    ed.pointer_event(pen(Phase::Hover, HOVER, &[at]));
    let head = ed.laser().head().expect("a dot");
    let screen = ed.view.doc_to_screen(head);
    assert!((screen[0] - at[0]).abs() < 0.01 && (screen[1] - at[1]).abs() < 0.01);
    assert!(!ed.laser().is_fading(), "hovering leaves no trail");
    ed.pointer_event(pen(Phase::Leave, HOVER, &[at]));
    assert_eq!(ed.laser().head(), None);
}

#[test]
fn no_dot_over_the_toolbar_or_without_the_laser() {
    let mut ed = lasering();
    let bar = ed.toolbar().centre_of(Item::Undo);
    ed.pointer_event(pen(Phase::Hover, HOVER, &[bar]));
    assert_eq!(ed.laser().head(), None, "the toolbar is not part of the page");
    let mut ed = editor();
    ed.pointer_event(pen(Phase::Hover, HOVER, &[on_page(&ed, 200.0, 300.0)]));
    assert_eq!(ed.laser().head(), None, "the pen tool shows no dot");
}

#[test]
fn the_mouse_does_not_point_or_select_with_the_laser_in_the_tablet_mode() {
    let mut ed = lasering();
    ed.set_mode(Mode::Tablet);
    let id = add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
    ed.mouse_moved(on_page(&ed, 100.0, 300.0));
    assert_eq!(ed.laser().head(), None, "no dot follows the mouse");
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    for i in 1..8 {
        ed.mouse_moved(on_page(&ed, 100.0 + i as f32 * 25.0, 300.0));
    }
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert!(ed.selection.is_empty() && !ed.laser().is_fading(), "no selection and no trail");
    assert!(ed.ink.get(id).is_some() && ed.is_idle());
}

#[test]
fn a_finger_still_only_pans_and_leaves_no_trail() {
    let mut ed = lasering();
    let before = ed.view.scroll;
    let (a, b) = (on_page(&ed, 300.0, 600.0), on_page(&ed, 300.0, 300.0));
    ed.pointer_event(touch(1, Phase::Down, a, 30.0));
    ed.pointer_event(touch(1, Phase::Move, b, 30.0));
    ed.pointer_event(touch(1, Phase::Up, b, 30.0));
    assert_ne!(ed.view.scroll, before, "the page panned");
    assert!(!ed.laser().is_fading() && ed.laser().head().is_none());
}

#[test]
fn choosing_a_tool_or_loading_a_document_clears_the_laser() {
    let mut ed = lasering();
    let p = path(&ed);
    pen_stroke(&mut ed, TIP, &p);
    assert!(ed.laser().is_fading());
    ed.set_tool(Tool::Pen);
    assert!(!ed.laser().is_fading(), "the trail went with the tool");
    ed.set_tool(Tool::Laser);
    pen_stroke(&mut ed, TIP, &p);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.laser().is_fading() && ed.laser().head().is_none());
    ed.pointer_event(pen(Phase::Hover, HOVER, &[[300.0, 300.0]]));
    ed.ask_unsaved();
    assert_eq!(ed.laser().head(), None, "the prompt hides the dot");
}

#[test]
fn pressing_the_toolbar_with_the_laser_chosen_leaves_no_trail() {
    let mut ed = lasering();
    let undo = ed.toolbar().centre_of(Item::Undo);
    pen_stroke(&mut ed, TIP, &[undo, [undo[0] + 5.0, undo[1]]]);
    assert!(!ed.laser().is_fading());
}

#[test]
fn the_pen_draws_again_after_the_laser() {
    let mut ed = lasering();
    let pen_at = ed.toolbar().centre_of(Item::Tool(Tool::Pen));
    pen_stroke(&mut ed, TIP, &[pen_at, pen_at]);
    assert_eq!(ed.tool, Tool::Pen);
    ed.chrome.panel = None;
    let p = path(&ed);
    pen_stroke(&mut ed, TIP, &p);
    assert_eq!(ed.ink.len(), 1);
}
