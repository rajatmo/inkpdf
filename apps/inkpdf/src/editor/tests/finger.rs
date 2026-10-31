//! The finger button of the right pill: a finger that draws as the stylus does, so that every tool works with no stylus. One finger on the page is the
//! tip of the stylus for the tool in use, in both modes; a second finger calls the stroke off and the two pinch and pan; the toolbars and cards are
//! pressed by a finger as before; with the button up the finger only pans and zooms.

use super::*;
use crate::input::Mode;
use crate::toolbar::Pill;

fn on(tool: Tool, mode: Mode) -> Editor {
    let mut ed = editor();
    ed.set_tool(tool);
    ed.chrome.panel = None;
    ed.set_mode(mode);
    ed.set_finger_draws(true);
    ed
}

/// A finger put down at the first point, moved through the rest and lifted at the last.
fn drag(ed: &mut Editor, pts: &[[f32; 2]]) {
    ed.pointer_event(touch(1, Phase::Down, pts[0], 30.0));
    for &p in &pts[1..] {
        ed.pointer_event(touch(1, Phase::Move, p, 30.0));
    }
    ed.pointer_event(touch(1, Phase::Up, pts[pts.len() - 1], 30.0));
}

fn wave(ed: &Editor) -> Vec<[f32; 2]> {
    vec![on_page(ed, 100.0, 300.0), on_page(ed, 160.0, 340.0), on_page(ed, 220.0, 300.0), on_page(ed, 280.0, 340.0)]
}

const MODES: [Mode; 2] = [Mode::Desktop, Mode::Tablet];

#[test]
fn the_button_is_on_the_right_pill_and_every_device_turns_the_finger_on_and_off_and_it_is_remembered() {
    for how in 0..3 {
        let mut ed = editor();
        assert_eq!(Item::Finger.pill(), Pill::View, "the pill on the right");
        assert!(!ed.finger_draws() && !ed.toolbar_state().finger, "it starts off: a finger pans");
        let press = |ed: &mut Editor| {
            let at = ed.toolbar().centre_of(Item::Finger);
            match how {
                0 => mouse_click_at(ed, at),
                1 => pen_tap(ed, at),
                _ => finger_tap(ed, at),
            }
        };
        press(&mut ed);
        assert!(ed.finger_draws() && ed.toolbar_state().finger, "device {how}: on");
        assert!(ed.requests.has(&Request::SettingsChanged), "device {how}: it is to be remembered");
        assert!(ed.settings().finger_draws);
        let mut next = editor();
        next.apply_settings(&ed.settings());
        assert!(next.finger_draws(), "device {how}: the next session starts as this one ended");
        press(&mut ed);
        assert!(!ed.finger_draws() && !ed.settings().finger_draws, "device {how}: off again");
        assert_eq!(ed.ink.len(), 0, "device {how}: pressing a button drew nothing");
    }
}

#[test]
fn with_it_on_one_finger_draws_with_the_pen_tool_in_both_modes_and_with_it_off_it_only_pans() {
    for mode in MODES {
        let mut ed = on(Tool::Pen, mode);
        let path = wave(&ed);
        drag(&mut ed, &path);
        assert_eq!(ed.ink.len(), 1, "{mode:?}: the finger draws");
        assert!(ed.ink.is_dirty() && !ed.finger_is_drawing(), "{mode:?}: and has let go");

        let mut off = on(Tool::Pen, mode);
        off.set_finger_draws(false);
        let before = off.view.scroll;
        let up = [on_page(&off, 300.0, 600.0), on_page(&off, 300.0, 450.0), on_page(&off, 300.0, 300.0)];
        drag(&mut off, &up);
        assert_eq!(off.ink.len(), 0, "{mode:?}: with the button up a finger draws nothing");
        settle(&mut off);
        assert!(off.view.scroll != before, "{mode:?}: it pans");
    }
}

#[test]
fn every_tool_works_with_the_finger_alone_in_both_modes() {
    for mode in MODES {
        // The pen: a stroke. The shape tool: a shape. The text tool: a box to type in. The lasso: a loop that chooses. The eraser: takes a stroke away.
        let mut ed = on(Tool::Geometry, mode);
        ed.styles.shape = crate::geometry::Shape::Rectangle;
        let corners = [on_page(&ed, 100.0, 200.0), on_page(&ed, 250.0, 320.0)];
        drag(&mut ed, &corners);
        assert_eq!(ed.ink.len(), 1, "{mode:?}: the shape tool makes a shape");
        assert!(ed.ink.strokes()[0].figure.is_some());

        let mut ed = on(Tool::Text, mode);
        let at = on_page(&ed, 120.0, 200.0);
        ed.pointer_event(touch(1, Phase::Down, at, 30.0));
        ed.pointer_event(touch(1, Phase::Up, at, 30.0));
        assert!(ed.text_edit().is_some(), "{mode:?}: a tap of the finger places a text box");

        let mut ed = on(Tool::Lasso, mode);
        let id = add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
        let ring: Vec<[f32; 2]> =
            [[50.0, 250.0], [350.0, 250.0], [350.0, 350.0], [50.0, 350.0], [50.0, 250.0]].iter().map(|p| on_page(&ed, p[0], p[1])).collect();
        drag(&mut ed, &ring);
        assert_eq!(ed.selection, vec![id], "{mode:?}: a loop of the finger chooses the stroke");

        let mut ed = on(Tool::Eraser, mode);
        add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
        let across = [on_page(&ed, 90.0, 300.0), on_page(&ed, 310.0, 300.0)];
        drag(&mut ed, &across);
        assert_eq!(ed.ink.len(), 0, "{mode:?}: the finger rubs the stroke out");

        let mut ed = on(LASER, mode);
        let path = wave(&ed);
        drag(&mut ed, &path);
        assert_eq!(ed.ink.len(), 0, "{mode:?}: the laser draws nothing");
        assert!(!laser_trail(&ed, 1000.0).is_empty() || laser_fading(&ed), "{mode:?}: it leaves its trail");
    }
}

#[test]
fn two_fingers_put_down_together_are_a_pinch_that_draws_nothing_and_a_second_after_a_stroke_began_ends_it() {
    for mode in MODES {
        let mut ed = on(Tool::Pen, mode);
        let (a, b) = (on_page(&ed, 300.0, 300.0), on_page(&ed, 360.0, 300.0));
        ed.pointer_event(touch(1, Phase::Down, a, 30.0));
        assert!(ed.finger_is_drawing(), "{mode:?}: the first finger is the stylus");
        ed.pointer_event(touch(1, Phase::Move, [a[0] + 2.0, a[1] + 1.0], 30.0));
        ed.pointer_event(touch(2, Phase::Down, b, 30.0));
        assert!(!ed.finger_is_drawing(), "{mode:?}: the second calls it off");
        assert!(ed.pinching(), "{mode:?}: and the two are a pinch");
        let zoom = ed.view.zoom;
        for step in 1..=6 {
            let d = step as f32 * 12.0;
            ed.pointer_event(touch(1, Phase::Move, [a[0] - d, a[1]], 30.0));
            ed.pointer_event(touch(2, Phase::Move, [b[0] + d, b[1]], 30.0));
        }
        assert!(ed.view.zoom > zoom, "{mode:?}: fingers moving apart zoom in");
        ed.pointer_event(touch(1, Phase::Up, a, 30.0));
        ed.pointer_event(touch(2, Phase::Up, b, 30.0));
        assert_eq!(ed.ink.len(), 0, "{mode:?}: nothing was drawn, and nothing is left");
        assert!(!ed.ink.is_dirty(), "{mode:?}: and the document is not marked changed");
        // A finger put down alone again draws.
        let path = wave(&ed);
        drag(&mut ed, &path);
        assert_eq!(ed.ink.len(), 1, "{mode:?}: and then one finger draws again");

        // A stroke that was begun is ended by a second finger (it is what was drawn: undo takes it back), and the two then pinch.
        let mut ed = on(Tool::Pen, mode);
        let (a, b) = (on_page(&ed, 300.0, 300.0), on_page(&ed, 360.0, 300.0));
        ed.pointer_event(touch(1, Phase::Down, a, 30.0));
        ed.pointer_event(touch(1, Phase::Move, [a[0] + 40.0, a[1] + 10.0], 30.0));
        ed.pointer_event(touch(2, Phase::Down, b, 30.0));
        assert!(!ed.finger_is_drawing() && ed.pinching(), "{mode:?}: ended, and pinching");
        ed.pointer_event(touch(1, Phase::Up, a, 30.0));
        ed.pointer_event(touch(2, Phase::Up, b, 30.0));
        assert_eq!(ed.ink.len(), 1, "{mode:?}: the stroke is kept");
    }
}
#[test]
fn the_toolbars_and_cards_are_pressed_by_the_finger_as_before_and_a_palm_or_a_stylus_near_is_not_a_stylus() {
    let mut ed = on(Tool::Pen, Mode::Tablet);
    // A button of the toolbar: pressed, and nothing is drawn under it.
    let at = ed.toolbar().centre_of(Item::Tool(Tool::Lasso));
    finger_tap(&mut ed, at);
    assert_eq!(ed.tool, Tool::Lasso, "the finger presses a tool's button");
    ed.set_tool(Tool::Pen);
    // A palm (a big contact) is dropped.
    let p = on_page(&ed, 200.0, 300.0);
    ed.pointer_event(touch(1, Phase::Down, p, 400.0));
    ed.pointer_event(touch(1, Phase::Move, [p[0] + 50.0, p[1]], 400.0));
    ed.pointer_event(touch(1, Phase::Up, [p[0] + 50.0, p[1]], 400.0));
    assert_eq!(ed.ink.len(), 0, "a palm draws nothing");
    // A finger put down while the stylus is near is the hand resting.
    ed.pointer_event(pen(Phase::Hover, HOVER, &[on_page(&ed, 400.0, 400.0)]));
    let path = wave(&ed);
    drag(&mut ed, &path);
    assert_eq!(ed.ink.len(), 0, "a hand beside a stylus draws nothing");
    // And the real stylus draws as it did.
    ed.pointer_event(pen(Phase::Leave, HOVER, &[]));
    let path = wave(&ed);
    pen_stroke(&mut ed, TIP, &path);
    assert_eq!(ed.ink.len(), 1, "the stylus still draws");
}

#[test]
fn turning_it_off_in_the_middle_of_a_stroke_ends_the_stroke_and_the_overview_and_prompts_are_not_drawn_on() {
    let mut ed = on(Tool::Pen, Mode::Tablet);
    let path = wave(&ed);
    ed.pointer_event(touch(1, Phase::Down, path[0], 30.0));
    ed.pointer_event(touch(1, Phase::Move, path[1], 30.0));
    assert!(ed.finger_is_drawing());
    ed.set_finger_draws(false);
    assert!(!ed.finger_is_drawing(), "nothing is left half done");
    ed.pointer_event(touch(1, Phase::Up, path[1], 30.0));
    assert!(ed.is_idle(), "nothing under way");
    // The overview takes the finger as it always did: it scrolls the pages and draws nothing.
    ed.set_finger_draws(true);
    ed.open_overview();
    let (a, b) = (on_page(&ed, 300.0, 400.0), on_page(&ed, 300.0, 200.0));
    drag(&mut ed, &[a, b]);
    assert_eq!(ed.ink.len(), 1, "the overview is not drawn on (the one stroke is the one that was ended)");
}

#[test]
fn the_select_tool_with_the_finger_pans_on_empty_page_and_follows_a_link_as_the_stylus_does() {
    for mode in MODES {
        let mut ed = on(Tool::Select, mode);
        let before = ed.view.scroll;
        let (a, b) = (on_page(&ed, 300.0, 600.0), on_page(&ed, 300.0, 300.0));
        drag(&mut ed, &[a, [a[0], (a[1] + b[1]) / 2.0], b]);
        assert_eq!(ed.ink.len(), 0, "{mode:?}: the reader draws nothing");
        assert!(ed.view.scroll != before, "{mode:?}: a drag on empty page moves it, as the stylus's does");

        // A tap on a link goes where it goes.
        let mut ed = on(Tool::Select, mode);
        ed.links = vec![crate::links::Link { rect: [112.0, 302.0, 312.0, 322.0], target: crate::links::Target::Page(1) }];
        let at = ed.view.doc_to_screen([212.0, 312.0]);
        ed.pointer_event(touch(1, Phase::Down, at, 30.0));
        ed.pointer_event(touch(1, Phase::Up, at, 30.0));
        settle(&mut ed);
        assert_eq!(ed.view.current_page(&ed.layout), 1, "{mode:?}: a tap of the finger follows the link");
    }
}
