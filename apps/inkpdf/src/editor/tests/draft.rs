//! Drawing a polyline or a spline by tapping: a tap starts the stroke, each tap adds a node, a double
//! tap ends it, a double tap near the first node closes it.

use super::*;
use crate::figure::Figure;
use crate::geometry::Shape;

fn drawing(shape: Shape) -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Geometry);
    ed.shape = shape;
    ed.chrome.panel = None;
    ed
}

fn pen_at(ed: &mut Editor, phase: Phase, pos: [f32; 2], t_ms: f64) {
    let buttons = PenButtons { contact: phase != Phase::Up, ..TIP };
    ed.pointer_event(PointerEvent {
        device: Device::Pen,
        id: 1,
        phase,
        samples: vec![Sample { pos, pressure: 0.5, t_ms }],
        pen: buttons,
        contact_px: 0.0,
    });
}

/// A tap of the pen at page point `(x, y)` at `t_ms`.
fn tap(ed: &mut Editor, x: f32, y: f32, t_ms: f64) {
    let at = on_page(ed, x, y);
    pen_at(ed, Phase::Down, at, t_ms);
    pen_at(ed, Phase::Up, at, t_ms + 20.0);
}

/// A double tap.
fn double_tap(ed: &mut Editor, x: f32, y: f32, t_ms: f64) {
    tap(ed, x, y, t_ms);
    tap(ed, x, y, t_ms + 150.0);
}

fn nodes(ed: &Editor) -> Vec<[f32; 2]> {
    ed.draft().map(|d| d.nodes.clone()).unwrap_or_default()
}

fn figure(ed: &Editor) -> Figure {
    ed.ink.strokes().last().and_then(|s| s.figure.clone()).expect("a shape")
}

fn page(x: f32, y: f32) -> [f32; 2] {
    [12.0 + x, 12.0 + y]
}

#[test]
fn taps_start_a_polyline_add_nodes_and_a_double_tap_ends_it() {
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    assert_eq!(nodes(&ed), [page(100.0, 300.0)], "a tap on empty page starts a stroke");
    assert_eq!(ed.ink.len(), 0, "nothing is stored while it is drawn");
    tap(&mut ed, 300.0, 300.0, 3_000.0);
    tap(&mut ed, 300.0, 500.0, 5_000.0);
    assert_eq!(nodes(&ed).len(), 3, "every tap is another node");
    assert!(ed.draft_preview().is_some_and(|(outline, n)| n.len() == 3 && outline.len() == 3), "drawn as straight segments so far");
    // The double tap ends it: its first tap is the last node, the second ends the stroke.
    double_tap(&mut ed, 500.0, 500.0, 7_000.0);
    assert!(ed.draft().is_none());
    assert_eq!(
        figure(&ed),
        Figure::Poly { points: vec![page(100.0, 300.0), page(300.0, 300.0), page(300.0, 500.0), page(500.0, 500.0)], closed: false }
    );
    assert_eq!(ed.ink.len(), 1);
    assert_eq!(ed.selection, vec![ed.ink.strokes()[0].id], "the new shape is selected, anchors showing");
    // All of it is one undo step.
    ed.undo();
    assert_eq!(ed.ink.len(), 0);
}

#[test]
fn a_double_tap_near_the_first_node_closes_the_polygon() {
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    tap(&mut ed, 400.0, 300.0, 3_000.0);
    tap(&mut ed, 250.0, 500.0, 5_000.0);
    double_tap(&mut ed, 104.0, 296.0, 7_000.0);
    assert_eq!(
        figure(&ed),
        Figure::Poly { points: vec![page(100.0, 300.0), page(400.0, 300.0), page(250.0, 500.0)], closed: true },
        "a triangle, with no node doubled up on the first"
    );
    let pts = &ed.ink.strokes()[0].pts;
    assert_eq!(pts.first(), pts.last(), "and its outline comes back to the start");
    // Too few nodes to close: it just ends.
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    double_tap(&mut ed, 104.0, 296.0, 3_000.0);
    assert_eq!(ed.ink.len(), 0, "a single node and a double tap make nothing");
    assert!(ed.draft().is_none());
}

#[test]
fn a_double_tap_that_is_slow_or_far_apart_is_two_nodes() {
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    tap(&mut ed, 300.0, 300.0, 3_000.0);
    tap(&mut ed, 300.0, 300.0, 3_900.0);
    assert!(ed.draft().is_some(), "too slow to be a double tap");
    assert_eq!(nodes(&ed).len(), 3);
    tap(&mut ed, 300.0, 330.0, 4_000.0);
    assert!(ed.draft().is_some(), "too far from the last tap to be a double tap");
    assert_eq!(nodes(&ed).len(), 4);
}

#[test]
fn the_curve_tool_makes_a_spline_through_the_taps_closed_by_a_double_tap_near_the_first() {
    let mut ed = drawing(Shape::Curve);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    tap(&mut ed, 300.0, 200.0, 3_000.0);
    tap(&mut ed, 500.0, 300.0, 5_000.0);
    double_tap(&mut ed, 700.0, 200.0, 7_000.0);
    let Figure::Path { anchors, closed } = figure(&ed) else { panic!("a spline: {:?}", figure(&ed)) };
    assert!(!closed);
    assert_eq!(anchors.len(), 4);
    assert!(anchors.iter().all(|a| a.smooth && (a.fwd != [0.0; 2] || a.back != [0.0; 2])), "every node smooth, with tangents");
    let pts = &ed.ink.strokes()[0].pts;
    assert!(pts.len() > 30, "drawn as a curve through the nodes ({} points)", pts.len());
    for a in &anchors {
        assert!(pts.iter().any(|p| (p[0] - a.pos[0]).abs() < 0.01 && (p[1] - a.pos[1]).abs() < 0.01), "the curve passes through {:?}", a.pos);
    }
    // Closed.
    let mut ed = drawing(Shape::Curve);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    tap(&mut ed, 400.0, 300.0, 3_000.0);
    tap(&mut ed, 250.0, 500.0, 5_000.0);
    double_tap(&mut ed, 102.0, 298.0, 7_000.0);
    assert!(matches!(figure(&ed), Figure::Path { closed: true, ref anchors } if anchors.len() == 3));
}

#[test]
fn undo_while_drawing_takes_back_the_last_node_and_then_the_stroke() {
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    tap(&mut ed, 300.0, 300.0, 3_000.0);
    tap(&mut ed, 300.0, 500.0, 5_000.0);
    ed.undo();
    assert_eq!(nodes(&ed), [page(100.0, 300.0), page(300.0, 300.0)]);
    ed.redo();
    assert_eq!(nodes(&ed).len(), 2, "redo does nothing while drawing");
    ed.undo();
    ed.undo();
    assert!(ed.draft().is_none() && ed.ink.len() == 0);
}

#[test]
fn leaving_the_tool_changing_the_shape_saving_or_adding_a_page_ends_the_stroke_as_it_is() {
    type Step = fn(&mut Editor);
    let steps: [(&str, Step); 4] = [
        ("tool", |ed| ed.set_tool(Tool::Pen)),
        ("shape", |ed| ed.pick_shape(Shape::Rectangle)),
        ("save", |ed| {
            let save = ed.toolbar().centre_of(Item::Save);
            mouse_click_at(ed, save);
        }),
        ("page", |ed| {
            ed.edit_pages(crate::pages::PageEdit::Insert { at: 2, size: [612.0, 792.0] });
        }),
    ];
    for (what, step) in steps {
        let mut ed = drawing(Shape::Line);
        tap(&mut ed, 100.0, 300.0, 1_000.0);
        tap(&mut ed, 300.0, 300.0, 3_000.0);
        tap(&mut ed, 300.0, 500.0, 5_000.0);
        step(&mut ed);
        assert!(ed.draft().is_none(), "{what}");
        assert_eq!(figure(&ed), Figure::Poly { points: vec![page(100.0, 300.0), page(300.0, 300.0), page(300.0, 500.0)], closed: false }, "{what}");
    }
    // One node is not a line: it is dropped.
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    ed.set_tool(Tool::Pen);
    assert_eq!((ed.draft().is_none(), ed.ink.len()), (true, 0));
}

#[test]
fn nodes_snap_to_the_anchors_of_other_shapes_and_stay_on_the_page() {
    let mut ed = drawing(Shape::Line);
    // A line made by a drag, then a stroke by taps that starts a few pixels from its end.
    let (a, b) = (on_page(&ed, 100.0, 200.0), on_page(&ed, 300.0, 200.0));
    pen_at(&mut ed, Phase::Down, a, 100_000.0);
    pen_at(&mut ed, Phase::Move, b, 100_010.0);
    pen_at(&mut ed, Phase::Up, b, 100_020.0);
    assert_eq!(ed.ink.len(), 1);
    let end = figure(&ed).anchor_pos(1).expect("end");
    ed.selection.clear();
    tap(&mut ed, 303.0, 203.0, 110_000.0);
    assert_eq!(nodes(&ed), [end], "a tap on the end of a shape starts the stroke there, exactly");
    assert!(ed.selection.is_empty());
    // A tap off the page is kept on it.
    tap(&mut ed, -500.0, 5_000.0, 112_000.0);
    let last = *nodes(&ed).last().expect("node");
    let rect = ed.page_rect(0).expect("page");
    assert!(last[0] >= rect[0] && last[1] <= rect[3], "{last:?} within {rect:?}");
}

#[test]
fn a_tap_on_a_shapes_outline_selects_it_and_a_tap_on_empty_page_starts_a_stroke_and_lets_go() {
    let mut ed = drawing(Shape::Line);
    let (a, b) = (on_page(&ed, 100.0, 200.0), on_page(&ed, 300.0, 200.0));
    pen_at(&mut ed, Phase::Down, a, 100_000.0);
    pen_at(&mut ed, Phase::Move, b, 100_010.0);
    pen_at(&mut ed, Phase::Up, b, 100_020.0);
    ed.selection.clear();
    tap(&mut ed, 200.0, 200.0, 110_000.0);
    assert_eq!(ed.selection.len(), 1, "a tap on the outline selects");
    assert!(ed.draft().is_none());
    tap(&mut ed, 200.0, 500.0, 112_000.0);
    assert!(ed.selection.is_empty() && ed.draft().is_some(), "a tap on empty page lets go and starts a stroke");
}

#[test]
fn only_the_stylus_draws_by_tapping_in_the_tablet_mode_and_the_other_tools_are_unaffected() {
    // The mouse (as a tablet's) and a finger cannot start a stroke.
    let mut ed = drawing(Shape::Line);
    ed.set_mode(Mode::Tablet);
    let at = on_page(&ed, 100.0, 300.0);
    mouse_click_at(&mut ed, at);
    ed.pointer_event(touch(1, Phase::Down, at, 30.0));
    ed.pointer_event(touch(1, Phase::Up, at, 30.0));
    assert!(ed.draft().is_none());
    // The eraser end and the barrel button end a stroke in progress.
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    tap(&mut ed, 300.0, 300.0, 3_000.0);
    let far = on_page(&ed, 500.0, 600.0);
    pen_stroke(&mut ed, BARREL, &[far, far]);
    assert!(ed.draft().is_none());
    assert_eq!(ed.ink.len(), 1, "the nodes so far became the line");
    // Rectangles, ellipses and axes are still made by dragging; a tap with them lets go.
    for shape in [Shape::Rectangle, Shape::Ellipse, Shape::Axes2d] {
        let mut ed = drawing(shape);
        tap(&mut ed, 100.0, 300.0, 1_000.0);
        assert!(ed.draft().is_none() && ed.ink.len() == 0, "{shape:?}");
    }
    // The pen tool's taps draw dots, as ever.
    let mut ed = editor();
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    assert_eq!((ed.draft().is_none(), ed.ink.len()), (true, 1));
}

#[test]
fn a_drag_still_makes_a_two_node_line_or_curve_straight_away() {
    for shape in [Shape::Line, Shape::Curve] {
        let mut ed = drawing(shape);
        let (a, b) = (on_page(&ed, 100.0, 200.0), on_page(&ed, 300.0, 300.0));
        pen_at(&mut ed, Phase::Down, a, 1_000.0);
        pen_at(&mut ed, Phase::Move, b, 1_010.0);
        pen_at(&mut ed, Phase::Up, b, 1_020.0);
        assert_eq!((ed.ink.len(), ed.draft().is_none()), (1, true), "{shape:?}");
        assert_eq!(figure(&ed).anchor_count(), 2);
    }
}

#[test]
fn hovering_makes_the_next_segment_follow_the_pen() {
    let mut ed = drawing(Shape::Line);
    tap(&mut ed, 100.0, 300.0, 1_000.0);
    let over = on_page(&ed, 300.0, 400.0);
    ed.pointer_event(PointerEvent {
        device: Device::Pen,
        id: 1,
        phase: Phase::Hover,
        samples: vec![Sample { pos: over, pressure: 0.0, t_ms: 2_000.0 }],
        pen: HOVER,
        contact_px: 0.0,
    });
    let (outline, n) = ed.draft_preview().expect("a draft");
    assert_eq!((n.len(), outline.len()), (1, 2), "one node, and a segment to where the pen is");
    assert_eq!(ed.draft().map(|d| d.nodes.len()), Some(1), "but the hover is not a node");
    ed.pointer_event(PointerEvent { device: Device::Pen, id: 1, phase: Phase::Leave, samples: vec![], pen: HOVER, contact_px: 0.0 });
    assert_eq!(ed.draft_preview().map(|p| p.0.len()), Some(1));
}
