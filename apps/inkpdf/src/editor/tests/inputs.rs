//! Which device does what, with each tool: the stylus does everything to the page (but nothing with
//! the text tool, which is the mouse's and the keyboard's: see `text`), a finger pans and zooms, the
//! mouse scrolls (and with the text tool places and selects text), and the keyboard only types.

use super::*;

const TOOLS: [Tool; 4] = [Tool::Pen, Tool::Geometry, Tool::Text, Tool::Laser];

fn with(tool: Tool) -> Editor {
    let mut ed = editor();
    ed.set_tool(tool);
    ed.panel = None;
    ed
}

/// Everything about the document and the view that a stray input could change.
fn snapshot(ed: &Editor) -> (usize, Vec<u64>, [f32; 2], f32, bool, bool) {
    (ed.ink.len(), ed.selection.clone(), ed.view.scroll, ed.view.zoom, ed.laser().is_fading() || ed.laser().head().is_some(), ed.ink.is_dirty())
}

#[test]
fn the_mouse_does_nothing_to_the_page_with_any_tool_but_scroll() {
    // (With the text tool it also places and selects text boxes: see `text`.)
    for tool in TOOLS.into_iter().filter(|t| *t != Tool::Text) {
        let mut ed = with(tool);
        add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
        let before = snapshot(&ed);
        // Click, drag and release on the stroke and on empty page, with every button.
        for (button, from, to) in [
            (MouseButton::Left, on_page(&ed, 200.0, 300.0), on_page(&ed, 260.0, 380.0)),
            (MouseButton::Left, on_page(&ed, 400.0, 600.0), on_page(&ed, 500.0, 700.0)),
            (MouseButton::Right, on_page(&ed, 200.0, 300.0), on_page(&ed, 200.0, 300.0)),
            (MouseButton::Middle, on_page(&ed, 200.0, 300.0), on_page(&ed, 100.0, 500.0)),
            (MouseButton::Back, on_page(&ed, 200.0, 300.0), on_page(&ed, 200.0, 300.0)),
        ] {
            ed.mouse_moved(from);
            ed.mouse_button(ElementState::Pressed, button);
            ed.mouse_moved(to);
            ed.mouse_button(ElementState::Released, button);
        }
        assert_eq!(snapshot(&ed), before, "{tool:?}: the mouse changed something");
        assert!(ed.is_idle() && ed.panel.is_none());
        assert!(
            !(ed.requests.has(&Request::Save)
                || ed.requests.has(&Request::Close)
                || ed.requests.has(&Request::Fullscreen)
                || ed.requests.has(&Request::AddPage)
                || ed.requests.insert_page().is_some())
        );
        // The wheel scrolls, up and down, and sideways with a tilting wheel or a touchpad.
        ed.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
        settle(&mut ed);
        assert!(ed.view.scroll[1] > before.2[1], "{tool:?}: the wheel scrolls");
        let zoom = ed.view.zoom;
        ed.wheel(MouseScrollDelta::LineDelta(0.0, 3.0));
        ed.wheel(MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, -40.0)));
        assert_eq!(ed.view.zoom, zoom, "{tool:?}: the wheel never zooms");
    }
}

#[test]
fn a_finger_pans_and_zooms_with_any_tool_and_draws_or_selects_nothing() {
    for tool in TOOLS {
        let mut ed = with(tool);
        add_stroke(&mut ed, &[[100.0, 300.0], [300.0, 300.0]]);
        let before = (ed.ink.len(), ed.selection.clone(), ed.laser().is_fading());
        // One finger pans.
        let scroll = ed.view.scroll;
        ed.pointer_event(touch(1, Phase::Down, on_page(&ed, 200.0, 600.0), 30.0));
        ed.pointer_event(touch(1, Phase::Move, on_page(&ed, 200.0, 300.0), 30.0));
        ed.pointer_event(touch(1, Phase::Up, on_page(&ed, 200.0, 300.0), 30.0));
        assert_ne!(ed.view.scroll, scroll, "{tool:?}: a finger pans");
        // Two fingers pinch.
        let zoom = ed.view.zoom;
        ed.pointer_event(touch(2, Phase::Down, [500.0, 500.0], 30.0));
        ed.pointer_event(touch(3, Phase::Down, [700.0, 500.0], 30.0));
        for i in 1..=10 {
            ed.pointer_event(touch(3, Phase::Move, [700.0 + 20.0 * i as f32, 500.0], 30.0));
        }
        ed.pointer_event(touch(3, Phase::Up, [900.0, 500.0], 30.0));
        ed.pointer_event(touch(2, Phase::Up, [500.0, 500.0], 30.0));
        assert!(ed.view.zoom > zoom, "{tool:?}: two fingers zoom");
        // And none of it drew, selected or pointed.
        assert_eq!((ed.ink.len(), ed.selection.clone(), ed.laser().is_fading()), before, "{tool:?}");
        assert!(!ed.ink.is_dirty() || ed.ink.len() == 1);
    }
}

#[test]
fn the_stylus_does_the_work_and_what_it_does_depends_on_the_tool() {
    let drag = |ed: &mut Editor, buttons: PenButtons| {
        let path: Vec<[f32; 2]> = (0..8).map(|i| on_page(ed, 100.0 + i as f32 * 25.0, 300.0 + (i as f32 * 0.9).sin() * 20.0)).collect();
        pen_stroke(ed, buttons, &path);
    };
    // The pen tool: the tip draws ink.
    let mut ed = with(Tool::Pen);
    drag(&mut ed, TIP);
    assert_eq!((ed.ink.len(), ed.ink.strokes()[0].figure.is_none()), (1, true), "ink, not a shape");
    // The shape tool: the tip draws a shape, selected.
    let mut ed = with(Tool::Geometry);
    drag(&mut ed, TIP);
    assert_eq!((ed.ink.len(), ed.ink.strokes()[0].figure.is_some(), ed.selection.len()), (1, true, 1));
    // The text tool: the stylus does nothing to the page, with any end or button.
    for buttons in [TIP, ERASER, BARREL] {
        let mut ed = with(Tool::Text);
        drag(&mut ed, buttons);
        assert_eq!((ed.ink.len(), ed.selection.len(), ed.text_edit().is_none(), ed.ink.is_dirty()), (0, 0, true, false));
    }
    // The pointer tool: the tip draws nothing and leaves a trail.
    let mut ed = with(Tool::Laser);
    drag(&mut ed, TIP);
    assert_eq!(ed.ink.len(), 0);
    assert!(ed.laser().is_fading());
    // The eraser end erases with the pen and shape tools, and does not with the pointer.
    for (tool, erases) in [(Tool::Pen, true), (Tool::Geometry, true), (Tool::Text, false), (Tool::Laser, false)] {
        let mut ed = with(tool);
        let id = add_stroke(&mut ed, &[[100.0, 300.0], [400.0, 300.0]]);
        let across = [on_page(&ed, 250.0, 250.0), on_page(&ed, 250.0, 300.0), on_page(&ed, 250.0, 350.0)];
        pen_stroke(&mut ed, ERASER, &across);
        assert_eq!(ed.ink.get(id).is_none(), erases, "{tool:?}");
    }
    // The barrel button lassos with the pen and shape tools.
    for (tool, selects) in [(Tool::Pen, true), (Tool::Geometry, true), (Tool::Text, false), (Tool::Laser, false)] {
        let mut ed = with(tool);
        let id = add_stroke(&mut ed, &[[200.0, 300.0], [300.0, 300.0]]);
        let around = [
            on_page(&ed, 150.0, 250.0),
            on_page(&ed, 350.0, 250.0),
            on_page(&ed, 350.0, 350.0),
            on_page(&ed, 150.0, 350.0),
            on_page(&ed, 150.0, 250.0),
        ];
        pen_stroke(&mut ed, BARREL, &around);
        assert_eq!(ed.selection == vec![id], selects, "{tool:?}");
    }
}

#[test]
fn only_the_shape_tool_selects_ink_by_tapping_with_the_stylus() {
    for (tool, selects) in [(Tool::Pen, false), (Tool::Geometry, true), (Tool::Text, false), (Tool::Laser, false)] {
        let mut ed = with(tool);
        let id = add_stroke(&mut ed, &[[100.0, 300.0], [400.0, 300.0]]);
        let on = on_page(&ed, 250.0, 300.0);
        pen_stroke(&mut ed, TIP, &[on, on]);
        assert_eq!(ed.selection == vec![id], selects, "{tool:?}");
    }
}

#[test]
fn the_toolbars_buttons_answer_to_the_stylus_a_finger_and_the_mouse() {
    for tool in TOOLS {
        let mut ed = with(tool);
        let save = ed.toolbar().centre_of(Item::Save);
        mouse_click_at(&mut ed, save);
        assert!(ed.requests.has(&Request::Save), "{tool:?}: mouse");
        ed.requests.take(&Request::Save);
        pen_stroke(&mut ed, TIP, &[save, save]);
        assert!(ed.requests.has(&Request::Save), "{tool:?}: stylus");
        ed.requests.take(&Request::Save);
        ed.palm = crate::input::PalmGuard::default();
        ed.pointer_event(touch(9, Phase::Down, save, 30.0));
        ed.pointer_event(touch(9, Phase::Up, save, 30.0));
        assert!(ed.requests.has(&Request::Save), "{tool:?}: finger");
    }
}
