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
fn the_wheel_a_finger_and_the_window_changes_each_reach_the_view() {
    let mut ed = desktop(Tool::Pen);
    let top = ed.view.scroll;
    ed.handle(InputEvent::Wheel(Wheel::Lines(0.0, -3.0)));
    settle(&mut ed);
    assert!(ed.view.scroll[1] > top[1], "a notch of the wheel scrolls");
    let before = ed.view.scroll;
    ed.handle(InputEvent::Pointer(touch(1, Phase::Down, [300.0, 400.0], 0.0)));
    ed.handle(InputEvent::Pointer(touch(1, Phase::Move, [300.0, 350.0], 0.0)));
    ed.handle(InputEvent::Pointer(touch(1, Phase::Up, [300.0, 350.0], 0.0)));
    assert_ne!(ed.view.scroll, before, "a drag of a finger pans");
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

// ── what the Android reader hands over (decoded motion events) reaches the editor right ────────

mod from_motion_events {
    use super::*;
    use crate::platform::motion_event::{Action, Decoder, RawMotion, RawPointer, Tool as Held};

    fn pointer(id: u32, tool: Held, pos: [f32; 2], pressure: f32) -> RawPointer {
        RawPointer { id, tool, pos, pressure, contact_px: 20.0, history: Vec::new() }
    }

    fn motion(action: Action, t_ms: i64, pointers: Vec<RawPointer>) -> RawMotion {
        RawMotion {
            action,
            action_index: 0,
            t_ns: t_ms * 1_000_000,
            stylus_primary: false,
            stylus_secondary: false,
            primary: false,
            secondary: false,
            pointers,
        }
    }

    /// A one-pointer gesture of `tool` along `points`: down, moves, up.
    fn gesture(tool: Held, id: u32, t0: i64, points: &[[f32; 2]]) -> Vec<RawMotion> {
        let at = |i: usize| pointer(id, tool, points[i], 0.5);
        let mut out = vec![motion(Action::Down, t0, vec![at(0)])];
        for i in 1..points.len() {
            out.push(motion(Action::Move, t0 + i as i64 * 4, vec![at(i)]));
        }
        out.push(motion(Action::Up, t0 + points.len() as i64 * 4, vec![at(points.len() - 1)]));
        out
    }

    fn feed(ed: &mut Editor, decoder: &mut Decoder, motions: Vec<RawMotion>) {
        let mut events = Vec::new();
        for m in motions {
            decoder.motion(&m, &mut events);
        }
        for ev in events {
            ed.handle(ev);
        }
    }

    /// A tablet with the pen tool, as an Android device starts.
    fn tablet() -> Editor {
        let mut ed = editor();
        ed.set_tool(Tool::Pen);
        ed.chrome.panel = None;
        ed.set_mode(Mode::Tablet);
        ed
    }

    /// What a finger dragged up the page does after the pen has written: it pans (the page moves, the zoom does not).
    fn finger_after(ed: &mut Editor, decoder: &mut Decoder, t0: i64) -> (bool, f32) {
        let before = (ed.view.scroll, ed.view.zoom);
        let drag = gesture(
            Held::Finger,
            0,
            t0,
            &[on_page(ed, 300.0, 700.0), on_page(ed, 300.0, 650.0), on_page(ed, 300.0, 600.0), on_page(ed, 300.0, 550.0), on_page(ed, 300.0, 500.0)],
        );
        feed(ed, decoder, drag);
        settle(ed);
        (ed.view.scroll != before.0, ed.view.zoom / before.1)
    }

    #[test]
    fn a_finger_pans_after_the_pen_has_written_whatever_the_screen_reports_of_hover() {
        let line = |ed: &Editor| vec![on_page(ed, 100.0, 300.0), on_page(ed, 160.0, 330.0), on_page(ed, 220.0, 300.0)];
        // (name, hover reported before and after the stroke, hover exit reported)
        for (name, hover, exit) in [("no hover at all", false, false), ("hover, and the pen leaves", true, true), ("hover, and no exit", true, false)]
        {
            let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
            let stroke = line(&ed);
            let mut motions = Vec::new();
            if hover {
                motions.push(motion(Action::HoverEnter, 1, vec![pointer(0, Held::Stylus, stroke[0], 0.0)]));
            }
            motions.extend(gesture(Held::Stylus, 0, 10, &stroke));
            if hover {
                motions.push(motion(Action::HoverMove, 60, vec![pointer(0, Held::Stylus, stroke[2], 0.0)]));
            }
            if exit {
                motions.push(motion(Action::HoverExit, 80, vec![pointer(0, Held::Stylus, stroke[2], 0.0)]));
            }
            feed(&mut ed, &mut decoder, motions);
            assert_eq!(ed.ink.len(), 1, "{name}: written");
            if hover && !exit {
                continue; // (a pen that never says it left is still near: a finger is a palm until it does)
            }
            let (moved, zoom) = finger_after(&mut ed, &mut decoder, 2000);
            assert!(moved && (zoom - 1.0).abs() < 1e-4, "{name}: the finger pans (moved {moved}, zoom x{zoom})");
            assert_eq!(ed.fingers.fingers(), 0, "{name}: nothing is left of the finger");
        }
    }

    /// Whatever pen, finger and hover events a device sends, once every pointer has lifted nothing of a finger is left in the editor
    /// (a finger left behind turns the next one-finger pan into a pinch).
    #[test]
    fn no_finger_is_left_behind_whatever_order_pen_and_fingers_come_in() {
        use crate::stress::Rng;
        use std::collections::BTreeMap;
        for seed in 0..300u64 {
            let mut rng = Rng::new(seed + 1);
            let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
            let mut active: BTreeMap<u32, Held> = BTreeMap::new();
            let mut hovering = false;
            let mut t = 100_i64;
            let mut log = Vec::new();
            let hover_capable = rng.chance(0.5);
            for _ in 0..60 {
                t += 7;
                let pos = [rng.range(100.0, 900.0), rng.range(100.0, 700.0)];
                let all =
                    |active: &BTreeMap<u32, Held>| -> Vec<RawPointer> { active.iter().map(|(&id, &tool)| pointer(id, tool, pos, 0.5)).collect() };
                let index_of = |active: &BTreeMap<u32, Held>, id: u32| active.keys().position(|&k| k == id).unwrap_or(0);
                let motions: Vec<RawMotion> = match rng.below(8) {
                    0 | 1 => {
                        let id = rng.below(4) as u32;
                        if active.contains_key(&id) {
                            continue;
                        }
                        let tool = if rng.chance(0.4) { Held::Stylus } else { Held::Finger };
                        if tool == Held::Stylus && active.values().any(|&k| k == Held::Stylus) {
                            continue;
                        }
                        active.insert(id, tool);
                        hovering = false;
                        let mut m = motion(Action::Down, t, all(&active));
                        m.action_index = index_of(&active, id);
                        log.push(format!("down {id} {tool:?}"));
                        vec![m]
                    }
                    2..=4 => {
                        if active.is_empty() {
                            continue;
                        }
                        log.push("move".to_string());
                        vec![motion(Action::Move, t, all(&active))]
                    }
                    5 => {
                        let Some(&id) = active.keys().nth(rng.below(active.len().max(1))) else { continue };
                        let mut m = motion(Action::Up, t, all(&active));
                        m.action_index = index_of(&active, id);
                        log.push(format!("up {id}"));
                        active.remove(&id);
                        vec![m]
                    }
                    6 if hover_capable && !active.values().any(|&k| k == Held::Stylus) => {
                        let action = if hovering && rng.chance(0.4) { Action::HoverExit } else { Action::HoverMove };
                        hovering = action != Action::HoverExit;
                        log.push(format!("{action:?}"));
                        vec![motion(action, t, vec![pointer(0, Held::Stylus, pos, 0.0)])]
                    }
                    7 if rng.chance(0.15) => {
                        active.clear();
                        log.push("cancel".to_string());
                        vec![motion(Action::Cancel, t, Vec::new())]
                    }
                    _ => continue,
                };
                feed(&mut ed, &mut decoder, motions);
            }
            // Everything lifts.
            while let Some((&id, _)) = active.iter().next() {
                t += 7;
                let mut m = motion(Action::Up, t, all_pointers(&active, pointer));
                m.action_index = active.keys().position(|&k| k == id).unwrap_or(0);
                active.remove(&id);
                feed(&mut ed, &mut decoder, vec![m]);
            }
            assert_eq!(ed.fingers.fingers(), 0, "seed {seed}: a finger was left behind after {log:?}");
        }
    }

    fn all_pointers(active: &std::collections::BTreeMap<u32, Held>, make: fn(u32, Held, [f32; 2], f32) -> RawPointer) -> Vec<RawPointer> {
        active.iter().map(|(&id, &tool)| make(id, tool, [300.0, 300.0], 0.5)).collect()
    }

    #[test]
    fn a_stylus_draws_and_a_finger_does_not() {
        let line =
            |ed: &Editor, y: f32| vec![on_page(ed, 100.0, y), on_page(ed, 160.0, y + 30.0), on_page(ed, 220.0, y), on_page(ed, 280.0, y + 30.0)];
        let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
        let stylus = gesture(Held::Stylus, 1, 10, &line(&ed, 300.0));
        feed(&mut ed, &mut decoder, stylus);
        assert_eq!(ed.ink.len(), 1, "the stylus draws");
        // The same line with a finger draws nothing. (A finger right after a stylus is a palm, which the palm guard ignores, so it
        // is a fresh editor: a finger dragged up the page scrolls it down.)
        let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
        let before = ed.view.scroll;
        let finger = gesture(
            Held::Finger,
            2,
            200,
            &[on_page(&ed, 300.0, 700.0), on_page(&ed, 300.0, 600.0), on_page(&ed, 300.0, 500.0), on_page(&ed, 300.0, 400.0)],
        );
        feed(&mut ed, &mut decoder, finger);
        assert_eq!(ed.ink.len(), 0, "a finger draws nothing");
        settle(&mut ed);
        assert_ne!(ed.view.scroll, before, "it moved the page");
    }

    #[test]
    fn the_eraser_end_erases_and_the_barrel_button_lassos() {
        let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
        add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
        let across = [on_page(&ed, 90.0, 300.0), on_page(&ed, 200.0, 300.0), on_page(&ed, 310.0, 300.0)];
        feed(&mut ed, &mut decoder, gesture(Held::Eraser, 3, 10, &across));
        assert_eq!(ed.ink.len(), 0, "the eraser end erased it");
        let id = add_stroke(&mut ed, &[[100.0, 400.0], [300.0, 400.0]]);
        let ring: Vec<[f32; 2]> =
            [[50.0, 350.0], [350.0, 350.0], [350.0, 450.0], [50.0, 450.0], [50.0, 350.0]].iter().map(|p| on_page(&ed, p[0], p[1])).collect();
        let mut barrel = gesture(Held::Stylus, 4, 100, &ring);
        barrel.iter_mut().for_each(|m| m.stylus_secondary = true);
        feed(&mut ed, &mut decoder, barrel);
        assert_eq!(ed.selection, vec![id], "the barrel button looped it");
    }

    #[test]
    fn the_one_button_of_an_s_pen_erases_where_a_pen_with_no_eraser_end_has_no_other_way() {
        let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
        add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
        let across = [on_page(&ed, 90.0, 300.0), on_page(&ed, 200.0, 300.0), on_page(&ed, 310.0, 300.0)];
        let mut button = gesture(Held::Stylus, 3, 10, &across);
        button.iter_mut().for_each(|m| m.stylus_primary = true);
        feed(&mut ed, &mut decoder, button);
        assert_eq!(ed.ink.len(), 0, "the stylus's button erased it");
        assert!(ed.selection.is_empty(), "and did not lasso");
    }

    #[test]
    fn a_tap_of_a_finger_or_the_stylus_presses_the_toolbar_buttons() {
        // (The buttons did nothing when touch went through a path that could only pan.)
        for tool in [Held::Finger, Held::Stylus] {
            let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
            let at = ed.toolbar().centre_of(Item::Close);
            feed(&mut ed, &mut decoder, gesture(tool, 5, 10, &[at, at]));
            assert!(ed.requests.has(&Request::Close), "{tool:?} pressed the close button");
            let save = ed.toolbar().centre_of(Item::Undo);
            add_stroke(&mut ed, &[[100.0, 300.0], [200.0, 300.0]]);
            feed(&mut ed, &mut decoder, gesture(tool, 5, 100, &[save, save]));
            assert_eq!(ed.ink.len(), 0, "{tool:?} pressed undo");
        }
    }

    #[test]
    fn a_pen_hovering_over_the_page_is_told_and_a_pen_and_a_finger_together_stay_apart() {
        let (mut ed, mut decoder) = (tablet(), Decoder::new(0.0));
        ed.set_tool(Tool::Laser);
        let over = on_page(&ed, 150.0, 300.0);
        feed(&mut ed, &mut decoder, vec![motion(Action::HoverEnter, 1, vec![pointer(7, Held::Stylus, over, 0.0)])]);
        assert!(ed.laser().head().is_some(), "the pointer tool's dot follows a hovering pen");
        feed(&mut ed, &mut decoder, vec![motion(Action::HoverExit, 2, vec![pointer(7, Held::Stylus, over, 0.0)])]);
        // A pen draws while a finger rests on the screen (a palm or a thumb): the stroke is the pen's alone.
        ed.set_tool(Tool::Pen);
        let rest = pointer(1, Held::Finger, on_page(&ed, 500.0, 600.0), 1.0);
        let pen_at = |p: [f32; 2]| pointer(2, Held::Stylus, p, 0.5);
        let pts = [on_page(&ed, 100.0, 300.0), on_page(&ed, 160.0, 330.0), on_page(&ed, 220.0, 300.0)];
        let mut down2 = motion(Action::Down, 10, vec![rest.clone(), pen_at(pts[0])]);
        down2.action_index = 1;
        let mut up2 = motion(Action::Up, 30, vec![rest.clone(), pen_at(pts[2])]);
        up2.action_index = 1;
        feed(
            &mut ed,
            &mut decoder,
            vec![
                motion(Action::Down, 5, vec![rest.clone()]),
                down2,
                motion(Action::Move, 20, vec![rest.clone(), pen_at(pts[1])]),
                up2,
                motion(Action::Up, 40, vec![rest]),
            ],
        );
        assert_eq!(ed.ink.len(), 1, "the pen drew with a finger resting on the screen");
    }
}
