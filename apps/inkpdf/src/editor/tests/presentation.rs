//! Showing the document as slides: one page at a time, whole and centred, over black; every device turns the pages and leaves, and nothing else of the program
//! is reachable while it is shown.

use super::*;
use crate::input::{Dir, Mode};
use crate::tools::Key;

/// Four pages, in a 1200 x 900 window.
fn deck() -> Editor {
    let mut ed = Editor::reference([1200.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
    ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0], [792.0, 612.0], [612.0, 792.0]]), vec![]);
    ed
}

fn presenting_at(page: usize) -> Editor {
    let mut ed = deck();
    ed.ws.scroll_to_page(page);
    settle(&mut ed);
    ed.shortcut("l", false);
    ed
}

fn shown(ed: &Editor) -> usize {
    ed.presentation_view().expect("the slides").page
}

/// A tap of each device at `at`: 0 the mouse, 1 the stylus's tip, 2 a finger.
fn tap_with(ed: &mut Editor, how: usize, at: [f32; 2]) {
    match how {
        0 => mouse_click_at(ed, at),
        1 => pen_tap(ed, at),
        _ => finger_tap(ed, at),
    }
}

#[test]
fn the_slides_begin_at_the_page_in_view_with_ctrl_l_or_the_button_and_make_the_window_full_screen_and_put_it_back() {
    let mut ed = presenting_at(2);
    assert!(ed.presenting() && shown(&ed) == 2, "Ctrl+L: the page that was in view");
    assert!(ed.requests.take(&Request::Fullscreen), "the window is made full screen");
    ed.chrome.fullscreen = true;
    ed.press(Key::Escape);
    assert!(!ed.presenting());
    assert!(ed.requests.take(&Request::Fullscreen), "and given back");

    // The button of the right pill, with every device, in either mode.
    for mode in [Mode::Desktop, Mode::Tablet] {
        for how in 0..3 {
            let mut ed = deck();
            ed.set_mode(mode);
            let at = ed.toolbar().centre_of(Item::Present);
            tap_with(&mut ed, how, at);
            assert!(ed.presenting(), "{mode:?}, device {how}");
        }
    }
    // (A window that is full screen already is left so.)
    let mut ed = deck();
    ed.chrome.fullscreen = true;
    ed.shortcut("l", false);
    assert!(ed.presenting() && !ed.requests.has(&Request::Fullscreen));
    ed.press(Key::Escape);
    assert!(!ed.requests.has(&Request::Fullscreen), "and not toggled when it ends");
}

#[test]
fn one_page_is_shown_whole_and_centred_in_every_shape_of_window_and_the_curtain_hides_the_pages_beside_it() {
    for (viewport, dpr) in [([1200.0, 900.0], 1.0), ([900.0, 1600.0], 2.0), ([2400.0, 1080.0], 2.6), ([360.0, 480.0], 1.0)] {
        for start in 0..4 {
            let mut ed = Editor::reference(viewport, dpr, crate::motion::Prefs::REDUCED);
            ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0], [792.0, 612.0], [612.0, 792.0]]), vec![]);
            ed.ws.scroll_to_page(start);
            settle(&mut ed);
            ed.shortcut("l", false);
            let v = ed.presentation_view().expect("slides");
            let r = v.page_rect.map(f32::from_bits);
            // (The page in view is the one at the middle of the window: in a tall window that may be the next.)
            assert!(v.page == start || v.page == start + 1, "{viewport:?}");
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[2] <= viewport[0] + 0.5 && r[3] <= viewport[1] + 0.5,
                "{viewport:?}: the page {r:?} is whole in the window"
            );
            let (w, h) = (r[2] - r[0], r[3] - r[1]);
            let page = ed.layout.page(v.page).expect("page");
            assert!((w / h - page.w / page.h).abs() < 0.01, "its shape is kept");
            assert!(((r[0] + r[2]) / 2.0 - viewport[0] / 2.0).abs() < 1.0 && ((r[1] + r[3]) / 2.0 - viewport[1] / 2.0).abs() < 1.0, "centred: {r:?}");
            assert!((w - viewport[0]).abs() < 1.0 || (h - viewport[1]).abs() < 1.0, "as big as it can be");
        }
    }
    // A window that changes shape has the page fitted again.
    let mut ed = presenting_at(1);
    ed.set_viewport([700.0, 1300.0]);
    let r = ed.presentation_view().expect("slides").page_rect.map(f32::from_bits);
    assert!((r[2] - r[0] - 700.0).abs() < 1.0 && ((r[1] + r[3]) / 2.0 - 650.0).abs() < 1.0, "{r:?}");
}

#[test]
fn every_device_turns_the_pages_by_the_right_and_the_left_of_the_window_and_the_first_and_last_stay() {
    for how in 0..3 {
        let mut ed = presenting_at(0);
        let (right, left) = ([900.0, 450.0], [100.0, 450.0]);
        tap_with(&mut ed, how, left);
        assert_eq!(shown(&ed), 0, "device {how}: nothing before the first");
        for want in [1, 2, 3, 3] {
            tap_with(&mut ed, how, right);
            assert_eq!(shown(&ed), want, "device {how}: on");
        }
        for want in [2, 1, 0] {
            tap_with(&mut ed, how, left);
            assert_eq!(shown(&ed), want, "device {how}: back");
        }
    }
}

#[test]
fn a_finger_swipes_between_the_pages_and_a_drag_of_the_mouse_or_the_tip_turns_nothing() {
    let mut ed = presenting_at(1);
    let swipe = |ed: &mut Editor, from: [f32; 2], to: [f32; 2]| {
        ed.pointer_event(touch(1, Phase::Down, from, 30.0));
        ed.pointer_event(touch(1, Phase::Move, [(from[0] + to[0]) / 2.0, from[1]], 30.0));
        ed.pointer_event(touch(1, Phase::Up, to, 30.0));
    };
    swipe(&mut ed, [800.0, 450.0], [300.0, 450.0]);
    assert_eq!(shown(&ed), 2, "to the left: the next");
    swipe(&mut ed, [300.0, 450.0], [800.0, 470.0]);
    assert_eq!(shown(&ed), 1, "to the right: the one before");
    swipe(&mut ed, [600.0, 200.0], [620.0, 700.0]);
    assert_eq!(shown(&ed), 1, "a drag down is no swipe");
    mouse_drag(&mut ed, &[[900.0, 450.0], [700.0, 450.0], [300.0, 450.0]]);
    pen_stroke(&mut ed, TIP, &[[900.0, 450.0], [700.0, 450.0], [300.0, 450.0]]);
    assert_eq!(shown(&ed), 1, "the mouse and the tip turn a page by a tap, not by a drag");
}

#[test]
fn the_keys_and_the_wheel_turn_the_pages() {
    let mut ed = presenting_at(1);
    let forward = [Key::Page(true), Key::Arrow(Dir::Right, false), Key::Arrow(Dir::Down, false), Key::Text("\n"), Key::Text(" ")];
    let back = [Key::Page(false), Key::Arrow(Dir::Left, false), Key::Arrow(Dir::Up, false), Key::Backspace];
    for key in forward {
        ed.press(Key::Home(false));
        assert!(ed.press(key), "{key:?}");
        assert_eq!(shown(&ed), 1, "{key:?}");
    }
    for key in back {
        ed.press(Key::End(false));
        assert!(ed.press(key), "{key:?}");
        assert_eq!(shown(&ed), 2, "{key:?}");
    }
    ed.press(Key::Home(false));
    assert_eq!(shown(&ed), 0);
    ed.press(Key::End(false));
    assert_eq!(shown(&ed), 3);
    assert!(!ed.press(Key::Delete) && !ed.press(Key::Text("x")), "nothing else is a key of the slides");
    ed.wheel(Wheel::Lines(0.0, -1.0));
    assert_eq!(shown(&ed), 2, "a notch up: the page before");
    ed.wheel(Wheel::Lines(0.0, 1.0));
    ed.wheel(Wheel::Lines(0.0, 1.0));
    assert_eq!(shown(&ed), 3, "a notch down: on, and the last stays");
    ed.press(Key::Home(false));
    for _ in 0..4 {
        ed.wheel(Wheel::Pixels(0.0, 60.0));
    }
    assert_eq!(shown(&ed), 2, "a touchpad: a page for each hundred pixels (240 here)");
}

#[test]
fn escape_ctrl_l_and_the_button_that_shows_for_a_moment_leave_with_every_device_and_leave_the_document_at_the_page_shown() {
    let mut ed = presenting_at(0);
    ed.press(Key::End(false));
    ed.press(Key::Escape);
    settle(&mut ed);
    assert!(!ed.presenting());
    assert_eq!(ed.view.current_page(&ed.layout), 3, "the document is at the page that was shown last");

    let mut ed = presenting_at(1);
    ed.shortcut("l", false);
    assert!(!ed.presenting(), "Ctrl+L again");

    for how in 0..3 {
        let mut ed = presenting_at(1);
        let layout = ed.presentation_layout().expect("layout");
        // The button is there for the first moment, and goes.
        assert!(ed.presentation_view().expect("slides").hint);
        for _ in 0..200 {
            ed.step_view(0.016);
        }
        assert!(!ed.presentation_view().expect("slides").hint, "device {how}: it has gone");
        if how == 0 {
            // (A mouse shows it by going to it: it moves there, and the click leaves.)
            tap_with(&mut ed, how, layout.exit_centre());
            assert!(!ed.presenting(), "the mouse that has moved to the button presses it");
            continue;
        }
        // A tap where it was, now that it is not showing, is a tap on the page: on.
        tap_with(&mut ed, how, layout.exit_centre());
        assert!(ed.presenting() && shown(&ed) == 2, "device {how}: not showing, it is not pressed");
        assert!(ed.presentation_view().expect("slides").hint, "device {how}: a tap shows it");
        tap_with(&mut ed, how, layout.exit_centre());
        assert!(!ed.presenting(), "device {how}: showing, it leaves");
    }
    // A moving mouse shows it too.
    let mut ed = presenting_at(0);
    for _ in 0..200 {
        ed.step_view(0.016);
    }
    ed.mouse_moved([500.0, 500.0]);
    assert!(ed.presentation_view().expect("slides").hint);
}

#[test]
fn nothing_but_the_slides_is_reachable_while_they_are_shown() {
    for mode in [Mode::Desktop, Mode::Tablet] {
        let mut ed = deck();
        ed.set_mode(mode);
        ed.set_tool(Tool::Pen);
        ed.chrome.panel = None;
        ed.shortcut("l", false);
        let tablet_start = !ed.presenting();
        if tablet_start {
            let at = ed.toolbar().centre_of(Item::Present);
            pen_tap(&mut ed, at);
        }
        assert!(ed.presenting());
        // The pen, the mouse and a finger draw nothing, whatever the tool.
        let path = [on_page(&ed, 100.0, 300.0), on_page(&ed, 160.0, 340.0), on_page(&ed, 220.0, 300.0)];
        pen_stroke(&mut ed, TIP, &path);
        mouse_drag(&mut ed, &path);
        assert_eq!(ed.ink.len(), 0, "{mode:?}");
        // The toolbar is not there to press: a press where its undo button was turns a page.
        let page = shown(&ed);
        let undo = [40.0, 40.0];
        mouse_click_at(&mut ed, undo);
        assert_eq!(shown(&ed), page, "the click at the toolbar's corner is on the left: the first stays");
        assert!(ed.presenting() && !ed.requests.has(&Request::Save) && !ed.requests.has(&Request::Close));
        // No shortcut of the page fires.
        assert!(!ed.shortcut("z", false) && !ed.shortcut("s", false) && !ed.shortcut("f", false));
        assert!(!ed.requests.has(&Request::Save) && !ed.search_open());
        // The interface drawn is the slides' own: black and, for a moment, the counter.
        let mut ui = crate::toolbar::Ui::default();
        ed.draw_interface(&mut ui);
        assert!(ui.images.is_empty() && ui.panels.iter().all(|p| p.rect[0] >= 0.0), "{mode:?}");
    }
}

#[test]
fn a_document_opened_while_they_are_shown_ends_them_and_the_overview_and_a_prompt_cannot_begin_them() {
    let mut ed = presenting_at(0);
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(!ed.presenting());
    let mut ed = deck();
    ed.open_overview();
    ed.start_presentation();
    // (The overview was put away for them.)
    assert!(ed.presenting() && !ed.overview_open());
    let mut ed = deck();
    ed.chrome.prompt = Some(crate::toolbar::PromptView::default());
    ed.start_presentation();
    assert!(!ed.presenting(), "not with a question to answer");
}
