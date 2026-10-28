//! What every tool promises, checked against every tool alike (the Liskov side of the design): a tool that
//! breaks one of these cannot be put in the [`ToolSet`](crate::tools::ToolSet).

use crate::stress::Rng;
use crate::tools::{InputPolicy, MouseUse, PageEvent};

use super::*;

const ALL: [Tool; 4] = [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Laser];

/// A tool's own pointer entry, with no policy in front of it.
fn send(ed: &mut Editor, tool: Tool, dev: Device, id: u32, phase: Phase, pts: &[[f32; 2]], pen: PenButtons) {
    let samples = samples(pts, 1000.0);
    ed.tools.get_mut(tool).pointer(&mut ed.ws, &PageEvent { dev, id, phase, samples: &samples, pen });
}

/// The device a tool listens to on the page, if any.
fn listens_to(tool: Tool) -> Device {
    match tool {
        Tool::Text => Device::Mouse,
        _ => Device::Pen,
    }
}

fn policy(tool: Tool, mode: Mode) -> InputPolicy {
    crate::tools::ToolSet::new().get(tool).policy(mode)
}

#[test]
fn each_tools_policy_is_the_device_rule_of_the_design_in_each_mode() {
    use MouseUse::{AsStylus, Itself, Scrolls};
    // (stylus, mouse, keyboard). The finger pans and zooms and the wheel scrolls with every tool in every mode.
    // At a desk the mouse draws as a stylus does (it places text, as itself, with the text tool) and the stylus
    // does nothing in the text tool; as a tablet the mouse only scrolls and the stylus places text.
    let desktop = [
        (Tool::Pen, (true, AsStylus, false)),
        (Tool::Geometry, (true, AsStylus, false)),
        (Tool::Text, (false, Itself, true)),
        (Tool::Laser, (true, AsStylus, false)),
    ];
    let tablet = [
        (Tool::Pen, (true, Scrolls, false)),
        (Tool::Geometry, (true, Scrolls, false)),
        (Tool::Text, (true, Scrolls, true)),
        (Tool::Laser, (true, Scrolls, false)),
    ];
    for (mode, rules) in [(Mode::Desktop, desktop), (Mode::Tablet, tablet)] {
        for (tool, (stylus, mouse, keyboard)) in rules {
            assert_eq!(policy(tool, mode), InputPolicy { stylus, mouse, keyboard }, "{tool:?} in the {mode:?} mode");
        }
    }
}
#[test]
fn a_new_tool_is_idle_and_owns_no_pointer() {
    let ed = editor();
    for tool in ALL {
        let t = ed.tools.get(tool);
        assert!(!t.busy(), "{tool:?}");
        for dev in [Device::Pen, Device::Mouse, Device::Touch] {
            assert!(!t.owns(dev, 0) && !t.owns(dev, 1), "{tool:?} {dev:?}");
        }
        assert_eq!(t.drag_offset(), [0.0, 0.0], "{tool:?}");
    }
}

#[test]
fn a_press_and_lift_of_the_device_a_tool_listens_to_ends_with_the_tool_idle() {
    for tool in ALL {
        let mut ed = editor();
        ed.tool = tool;
        let dev = listens_to(tool);
        let (a, b) = (on_page(&ed, 100.0, 100.0), on_page(&ed, 220.0, 160.0));
        send(&mut ed, tool, dev, 1, Phase::Down, &[a], TIP);
        send(&mut ed, tool, dev, 1, Phase::Move, &[a, b], TIP);
        assert!(ed.tools.get(tool).owns(dev, 1) || tool == Tool::Laser, "{tool:?}: owns what it began");
        send(&mut ed, tool, dev, 1, Phase::Up, &[b], TIP);
        // (A tap-drawn polyline's draft, a text box's edit: pending work, not a gesture; both are ended
        // by `settle`.)
        ed.tools.get_mut(tool).settle(&mut ed.ws);
        assert!(!ed.tools.get(tool).busy(), "{tool:?}");
        assert!(!ed.tools.get(tool).owns(dev, 1), "{tool:?}");
    }
}

#[test]
fn another_pointer_never_ends_or_moves_the_gesture_of_the_first() {
    for tool in [Tool::Pen, Tool::Geometry, Tool::Text] {
        let mut ed = editor();
        ed.tool = tool;
        let dev = listens_to(tool);
        let (a, b) = (on_page(&ed, 100.0, 100.0), on_page(&ed, 300.0, 300.0));
        send(&mut ed, tool, dev, 1, Phase::Down, &[a], TIP);
        let busy = ed.tools.get(tool).busy();
        // A different pointer id moves and lifts: the tool takes no notice of it.
        send(&mut ed, tool, dev, 2, Phase::Move, &[b], TIP);
        send(&mut ed, tool, dev, 2, Phase::Up, &[b], TIP);
        assert_eq!(ed.tools.get(tool).busy(), busy, "{tool:?}");
        assert!(ed.tools.get(tool).owns(dev, 1), "{tool:?}");
    }
}

#[test]
fn cancel_drops_the_gesture_and_leaves_the_document_as_it_was() {
    for tool in ALL {
        let mut ed = editor();
        ed.tool = tool;
        add_stroke(&mut ed, &[[100.0, 100.0], [300.0, 100.0]]);
        let before = (ed.ink.revision(), ed.ink.len(), ed.selection.clone());
        let dev = listens_to(tool);
        let (a, b) = (on_page(&ed, 110.0, 90.0), on_page(&ed, 250.0, 120.0));
        let buttons = if tool == Tool::Pen { ERASER } else { TIP };
        send(&mut ed, tool, dev, 1, Phase::Down, &[a], buttons);
        send(&mut ed, tool, dev, 1, Phase::Move, &[a, b], buttons);
        ed.tools.get_mut(tool).cancel(&mut ed.ws);
        assert!(!ed.tools.get(tool).owns(dev, 1), "{tool:?}");
        assert!(ed.erasing.is_empty(), "{tool:?}: nothing is left marked for erasing");
        assert_eq!((ed.ink.revision(), ed.ink.len(), ed.selection.clone()), before, "{tool:?}");
    }
}

#[test]
fn ending_settling_and_leaving_an_idle_tool_change_nothing_and_can_be_repeated() {
    for tool in ALL {
        let mut ed = editor();
        add_stroke(&mut ed, &[[100.0, 100.0], [300.0, 100.0]]);
        let before = (ed.ink.revision(), ed.ink.len(), ed.selection.clone());
        for _ in 0..2 {
            let t = ed.tools.get_mut(tool);
            t.end(&mut ed.ws);
            t.settle(&mut ed.ws);
            t.leave(&mut ed.ws);
            t.cancel(&mut ed.ws);
            assert!(!t.undo(&mut ed.ws) && !t.redo(&mut ed.ws), "{tool:?}: nothing of its own to take back");
        }
        assert_eq!((ed.ink.revision(), ed.ink.len(), ed.selection.clone()), before, "{tool:?}");
        assert!(!ed.tools.get(tool).busy(), "{tool:?}");
    }
}

#[test]
fn what_a_tool_does_not_listen_to_never_reaches_it() {
    for (tool, mode) in ALL.into_iter().flat_map(|t| [(t, Mode::Desktop), (t, Mode::Tablet)]) {
        let mut ed = editor();
        ed.set_mode(mode);
        ed.tool = tool;
        add_stroke(&mut ed, &[[100.0, 100.0], [300.0, 100.0]]);
        let before = (ed.ink.revision(), ed.ink.len(), ed.selection.clone());
        let policy = policy(tool, ed.mode());
        let (a, b) = (on_page(&ed, 100.0, 100.0), on_page(&ed, 300.0, 100.0));
        for (dev, listens) in [(Device::Pen, policy.stylus), (Device::Mouse, policy.mouse != MouseUse::Scrolls), (Device::Touch, false)] {
            if listens {
                continue;
            }
            for phase in [Phase::Down, Phase::Move, Phase::Up] {
                ed.route_to_tool(dev, 7, phase, &samples(&[a, b], 1000.0), TIP);
            }
            assert!(!ed.tools.busy(), "{tool:?} {dev:?}");
            assert_eq!((ed.ink.revision(), ed.ink.len(), ed.selection.clone()), before, "{tool:?} {dev:?}");
        }
        if !policy.keyboard {
            ed.type_text("hello");
            ed.backspace();
            assert_eq!(ed.ink.revision(), before.0, "{tool:?}: the keyboard does nothing");
        }
    }
}

#[test]
fn any_stream_of_events_leaves_every_tool_sound() {
    let mut rng = Rng::new(0x5EED_C0DE);
    for tool in ALL {
        let mut ed = editor();
        ed.tool = tool;
        add_stroke(&mut ed, &[[100.0, 100.0], [300.0, 100.0]]);
        for _ in 0..4000 {
            let dev = rng.pick(&[Device::Pen, Device::Mouse, Device::Touch]);
            let phase = rng.pick(&[Phase::Down, Phase::Move, Phase::Move, Phase::Up, Phase::Cancel, Phase::Hover, Phase::Leave]);
            let pen = rng.pick(&[TIP, ERASER, BARREL, HOVER]);
            let id = rng.below(3) as u32;
            // Mostly on the page, sometimes far away from it.
            let extent = if rng.chance(0.1) { 1.0e7 } else { 700.0 };
            let pts: Vec<[f32; 2]> = (0..1 + rng.below(4)).map(|_| on_page(&ed, rng.coord(extent), rng.coord(extent))).collect();
            ed.route_to_tool(dev, id, phase, &samples(&pts, 1000.0), pen);
            if rng.chance(0.01) {
                ed.set_mode(rng.pick(&[Mode::Desktop, Mode::Tablet]));
            }
            if rng.chance(0.02) {
                ed.type_text("x\n");
            }
        }
        ed.tools.end_all(&mut ed.ws);
        ed.tools.settle_all(&mut ed.ws);
        assert!(!ed.tools.busy(), "{tool:?}: nothing left half done");
        for s in ed.ink.strokes() {
            assert!(s.pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "{tool:?}: a stroke with a point that is not finite");
        }
    }
}
