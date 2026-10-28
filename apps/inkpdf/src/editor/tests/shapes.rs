use super::*;
use crate::geometry::{self, SHAPES, Shape};

/// An editor with the shape tool chosen, making `shape`.
fn shaping(shape: Shape) -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Geometry);
    ed.shape = shape;
    ed
}

/// A pen drag across the page from `a` to `b` (page coordinates), in a few steps.
fn drag(ed: &mut Editor, a: [f32; 2], b: [f32; 2]) {
    let pts: Vec<[f32; 2]> = (0..=6)
        .map(|i| {
            let t = i as f32 / 6.0;
            on_page(ed, a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
        })
        .collect();
    pen_stroke(ed, TIP, &pts);
}

/// Page points on the screen.
fn page_pts(ed: &Editor, pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    pts.iter().map(|p| on_page(ed, p[0], p[1])).collect()
}

/// A page point in document space.
fn doc(x: f32, y: f32) -> [f32; 2] {
    [12.0 + x, 12.0 + y]
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05
}

#[test]
fn a_drag_makes_each_simple_shape_as_one_stroke_in_the_tools_colour_and_thickness() {
    for shape in SHAPES.into_iter().filter(|s| *s != Shape::Curve) {
        let mut ed = shaping(shape);
        drag(&mut ed, [100.0, 200.0], [300.0, 320.0]);
        assert_eq!(ed.ink.len(), 1, "{shape:?}: exactly one stroke");
        let stroke = &ed.ink.strokes()[0];
        let want = geometry::points(shape, doc(100.0, 200.0), doc(300.0, 320.0));
        assert_eq!(stroke.pts.len(), want.len(), "{shape:?}");
        assert!(stroke.pts.iter().zip(&want).all(|(a, b)| near(*a, *b)), "{shape:?} came out as dragged");
        let (rgba, width) = ed.current_style();
        assert_eq!((stroke.style.rgba, stroke.style.width), (rgba, width));
        assert_eq!(rgba, ed.palette[ToolStyle::SHAPES.colour], "the shape tool's colour, not the pen's");
        assert!(ed.ink.is_dirty(), "{shape:?}: there is something to save");
        assert!(ed.is_idle());
        // One undo takes the whole shape away.
        ed.undo();
        assert_eq!(ed.ink.len(), 0, "{shape:?}");
    }
}

#[test]
fn the_shape_follows_the_pen_while_it_is_down_and_is_only_stored_when_it_lifts() {
    let mut ed = shaping(Shape::Rectangle);
    ed.pointer_event(pen(Phase::Down, TIP, &[on_page(&ed, 100.0, 200.0)]));
    ed.pointer_event(pen(Phase::Move, TIP, &[on_page(&ed, 250.0, 300.0)]));
    let Some((start, end)) = ed.tools.shape.dragging() else { panic!("a shape is being dragged") };
    assert!(near(start, doc(100.0, 200.0)) && near(end, doc(250.0, 300.0)));
    assert_eq!(ed.ink.len(), 0, "nothing is stored until the pen lifts");
    ed.pointer_event(pen(Phase::Move, TIP, &[on_page(&ed, 400.0, 500.0)]));
    ed.pointer_event(pen(Phase::Up, TIP, &[on_page(&ed, 400.0, 500.0)]));
    assert_eq!(ed.ink.len(), 1);
    assert!(near(ed.ink.strokes()[0].pts[2], doc(400.0, 500.0)), "it ended where the pen did");
}

#[test]
fn a_tap_or_a_tiny_drag_makes_nothing_and_a_drag_from_off_the_page_neither() {
    let mut ed = shaping(Shape::Ellipse);
    let at = on_page(&ed, 200.0, 300.0);
    pen_stroke(&mut ed, TIP, &[at, at]);
    assert_eq!(ed.ink.len(), 0, "a tap");
    drag(&mut ed, [200.0, 300.0], [201.5, 301.0]);
    assert_eq!(ed.ink.len(), 0, "a drag of a pixel or two");
    // Left of the page, in the grey margin.
    pen_stroke(&mut ed, TIP, &[[50.0, 300.0], [60.0, 320.0], [100.0, 400.0]]);
    assert_eq!(ed.ink.len(), 0, "started off the page");
    assert!(!ed.ink.is_dirty());
}

#[test]
fn a_shape_is_kept_on_the_page_it_began_on() {
    let mut ed = shaping(Shape::Rectangle);
    // From inside page 0 to far beyond its right edge and bottom.
    let start = on_page(&ed, 500.0, 700.0);
    let far = on_page(&ed, 5000.0, 5000.0);
    pen_stroke(&mut ed, TIP, &[start, [start[0] + 30.0, start[1] + 20.0], far]);
    assert_eq!(ed.ink.len(), 1);
    let stroke = &ed.ink.strokes()[0];
    assert_eq!(stroke.page, 0);
    let b = stroke.pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
    let page = ed.layout.page(0).expect("page");
    assert!(b[2] <= page.x + page.w + 0.01 && b[3] <= page.bottom() + 0.01, "inside the page: {b:?}");
    assert!(b[2] > page.x + page.w - 0.01, "and it went right up to the edge");
}

#[test]
fn the_other_devices_do_not_make_shapes() {
    let mut ed = shaping(Shape::Line);
    // The mouse selects, as ever.
    ed.mouse_moved(on_page(&ed, 100.0, 300.0));
    ed.mouse_button(ElementState::Pressed, MouseButton::Left);
    ed.mouse_moved(on_page(&ed, 300.0, 400.0));
    ed.mouse_button(ElementState::Released, MouseButton::Left);
    // A finger pans.
    let before = ed.view.scroll;
    ed.pointer_event(touch(1, Phase::Down, on_page(&ed, 300.0, 600.0), 30.0));
    ed.pointer_event(touch(1, Phase::Move, on_page(&ed, 300.0, 300.0), 30.0));
    ed.pointer_event(touch(1, Phase::Up, on_page(&ed, 300.0, 300.0), 30.0));
    assert_eq!(ed.ink.len(), 0);
    assert_ne!(ed.view.scroll, before);
}

#[test]
fn the_eraser_end_erases_a_whole_shape_and_the_lasso_picks_one_up_as_one() {
    let mut ed = shaping(Shape::Axes3d);
    drag(&mut ed, [100.0, 200.0], [400.0, 450.0]);
    assert_eq!(ed.ink.len(), 1);
    // Through the middle with the eraser end: gone in one step.
    let across = [on_page(&ed, 250.0, 150.0), on_page(&ed, 250.0, 330.0), on_page(&ed, 250.0, 500.0)];
    pen_stroke(&mut ed, ERASER, &across);
    assert_eq!(ed.ink.len(), 0);
    ed.undo();
    assert_eq!(ed.ink.len(), 1, "one undo brings the whole shape back");
    // Lasso round it with the barrel button.
    let around =
        [on_page(&ed, 60.0, 160.0), on_page(&ed, 440.0, 160.0), on_page(&ed, 440.0, 490.0), on_page(&ed, 60.0, 490.0), on_page(&ed, 60.0, 160.0)];
    pen_stroke(&mut ed, BARREL, &around);
    assert_eq!(ed.selection.len(), 1);
}

#[test]
fn the_pens_and_the_shape_tools_colour_and_thickness_are_separate() {
    let mut ed = editor();
    let (pen_before, shape_before) = (ed.pen, ed.shape_style);
    ed.set_tool(Tool::Geometry);
    ed.shape_style = ToolStyle { colour: 3, width: 9.0 };
    assert_eq!(ed.pen, pen_before);
    ed.set_tool(Tool::Pen);
    assert_eq!(ed.current_style(), (ed.palette[pen_before.colour], pen_before.width));
    ed.set_tool(Tool::Geometry);
    assert_eq!(ed.current_style(), (ed.palette[3], 9.0));
    assert_ne!(shape_before, ed.shape_style);
}

// ── the panel ────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_shape_buttons_open_the_shape_panel_and_choose_the_shape_which_is_remembered() {
    let mut ed = editor();
    let button = ed.toolbar().centre_of(Item::Tool(Tool::Geometry));
    pen_stroke(&mut ed, TIP, &[button, button]);
    assert_eq!((ed.tool, ed.panel.is_some()), (Tool::Geometry, true));
    assert!(ed.requests.has(&Request::SettingsChanged), "the tool choice is saved");
    let mut previous = ed.shape;
    for shape in SHAPES {
        ed.requests.take(&Request::SettingsChanged);
        let at = ed.toolbar().tool_panel(Tool::Geometry, ed.view.viewport[0], false).expect("panel").shape_centre(shape);
        pen_stroke(&mut ed, TIP, &[at, at]);
        assert_eq!(ed.shape, shape);
        assert!(ed.panel.is_some(), "the panel stays open");
        assert_eq!(ed.settings().shape, shape);
        assert_eq!(ed.requests.has(&Request::SettingsChanged), shape != previous, "a new shape is saved, the same one is not");
        previous = shape;
    }
    assert_eq!(ed.ink.len(), 0, "choosing shapes draws nothing");
}

#[test]
fn the_shape_panels_colours_and_thickness_belong_to_the_shape_tool() {
    let mut ed = editor();
    let button = ed.toolbar().centre_of(Item::Tool(Tool::Geometry));
    pen_stroke(&mut ed, TIP, &[button, button]);
    let panel = ed.toolbar().tool_panel(Tool::Geometry, ed.view.viewport[0], false).expect("panel");
    let green = panel.swatch_centre(3);
    pen_stroke(&mut ed, TIP, &[green, green]);
    assert_eq!((ed.shape_style.colour, ed.pen.colour), (3, ToolStyle::PEN.colour));
    // Thickness: drag the slider to the thick end.
    let (a, b) = (panel.point_on(Slider::Width, 0.2), panel.point_on(Slider::Width, 1.0));
    pen_stroke(&mut ed, TIP, &[a, b]);
    assert!((ed.shape_style.width - crate::toolbar::MAX_WIDTH).abs() < 0.01);
    assert_eq!(ed.pen.width, ToolStyle::PEN.width, "the pen is as it was");
    // No smoothing slider here.
    assert_eq!(panel.slider_at(panel.point_on(Slider::Smoothing, 0.5)), None);
    // And the next shape is that thick and green.
    let away = page_pts(&ed, &[[20.0, 600.0], [40.0, 600.0]]);
    pen_stroke(&mut ed, TIP, &away);
    ed.shape = Shape::Line;
    drag(&mut ed, [20.0, 650.0], [60.0, 700.0]);
    let s = &ed.ink.strokes()[0];
    assert_eq!((s.style.rgba, s.style.width.round()), (ed.palette[3], crate::toolbar::MAX_WIDTH.round()));
}

#[test]
fn a_press_on_the_shape_panel_that_is_not_a_control_draws_no_shape() {
    let mut ed = editor();
    let button = ed.toolbar().centre_of(Item::Tool(Tool::Geometry));
    pen_stroke(&mut ed, TIP, &[button, button]);
    let p = ed.toolbar().tool_panel(Tool::Geometry, ed.view.viewport[0], false).expect("panel");
    let blank = [(p.rect[0] + p.rect[2]) / 2.0, p.rect[1] + 20.0];
    pen_stroke(&mut ed, TIP, &[blank, [blank[0] + 30.0, blank[1] + 10.0]]);
    assert_eq!(ed.ink.len(), 0);
    assert!(ed.panel.is_some());
}

#[test]
fn the_tool_and_every_setting_come_back_in_the_next_session() {
    let mut ed = editor();
    ed.set_tool(Tool::Geometry);
    ed.shape = Shape::Axes2d;
    ed.shape_style = ToolStyle { colour: 4, width: 0.75 };
    ed.pen = ToolStyle { colour: 2, width: 6.0 };
    ed.smoothing = 0.2;
    ed.palette[1] = [1, 2, 3, 255];
    let saved = crate::settings::Settings::parse(&ed.settings().to_text());
    let mut next = editor();
    next.apply_settings(&saved);
    assert_eq!((next.tool, next.shape, next.shape_style, next.pen, next.smoothing), (Tool::Geometry, Shape::Axes2d, ed.shape_style, ed.pen, 0.2));
    assert_eq!(next.palette, ed.palette);
    assert!(next.panel.is_none(), "it starts with the panel closed");
}

#[test]
fn choosing_a_tool_a_colour_or_a_thickness_asks_for_the_settings_to_be_saved() {
    let mut ed = editor();
    ed.requests.take(&Request::SettingsChanged);
    ed.set_tool(Tool::Pen);
    assert!(!ed.requests.has(&Request::SettingsChanged), "the tool in use did not change");
    ed.set_tool(Tool::Laser);
    assert!(ed.requests.has(&Request::SettingsChanged));
    ed.requests.take(&Request::SettingsChanged);
    ed.set_tool(Tool::Pen);
    let button = ed.toolbar().centre_of(Item::Tool(Tool::Pen));
    pen_stroke(&mut ed, TIP, &[button, button]);
    ed.requests.take(&Request::SettingsChanged);
    let blue = ed.toolbar().tool_panel(Tool::Pen, ed.view.viewport[0], false).expect("panel").swatch_centre(1);
    pen_stroke(&mut ed, TIP, &[blue, blue]);
    assert!(ed.requests.has(&Request::SettingsChanged), "a colour choice");
}
