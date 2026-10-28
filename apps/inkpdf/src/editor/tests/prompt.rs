use super::*;
use crate::toolbar::{Choice, PromptLayout};

/// The screen position of the middle of a prompt button.
fn button(ed: &Editor, choice: Choice) -> [f32; 2] {
    let layout = PromptLayout::new(ed.view.viewport, ed.dpr);
    let r = layout.buttons.iter().find(|(c, _)| *c == choice).expect("button").1;
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

fn asking() -> Editor {
    let mut ed = editor();
    ed.ask_unsaved();
    assert!(ed.prompt.is_some());
    ed
}

#[test]
fn a_tap_with_the_pen_a_finger_or_the_mouse_answers_and_puts_the_prompt_away() {
    for choice in [Choice::Save, Choice::Discard, Choice::Cancel] {
        // Pen.
        let mut ed = asking();
        let at = button(&ed, choice);
        ed.pointer_event(pen(Phase::Down, TIP, &[at]));
        assert!(ed.requests.answer().is_none(), "not until it is let go");
        ed.pointer_event(pen(Phase::Up, TIP, &[at]));
        assert_eq!((ed.requests.answer(), ed.prompt.is_some()), (Some(choice), false), "pen {choice:?}");
        // Finger.
        let mut ed = asking();
        let at = button(&ed, choice);
        ed.pointer_event(touch(1, Phase::Down, at, 30.0));
        ed.pointer_event(touch(1, Phase::Up, at, 30.0));
        assert_eq!((ed.requests.answer(), ed.prompt.is_some()), (Some(choice), false), "finger {choice:?}");
        // Mouse.
        let mut ed = asking();
        let at = button(&ed, choice);
        mouse_click_at(&mut ed, at);
        assert_eq!((ed.requests.answer(), ed.prompt.is_some()), (Some(choice), false), "mouse {choice:?}");
    }
}

#[test]
fn a_press_only_counts_if_it_is_let_go_on_the_same_button() {
    let mut ed = asking();
    let (save, cancel) = (button(&ed, Choice::Save), button(&ed, Choice::Cancel));
    ed.pointer_event(touch(1, Phase::Down, save, 30.0));
    ed.pointer_event(touch(1, Phase::Move, cancel, 30.0));
    ed.pointer_event(touch(1, Phase::Up, cancel, 30.0));
    assert_eq!(ed.requests.answer(), None, "pressed on one, released on another");
    assert!(ed.prompt.is_some(), "and still asking");
    // Pressed off the card, let go on a button.
    ed.pointer_event(touch(1, Phase::Down, [5.0, 5.0], 30.0));
    ed.pointer_event(touch(1, Phase::Up, save, 30.0));
    assert_eq!(ed.requests.answer(), None);
    // A cancelled touch never answers.
    ed.pointer_event(touch(1, Phase::Down, save, 30.0));
    ed.pointer_event(touch(1, Phase::Cancel, save, 30.0));
    ed.pointer_event(touch(1, Phase::Up, save, 30.0));
    assert_eq!(ed.requests.answer(), None);
    // And a clean press and release still works afterwards.
    ed.pointer_event(touch(1, Phase::Down, save, 30.0));
    ed.pointer_event(touch(1, Phase::Up, save, 30.0));
    assert_eq!(ed.requests.answer(), Some(Choice::Save));
}

#[test]
fn the_button_under_the_pointer_lights_up_and_a_pressed_one_darkens() {
    let mut ed = asking();
    let save = button(&ed, Choice::Save);
    ed.mouse_moved(save);
    assert_eq!(ed.prompt.expect("prompt").hover, Some(Choice::Save));
    ed.mouse_button(ElementState::Pressed, MouseButton::Left);
    assert_eq!(ed.prompt.expect("prompt").pressed, Some(Choice::Save));
    ed.mouse_moved(button(&ed, Choice::Discard));
    assert_eq!(ed.prompt.expect("prompt").pressed, None, "dragged off it, so it is no longer pressed");
    ed.mouse_moved([5.0, 5.0]);
    assert_eq!(ed.prompt.expect("prompt").hover, None);
}

#[test]
fn nothing_underneath_hears_anything_while_it_is_up() {
    let mut ed = asking();
    let (before_scroll, before_zoom) = (ed.view.scroll, ed.view.zoom);
    let a = on_page(&ed, 100.0, 300.0);
    let b = on_page(&ed, 400.0, 300.0);
    // The pen draws nothing, the eraser end and the barrel button do nothing.
    pen_stroke(&mut ed, TIP, &[a, b]);
    pen_stroke(&mut ed, ERASER, &[a, b]);
    pen_stroke(&mut ed, BARREL, &[a, b]);
    // A finger does not pan or pinch.
    ed.pointer_event(touch(1, Phase::Down, [500.0, 600.0], 30.0));
    ed.pointer_event(touch(1, Phase::Move, [500.0, 300.0], 30.0));
    ed.pointer_event(touch(2, Phase::Down, [700.0, 600.0], 30.0));
    ed.pointer_event(touch(2, Phase::Move, [900.0, 600.0], 30.0));
    ed.pointer_event(touch(2, Phase::Up, [900.0, 600.0], 30.0));
    ed.pointer_event(touch(1, Phase::Up, [500.0, 300.0], 30.0));
    // The mouse does not scroll or press anything under the prompt.
    ed.mouse_moved(a);
    ed.mouse_button(ElementState::Pressed, MouseButton::Left);
    ed.mouse_moved(b);
    ed.mouse_button(ElementState::Released, MouseButton::Left);
    ed.mouse_button(ElementState::Pressed, MouseButton::Middle);
    ed.mouse_button(ElementState::Released, MouseButton::Middle);
    ed.mouse_button(ElementState::Released, MouseButton::Right);
    ed.wheel(MouseScrollDelta::LineDelta(0.0, -5.0));
    ed.wheel(MouseScrollDelta::LineDelta(0.0, 5.0));
    ed.basic_touch(winit::event::TouchPhase::Started, 1, a);
    ed.basic_touch(winit::event::TouchPhase::Moved, 1, b);
    // The toolbar's buttons are out of reach too.
    let close = ed.toolbar().centre_of(Item::Close);
    mouse_click_at(&mut ed, close);
    ed.pointer_event(pen(Phase::Down, TIP, &[close]));
    ed.pointer_event(pen(Phase::Up, TIP, &[close]));

    assert_eq!(ed.ink.len(), 0);
    assert!(ed.selection.is_empty() && ed.is_idle());
    assert_eq!((ed.view.scroll, ed.view.zoom), (before_scroll, before_zoom));
    assert!(
        !(ed.requests.has(&Request::Close)
            || ed.requests.has(&Request::Save)
            || ed.requests.has(&Request::AddPage)
            || ed.requests.has(&Request::Fullscreen))
    );
    assert!(ed.requests.answer().is_none() && ed.prompt.is_some(), "and the prompt is still waiting");
}

#[test]
fn putting_the_prompt_up_stops_what_was_under_way() {
    let mut ed = editor();
    let a = on_page(&ed, 100.0, 300.0);
    ed.pointer_event(pen(Phase::Down, TIP, &[a]));
    assert!(ed.tools.pen.is_drawing());
    ed.panel = Some(crate::toolbar::PanelState::default());
    ed.ask_unsaved();
    assert!(ed.is_idle() && ed.panel.is_none());
    // The pen coming up afterwards is the prompt's business, not a stroke's.
    ed.pointer_event(pen(Phase::Up, TIP, &[a]));
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn loading_a_document_clears_the_prompt() {
    let mut ed = asking();
    ed.load(Layout::new(&[[612.0, 792.0]]), vec![]);
    assert!(ed.prompt.is_none() && ed.requests.answer().is_none());
}
