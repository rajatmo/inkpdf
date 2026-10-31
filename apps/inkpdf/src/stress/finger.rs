//! The finger button under hostile input: touches of up to three fingers, the button, the tools and the mode, the window changing size, all in any order. After
//! every event the editor is consistent and nothing floats off the window, and once every finger is up nothing is left under way.

use super::*;
use crate::input::Mode;

fn touch_at(id: u32, phase: Phase, pos: [f32; 2], contact: f32) -> PointerEvent {
    PointerEvent {
        device: Device::Touch,
        id,
        phase,
        samples: vec![Sample { pos, pressure: 1.0, t_ms: crate::clock::now_ms() }],
        pen: PenButtons::default(),
        contact_px: contact,
    }
}

#[test]
fn stress_fingers_that_draw_and_pinch_in_any_order_with_every_tool_and_mode_leave_the_editor_consistent_and_nothing_under_way() {
    let mut seen = std::collections::BTreeMap::<&'static str, usize>::new();
    for seed in 1..=4u64 {
        let mut rng = Rng::new(seed * 7919);
        let mut ed = Editor::reference([1100.0, 900.0], 1.0, crate::motion::Prefs::REDUCED);
        ed.load(Layout::new(&[[612.0, 792.0], [612.0, 792.0], [612.0, 792.0]]), vec![]);
        ed.set_finger_draws(true);
        // (A finger: which of three is down, and where it last was.)
        let mut down: [Option<[f32; 2]>; 3] = [None; 3];
        for i in 0..(10_000 * scale()) {
            let what = match rng.below(20) {
                0..=5 => {
                    // A finger goes down, anywhere in the window.
                    let id = rng.below(3);
                    if down[id].is_none() {
                        let at = [rng.range(0.0, ed.view.viewport[0]), rng.range(0.0, ed.view.viewport[1])];
                        down[id] = Some(at);
                        ed.pointer_event(touch_at(id as u32 + 1, Phase::Down, at, rng.pick(&[20.0, 30.0, 400.0])));
                    }
                    "down"
                }
                6..=11 => {
                    let id = rng.below(3);
                    if let Some(at) = down[id] {
                        let to = [at[0] + rng.range(-40.0, 40.0), at[1] + rng.range(-40.0, 40.0)];
                        down[id] = Some(to);
                        ed.pointer_event(touch_at(id as u32 + 1, Phase::Move, to, 30.0));
                    }
                    "move"
                }
                12..=14 => {
                    let id = rng.below(3);
                    if let Some(at) = down[id].take() {
                        let phase = if rng.chance(0.85) { Phase::Up } else { Phase::Cancel };
                        ed.pointer_event(touch_at(id as u32 + 1, phase, at, 30.0));
                    }
                    "up"
                }
                15 => {
                    let tool = rng.pick(&[Tool::Select, Tool::Pen, Tool::Geometry, Tool::Text, Tool::Lasso, Tool::Eraser, LASER]);
                    ed.set_tool(tool);
                    "tool"
                }
                16 => {
                    let on = rng.chance(0.7);
                    ed.set_finger_draws(on);
                    "button"
                }
                17 => {
                    ed.set_mode(rng.pick(&[Mode::Desktop, Mode::Tablet]));
                    "mode"
                }
                18 => {
                    if rng.chance(0.2) {
                        ed.set_viewport([rng.range(300.0, 2000.0), rng.range(400.0, 1400.0)]);
                    }
                    ed.step_view(0.016);
                    "frame"
                }
                _ => {
                    let key = rng.pick(&[Key::Escape, Key::Delete, Key::Text("x"), Key::Backspace]);
                    ed.press(key);
                    ed.undo();
                    "key"
                }
            };
            *seen.entry(what).or_default() += 1;
            let _ = ed.take_redraw();
            if i % 100 == 0 {
                check_editor(&ed, &format!("seed {seed}, event {i} ({what})"));
            }
        }
        // Every finger lifted: nothing is left drawing, pinching or half done.
        for (id, at) in down.iter_mut().enumerate() {
            if let Some(pos) = at.take() {
                ed.pointer_event(touch_at(id as u32 + 1, Phase::Up, pos, 30.0));
            }
        }
        assert!(!ed.finger_is_drawing() && !ed.pinching(), "seed {seed}: a finger is still down in the editor");
        ed.settle();
        check_editor(&ed, &format!("seed {seed}, at the end"));
    }
    for needed in ["down", "move", "up", "tool", "button", "mode"] {
        assert!(seen.get(needed).copied().unwrap_or(0) > 100, "the storm did {needed} only {:?} times: {seen:?}", seen.get(needed));
    }
}
