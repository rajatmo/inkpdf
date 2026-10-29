//! `Editor::handle` is the one way in: each kind of `InputEvent` does what the method it stands for does, and a whole session
//! handed over as events (as a replay does) leaves the editor as the same session driven by the methods.

use super::*;
use crate::input::{InputEvent, KeyInput};

fn desktop(tool: Tool) -> Editor {
    let mut ed = editor();
    ed.set_tool(tool);
    ed.chrome.panel = None;
    ed.set_mode(Mode::Desktop);
    ed
}

#[test]
fn a_pen_event_and_the_mouse_each_draw_when_handed_over_as_events() {
    let mut ed = desktop(Tool::Pen);
    let pts = [on_page(&ed, 100.0, 300.0), on_page(&ed, 160.0, 340.0), on_page(&ed, 220.0, 300.0)];
    for (phase, at) in [(Phase::Down, &pts[..1]), (Phase::Move, &pts[1..]), (Phase::Up, &pts[2..])] {
        ed.handle(InputEvent::Pointer(pen(phase, TIP, at)));
    }
    assert_eq!(ed.ink.len(), 1, "the pen");
    ed.handle(InputEvent::MouseMoved(on_page(&ed, 100.0, 500.0)));
    ed.handle(InputEvent::MouseButton { state: ButtonState::Pressed, button: MouseButton::Left });
    ed.handle(InputEvent::MouseMoved(on_page(&ed, 200.0, 520.0)));
    ed.handle(InputEvent::MouseButton { state: ButtonState::Released, button: MouseButton::Left });
    assert_eq!(ed.ink.len(), 2, "the left button, as a stylus at a desk");
    ed.handle(InputEvent::MouseButton { state: ButtonState::Pressed, button: MouseButton::Other });
    ed.handle(InputEvent::MouseButton { state: ButtonState::Released, button: MouseButton::Other });
    assert_eq!(ed.ink.len(), 2, "another button does nothing");
}

#[test]
fn the_wheel_a_touch_that_is_a_place_and_the_window_changes_each_reach_the_view() {
    let mut ed = desktop(Tool::Pen);
    let top = ed.view.scroll;
    ed.handle(InputEvent::Wheel(Wheel::Lines(0.0, -3.0)));
    settle(&mut ed);
    assert!(ed.view.scroll[1] > top[1], "a notch of the wheel scrolls");
    let before = ed.view.scroll;
    ed.handle(InputEvent::Touch { phase: Phase::Down, id: 1, pos: [300.0, 400.0] });
    ed.handle(InputEvent::Touch { phase: Phase::Move, id: 1, pos: [300.0, 350.0] });
    ed.handle(InputEvent::Touch { phase: Phase::Up, id: 1, pos: [300.0, 350.0] });
    assert_ne!(ed.view.scroll, before, "a drag of a touch that is only a place pans");
    ed.handle(InputEvent::Resized([800.0, 600.0]));
    assert_eq!(ed.view.viewport, [800.0, 600.0]);
    ed.handle(InputEvent::ScaleChanged(2.0));
    assert_eq!(ed.dpr, 2.0);
}

#[test]
fn keys_and_shortcuts_reach_the_text_box_and_the_history() {
    let mut ed = desktop(Tool::Text);
    let at = on_page(&ed, 100.0, 300.0);
    mouse_click_at(&mut ed, at);
    assert!(ed.is_typing());
    ed.handle(InputEvent::Key(KeyInput::Text("hi".to_string())));
    ed.handle(InputEvent::Key(KeyInput::Backspace));
    assert_eq!(ed.text_edit().map(|e| e.text.clone()), Some("h".to_string()));
    ed.handle(InputEvent::Key(KeyInput::Escape));
    assert_eq!(ed.ink.len(), 1, "the text is a box now");
    ed.handle(InputEvent::Shortcut { letter: "z".to_string(), shift: false });
    assert_eq!(ed.ink.len(), 0, "Ctrl+Z undoes it");
    ed.handle(InputEvent::Shortcut { letter: "S".to_string(), shift: true });
    assert!(ed.requests.take(&Request::SaveAs), "Ctrl+Shift+S, whatever the case of the letter");
}

#[test]
fn losing_the_focus_ends_what_a_held_button_was_doing() {
    let mut ed = desktop(Tool::Pen);
    ed.handle(InputEvent::MouseMoved(on_page(&ed, 100.0, 300.0)));
    ed.handle(InputEvent::MouseButton { state: ButtonState::Pressed, button: MouseButton::Left });
    ed.handle(InputEvent::MouseMoved(on_page(&ed, 160.0, 340.0)));
    ed.handle(InputEvent::FocusLost);
    ed.handle(InputEvent::MouseButton { state: ButtonState::Released, button: MouseButton::Left });
    assert!(ed.is_idle(), "nothing is left half done");
    ed.handle(InputEvent::MouseLeft);
}

#[test]
fn a_session_handed_over_as_events_is_the_session_driven_by_the_methods() {
    // The same gestures, once through the methods and once as events: the same ink and the same view.
    let script = |ed: &mut Editor| {
        let mut events = Vec::new();
        let a = on_page(ed, 120.0, 300.0);
        events.push(InputEvent::Pointer(pen(Phase::Down, TIP, &[a])));
        events.push(InputEvent::Pointer(pen(Phase::Move, TIP, &[on_page(ed, 180.0, 330.0), on_page(ed, 240.0, 300.0)])));
        events.push(InputEvent::Pointer(pen(Phase::Up, TIP, &[on_page(ed, 240.0, 300.0)])));
        events.push(InputEvent::Wheel(Wheel::Lines(0.0, -2.0)));
        events.push(InputEvent::MouseMoved([400.0, 400.0]));
        events
    };
    let mut by_events = desktop(Tool::Pen);
    for ev in script(&mut by_events) {
        by_events.handle(ev);
    }
    let mut by_methods = desktop(Tool::Pen);
    for ev in script(&mut by_methods) {
        match ev {
            InputEvent::Pointer(p) => by_methods.pointer_event(p),
            InputEvent::Wheel(w) => by_methods.wheel(w),
            InputEvent::MouseMoved(p) => by_methods.mouse_moved(p),
            other => panic!("{other:?}"),
        }
    }
    let strokes = |ed: &Editor| ed.ink.strokes().iter().map(|s| s.pts.clone()).collect::<Vec<_>>();
    assert_eq!((strokes(&by_events), by_events.view.scroll), (strokes(&by_methods), by_methods.view.scroll));
}
