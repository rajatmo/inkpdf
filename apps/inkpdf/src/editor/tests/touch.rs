use super::*;

#[test]
fn one_finger_pans_and_never_draws() {
    let mut ed = editor();
    let before = ed.view.scroll;
    ed.pointer_event(touch(1, Phase::Down, [600.0, 500.0], 30.0));
    ed.pointer_event(touch(1, Phase::Move, [600.0, 450.0], 30.0));
    ed.pointer_event(touch(1, Phase::Move, [600.0, 400.0], 30.0));
    assert!((ed.view.scroll[1] - before[1] - 100.0).abs() < 0.5, "dragging up 100px scrolls down 100 points");
    ed.pointer_event(touch(1, Phase::Up, [600.0, 400.0], 30.0));
    assert_eq!(ed.ink.len(), 0);
    assert!(ed.is_idle());
}

#[test]
fn two_fingers_zoom() {
    let mut ed = editor();
    let z0 = ed.view.zoom;
    ed.pointer_event(touch(1, Phase::Down, [500.0, 450.0], 30.0));
    ed.pointer_event(touch(2, Phase::Down, [700.0, 450.0], 30.0));
    ed.pointer_event(touch(2, Phase::Move, [900.0, 450.0], 30.0));
    assert!((ed.view.zoom / z0 - 2.0).abs() < 0.05, "fingers twice as far apart: {}", ed.view.zoom / z0);
    assert!(!ed.fit_mode);
    ed.pointer_event(touch(2, Phase::Up, [900.0, 450.0], 30.0));
    ed.pointer_event(touch(1, Phase::Up, [500.0, 450.0], 30.0));
}

#[test]
fn a_finger_never_selects_or_moves_ink() {
    let mut ed = editor();
    let id = add_stroke(&mut ed, &[[200.0, 300.0], [400.0, 300.0]]);
    let on_stroke = on_page(&ed, 300.0, 300.0);
    // A tap on a stroke selects nothing.
    ed.pointer_event(touch(1, Phase::Down, on_stroke, 30.0));
    ed.pointer_event(touch(1, Phase::Up, on_stroke, 30.0));
    assert!(ed.selection.is_empty());
    // A drag across it pans the page and leaves the stroke where it is.
    let before = ed.ink.get(id).expect("stroke").pts.clone();
    ed.pointer_event(touch(2, Phase::Down, on_stroke, 30.0));
    assert!(ed.is_idle(), "a finger starts no interaction");
    ed.pointer_event(touch(2, Phase::Move, [on_stroke[0] + 60.0, on_stroke[1] + 40.0], 30.0));
    ed.pointer_event(touch(2, Phase::Up, [on_stroke[0] + 60.0, on_stroke[1] + 40.0], 30.0));
    assert!(ed.selection.is_empty());
    assert_eq!(ed.ink.get(id).expect("stroke").pts, before);
    assert_eq!(ed.ink.len(), 1, "and draws nothing");
}

#[test]
fn a_tap_on_the_page_lets_go_of_a_selection() {
    let mut ed = editor();
    let id = add_stroke(&mut ed, &[[200.0, 300.0], [400.0, 300.0]]);
    ed.selection = vec![id];
    let elsewhere = on_page(&ed, 300.0, 600.0);
    ed.pointer_event(touch(1, Phase::Down, elsewhere, 30.0));
    ed.pointer_event(touch(1, Phase::Up, elsewhere, 30.0));
    assert!(ed.selection.is_empty());
}
#[test]
fn a_resting_palm_is_ignored_while_the_pen_is_near() {
    let mut ed = editor();
    let before = ed.view.scroll;
    ed.pointer_event(pen(Phase::Hover, HOVER, &[on_page(&ed, 300.0, 300.0)]));
    ed.pointer_event(touch(1, Phase::Down, [600.0, 500.0], 30.0));
    ed.pointer_event(touch(1, Phase::Move, [600.0, 300.0], 30.0));
    ed.pointer_event(touch(1, Phase::Up, [600.0, 300.0], 30.0));
    assert_eq!(ed.view.scroll, before, "touch did nothing with the pen hovering");
}

#[test]
fn a_big_contact_is_ignored_as_a_palm() {
    let mut ed = editor();
    let before = ed.view.scroll;
    ed.pointer_event(touch(1, Phase::Down, [600.0, 500.0], 300.0));
    ed.pointer_event(touch(1, Phase::Move, [600.0, 300.0], 300.0));
    assert_eq!(ed.view.scroll, before);
}
