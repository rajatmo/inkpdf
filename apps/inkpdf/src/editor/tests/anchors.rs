use super::*;
use crate::figure::{Anchor, Control, Figure};
use crate::geometry::Shape;

/// An editor with the shape tool making `shape`.
fn shaping(shape: Shape) -> Editor {
    let mut ed = editor();
    ed.set_tool(Tool::Geometry);
    ed.shape = shape;
    ed
}

/// A pen event at one place and time.
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

/// The same with the barrel button held.
fn barrel_at(ed: &mut Editor, phase: Phase, pos: [f32; 2], t_ms: f64) {
    let buttons = PenButtons { contact: phase != Phase::Up, barrel: true, ..TIP };
    ed.pointer_event(PointerEvent {
        device: Device::Pen,
        id: 1,
        phase,
        samples: vec![Sample { pos, pressure: 0.5, t_ms }],
        pen: buttons,
        contact_px: 0.0,
    });
}

/// A drag from `a` to `b` (screen), starting at `t_ms`.
fn drag_at(ed: &mut Editor, a: [f32; 2], b: [f32; 2], t_ms: f64) {
    pen_at(ed, Phase::Down, a, t_ms);
    pen_at(ed, Phase::Move, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], t_ms + 8.0);
    pen_at(ed, Phase::Move, b, t_ms + 16.0);
    pen_at(ed, Phase::Up, b, t_ms + 24.0);
}

/// Two quick taps at `p`: a double tap.
fn double_tap(ed: &mut Editor, p: [f32; 2], t_ms: f64) {
    for t in [t_ms, t_ms + 150.0] {
        pen_at(ed, Phase::Down, p, t);
        pen_at(ed, Phase::Up, p, t + 20.0);
    }
}

/// A tap of the barrel button at `p`.
fn barrel_tap(ed: &mut Editor, p: [f32; 2], t_ms: f64) {
    barrel_at(ed, Phase::Down, p, t_ms);
    barrel_at(ed, Phase::Up, p, t_ms + 30.0);
}

/// Draw `shape` across the page from `a` to `b` (page coordinates).
fn draw(ed: &mut Editor, a: [f32; 2], b: [f32; 2]) {
    let (a, b) = (on_page(ed, a[0], a[1]), on_page(ed, b[0], b[1]));
    drag_at(ed, a, b, 10_000.0);
}

/// The figure being edited.
fn figure(ed: &Editor) -> Figure {
    ed.editing().expect("a shape selected").1.clone()
}

fn figure_of(ed: &Editor) -> Figure {
    ed.ink.strokes()[0].figure.clone().expect("a shape")
}

fn figure_of_stroke(ed: &Editor, i: usize) -> Figure {
    ed.ink.strokes()[i].figure.clone().expect("a shape")
}

/// Where a control of the shape being edited is on the screen.
fn at(ed: &Editor, control: Control) -> [f32; 2] {
    ed.view.doc_to_screen(figure(ed).control_pos(control).expect("control"))
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.1 && (a[1] - b[1]).abs() < 0.1
}

fn spline(f: &Figure) -> Vec<Anchor> {
    match f {
        Figure::Path { anchors, .. } => anchors.clone(),
        other => panic!("not a spline: {other:?}"),
    }
}

fn nodes(f: &Figure) -> Vec<[f32; 2]> {
    (0..f.anchor_count()).filter_map(|i| f.anchor_pos(i)).collect()
}

/// A broken line of three nodes, selected: a line from (100, 300) to (300, 300) with a node added
/// halfway along it (double tap) and pulled up to (200, 200).
fn broken_line(ed: &mut Editor) {
    ed.shape = Shape::Line;
    draw(ed, [100.0, 300.0], [300.0, 300.0]);
    double_tap(ed, on_page(ed, 200.0, 300.0), 20_000.0);
    let node = at(ed, Control::Anchor(1));
    drag_at(ed, node, on_page(ed, 200.0, 200.0), 30_000.0);
}

// ── what a new shape shows ───────────────────────────────────────────────────────────────────

#[test]
fn a_new_shape_is_selected_with_the_anchors_of_its_kind_showing() {
    for (shape, anchors) in [(Shape::Line, 2), (Shape::Curve, 4), (Shape::Rectangle, 2), (Shape::Ellipse, 4), (Shape::Axes2d, 5), (Shape::Axes3d, 7)]
    {
        let mut ed = shaping(shape);
        draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
        assert_eq!(ed.ink.len(), 1, "{shape:?}");
        let (id, f) = ed.editing().expect("the new shape is being edited");
        assert_eq!(ed.selection, vec![id]);
        assert_eq!(f.controls().len(), anchors, "{shape:?}: its anchors and handles");
        assert_eq!(ed.selected_anchor(), None, "none pressed yet");
    }
}

// ── straight lines ───────────────────────────────────────────────────────────────────────────

#[test]
fn dragging_a_node_reshapes_the_shape_and_the_stored_ink_changes_only_when_the_pen_lifts() {
    let mut ed = shaping(Shape::Line);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = ed.ink.strokes()[0].pts.clone();
    let end = at(&ed, Control::Anchor(1));
    let to = [end[0] + 40.0, end[1] + 25.0];
    pen_at(&mut ed, Phase::Down, end, 20_000.0);
    assert!(matches!(ed.tools.shape.controlling(), Some((_, Control::Anchor(1), _))));
    pen_at(&mut ed, Phase::Move, to, 20_010.0);
    assert_eq!(ed.ink.strokes()[0].pts, before, "still the old shape in the store while the pen is down");
    let Some((_, _, figure)) = ed.tools.shape.controlling() else { panic!() };
    assert!(near(ed.view.doc_to_screen(figure.anchor_pos(1).expect("node")), to), "but the shape follows the pen");
    pen_at(&mut ed, Phase::Up, to, 20_020.0);
    let moved = figure_of(&ed);
    assert!(near(ed.view.doc_to_screen(moved.anchor_pos(1).expect("node")), to));
    assert_eq!(ed.ink.strokes()[0].pts, moved.points(), "the stroke is what the figure draws");
    assert_eq!(ed.ink.len(), 1, "no new stroke");
    assert_eq!(ed.selected_anchor(), Some(1), "the node pressed is the selected one");
    // One undo takes the whole drag back, and redo brings it again.
    ed.undo();
    assert_eq!(ed.ink.strokes()[0].pts, before);
    ed.redo();
    assert_eq!(ed.ink.strokes()[0].pts, moved.points());
}

#[test]
fn a_tap_on_a_node_selects_it_without_changing_the_shape() {
    let mut ed = shaping(Shape::Line);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = ed.ink.strokes()[0].pts.clone();
    let p = at(&ed, Control::Anchor(1));
    pen_at(&mut ed, Phase::Down, p, 30_000.0);
    pen_at(&mut ed, Phase::Up, p, 30_020.0);
    assert_eq!(ed.selected_anchor(), Some(1));
    assert_eq!(ed.ink.strokes()[0].pts, before);
    ed.undo();
    assert_eq!(ed.ink.len(), 0, "the only undo step is the drawing of the shape, not the tap");
}

#[test]
fn a_double_tap_on_the_line_adds_a_straight_node_and_dragging_it_leaves_every_segment_straight() {
    let mut ed = shaping(Shape::Line);
    draw(&mut ed, [100.0, 300.0], [300.0, 300.0]);
    let before = figure_of(&ed).points();
    let spot = on_page(&ed, 200.0, 300.0);
    double_tap(&mut ed, spot, 70_000.0);
    let f = figure_of(&ed);
    assert_eq!(f.anchor_count(), 3);
    assert_eq!(ed.selected_anchor(), Some(1), "the new node is the selected one");
    assert_eq!(f.points().len(), 3, "a node, and nothing between nodes");
    assert!(
        f.points()
            .iter()
            .zip([before[0], [212.0, 312.0], before[1]])
            .all(|(a, b)| (a[0] - b[0]).abs() < 0.1 && (a[1] - b[1]).abs() < 0.1 || a[1] == b[1])
    );
    // Pull the new node up by a little: the line is two straight segments, exactly through its nodes.
    let node = at(&ed, Control::Anchor(1));
    drag_at(&mut ed, node, [node[0], node[1] - 12.0], 80_000.0);
    let f = figure_of(&ed);
    assert_eq!(ed.ink.strokes()[0].pts, nodes(&f), "the stored ink is exactly the nodes");
    ed.undo();
    assert_eq!(figure_of(&ed).anchor_count(), 3);
    ed.undo();
    assert_eq!(figure_of(&ed).anchor_count(), 2);
}

#[test]
fn a_double_tap_on_a_node_closes_a_broken_line_into_a_polygon_and_opens_a_polygon_again() {
    let mut ed = shaping(Shape::Line);
    broken_line(&mut ed);
    assert_eq!(figure_of(&ed).anchor_count(), 3);
    // On an end node: the line closes.
    let end = at(&ed, Control::Anchor(0));
    double_tap(&mut ed, end, 100_000.0);
    assert!(matches!(figure_of(&ed), Figure::Poly { closed: true, .. }), "{:?}", figure_of(&ed));
    assert_eq!(figure_of(&ed).anchor_count(), 3);
    let stored = &ed.ink.strokes()[0].pts;
    assert_eq!(stored.first(), stored.last(), "a polygon comes back to where it started");
    // On a node of the polygon: it opens there.
    let node = at(&ed, Control::Anchor(1));
    double_tap(&mut ed, node, 110_000.0);
    assert!(matches!(figure_of(&ed), Figure::Poly { closed: false, .. }));
    assert_eq!(figure_of(&ed).anchor_count(), 4, "it runs round from the node and ends on it");
    ed.undo();
    assert!(matches!(figure_of(&ed), Figure::Poly { closed: true, .. }));
}

#[test]
fn an_end_dragged_onto_the_other_end_closes_a_broken_line_into_a_polygon() {
    let mut ed = shaping(Shape::Line);
    broken_line(&mut ed);
    // A fourth node on the second segment, so that closing leaves a polygon of three.
    let on = on_page(&ed, 250.0, 250.0);
    double_tap(&mut ed, on, 120_000.0);
    assert_eq!(figure_of(&ed).anchor_count(), 4);
    let start = figure_of(&ed).anchor_pos(0).expect("start");
    let last = at(&ed, Control::Anchor(3));
    let close = ed.view.doc_to_screen([start[0] + 4.0, start[1] - 3.0]);
    pen_at(&mut ed, Phase::Down, last, 130_000.0);
    pen_at(&mut ed, Phase::Move, close, 130_010.0);
    assert_eq!(ed.snap(), Some(start), "it snaps to the other end while dragging");
    pen_at(&mut ed, Phase::Up, close, 130_020.0);
    let f = figure_of(&ed);
    assert!(matches!(f, Figure::Poly { closed: true, .. }), "{f:?}");
    assert_eq!(f.anchor_count(), 3, "the two end nodes became one");
    ed.undo();
    assert_eq!(figure_of(&ed).anchor_count(), 4, "and one undo opens it again");
}

#[test]
fn the_barrel_button_on_empty_page_adds_a_node_at_the_end_and_near_the_start_closes() {
    let mut ed = shaping(Shape::Line);
    draw(&mut ed, [100.0, 300.0], [300.0, 300.0]);
    {
        let spot_ = on_page(&ed, 400.0, 400.0);
        barrel_tap(&mut ed, spot_, 140_000.0);
    }
    let f = figure_of(&ed);
    assert_eq!(f.anchor_count(), 3, "a node where the tap was");
    assert_eq!(f.anchor_pos(2), Some([412.0, 412.0]), "at the end");
    assert_eq!(ed.selected_anchor(), Some(2), "and it is the node touched last");
    assert_eq!(ed.selection.len(), 1, "the line is still selected: the tap did not start a lasso");
    {
        let spot_ = on_page(&ed, 200.0, 450.0);
        barrel_tap(&mut ed, spot_, 150_000.0);
    }
    assert_eq!(figure_of(&ed).anchor_count(), 4, "the next tap goes on the end again");
    assert_eq!(figure_of(&ed).anchor_pos(3), Some([212.0, 462.0]));
    ed.undo();
    ed.undo();
    // When the first node was the one touched last, it is the start that grows.
    let first = at(&ed, Control::Anchor(0));
    pen_at(&mut ed, Phase::Down, first, 170_000.0);
    pen_at(&mut ed, Phase::Up, first, 170_020.0);
    assert_eq!(ed.selected_anchor(), Some(0));
    {
        let spot_ = on_page(&ed, 50.0, 300.0);
        barrel_tap(&mut ed, spot_, 180_000.0);
    }
    let f = figure_of(&ed);
    assert_eq!((f.anchor_count(), f.anchor_pos(0)), (3, Some([62.0, 312.0])), "{f:?}");
    // Only a straight run of lines takes nodes this way: with anything else a barrel tap lets go.
    let mut ed = shaping(Shape::Rectangle);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    {
        let spot_ = on_page(&ed, 400.0, 400.0);
        barrel_tap(&mut ed, spot_, 190_000.0);
    }
    assert_eq!(figure_of(&ed).anchor_count(), 2);
    assert!(ed.selection.is_empty());
}
#[test]
fn the_barrel_button_on_a_node_takes_it_away_while_enough_are_left() {
    let mut ed = shaping(Shape::Line);
    broken_line(&mut ed);
    let p = at(&ed, Control::Anchor(1));
    barrel_tap(&mut ed, p, 90_000.0);
    assert_eq!(figure_of(&ed).anchor_count(), 2, "a broken line minus its corner is a line");
    assert_eq!(ed.selected_anchor(), None);
    assert_eq!((ed.ink.len(), ed.selection.len()), (1, 1), "the shape stayed, and is still selected");
    // A line cannot lose an end and stay a line.
    let p = at(&ed, Control::Anchor(0));
    barrel_tap(&mut ed, p, 91_000.0);
    assert_eq!(figure_of(&ed).anchor_count(), 2);
    assert_eq!(ed.ink.len(), 1);
    ed.undo();
    assert_eq!(figure_of(&ed).anchor_count(), 3);
}

#[test]
fn a_drawing_end_and_a_dragged_node_snap_to_the_anchors_of_other_shapes() {
    let mut ed = shaping(Shape::Line);
    draw(&mut ed, [100.0, 200.0], [300.0, 200.0]);
    let target = figure_of(&ed).anchor_pos(1).expect("node");
    ed.selection.clear();
    // A new line starting a few pixels from that end starts exactly on it.
    let near_end = ed.view.doc_to_screen([target[0] + 4.0, target[1] + 3.0]);
    let far = on_page(&ed, 300.0, 400.0);
    pen_at(&mut ed, Phase::Down, near_end, 100_000.0);
    assert!(ed.snap().is_some(), "the marker shows where it has snapped to");
    pen_at(&mut ed, Phase::Move, far, 100_010.0);
    pen_at(&mut ed, Phase::Up, far, 100_020.0);
    assert!(ed.snap().is_none(), "and goes when the pen lifts");
    let line = ed.ink.strokes().last().expect("the new line");
    assert_eq!(line.pts[0], target, "it starts exactly on the other line's end");
    // Dragging that line's far end to near the first line's start snaps it too.
    let start = figure_of_stroke(&ed, 0).anchor_pos(0).expect("node");
    let end = at(&ed, Control::Anchor(1));
    let close = ed.view.doc_to_screen([start[0] - 5.0, start[1] + 5.0]);
    pen_at(&mut ed, Phase::Down, end, 110_000.0);
    pen_at(&mut ed, Phase::Move, close, 110_010.0);
    let Some((_, _, figure)) = ed.tools.shape.controlling() else { panic!("an anchor is being moved") };
    assert_eq!(figure.anchor_pos(1), Some(start), "snapped while dragging");
    assert_eq!(ed.snap(), Some(start));
    pen_at(&mut ed, Phase::Up, close, 110_020.0);
    assert_eq!(ed.ink.strokes().last().expect("line").pts.last(), Some(&start));
    // Far from anything it does not snap.
    let free = on_page(&ed, 450.0, 500.0);
    let handle = at(&ed, Control::Anchor(1));
    drag_at(&mut ed, handle, free, 120_000.0);
    assert!(near(ed.view.doc_to_screen(*ed.ink.strokes().last().expect("line").pts.last().expect("end")), free));
}

// ── splines ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn dragging_a_tangent_handle_bends_the_spline_and_a_smooth_anchors_other_handle_follows() {
    let mut ed = shaping(Shape::Curve);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    // Add an anchor in the middle so that one has two handles.
    let mid = ed.ink.strokes()[0].pts[ed.ink.strokes()[0].pts.len() / 2];
    {
        let spot_ = ed.view.doc_to_screen(mid);
        double_tap(&mut ed, spot_, 40_000.0);
    }
    assert_eq!(figure_of(&ed).anchor_count(), 3);
    let before = spline(&figure_of(&ed));
    let handle = at(&ed, Control::Fwd(1));
    let to = [handle[0] + 10.0, handle[1] + 30.0];
    drag_at(&mut ed, handle, to, 50_000.0);
    let after = spline(&figure_of(&ed));
    assert_ne!(after[1].fwd, before[1].fwd, "the handle moved");
    let (f, b) = (after[1].fwd, after[1].back);
    assert!((f[0] * b[1] - f[1] * b[0]).abs() < 0.05 && f[0] * b[0] + f[1] * b[1] < 0.0, "the other handle swung round to stay in line: {f:?} {b:?}");
    assert!((b[0].hypot(b[1]) - before[1].back[0].hypot(before[1].back[1])).abs() < 0.01, "and kept its length");
    assert!(after[1].smooth);
    assert_eq!(after[0], before[0], "the other anchors are untouched");
}

#[test]
fn the_barrel_button_breaks_a_tangent_handle_off_so_it_moves_alone() {
    let mut ed = shaping(Shape::Curve);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let mid = ed.ink.strokes()[0].pts[ed.ink.strokes()[0].pts.len() / 2];
    {
        let spot_ = ed.view.doc_to_screen(mid);
        double_tap(&mut ed, spot_, 40_000.0);
    }
    let before = spline(&figure_of(&ed));
    let handle = at(&ed, Control::Fwd(1));
    let to = [handle[0] + 10.0, handle[1] + 30.0];
    for (phase, pos, t) in [(Phase::Down, handle, 50_000.0), (Phase::Move, to, 50_010.0), (Phase::Up, to, 50_020.0)] {
        barrel_at(&mut ed, phase, pos, t);
    }
    let after = spline(&figure_of(&ed));
    assert_eq!(after[1].back, before[1].back, "the partner handle stayed");
    assert_ne!(after[1].fwd, before[1].fwd);
    assert!(!after[1].smooth, "the anchor is now a corner");
    assert!(ed.selection.len() == 1, "and the barrel press did not start a lasso");
}

#[test]
fn a_double_tap_turns_a_spline_anchor_between_smooth_and_a_corner() {
    let mut ed = shaping(Shape::Curve);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let p = at(&ed, Control::Anchor(0));
    let tap = |ed: &mut Editor, t: f64| {
        pen_at(ed, Phase::Down, p, t);
        pen_at(ed, Phase::Up, p, t + 20.0);
    };
    tap(&mut ed, 60_000.0);
    assert!(spline(&figure_of(&ed))[0].smooth, "one tap does not convert");
    tap(&mut ed, 60_200.0);
    let a = spline(&figure_of(&ed))[0];
    assert!(!a.smooth && a.fwd == [0.0; 2], "a second tap soon after: the handles fold away {a:?}");
    assert_eq!(ed.ink.strokes()[0].pts, figure_of(&ed).points());
    tap(&mut ed, 80_000.0);
    tap(&mut ed, 80_200.0);
    let a = spline(&figure_of(&ed))[0];
    assert!(a.smooth && a.fwd != [0.0; 2], "and a double tap again brings them out");
    // Two taps far apart in time are two single taps.
    tap(&mut ed, 100_000.0);
    tap(&mut ed, 100_900.0);
    assert!(spline(&figure_of(&ed))[0].smooth);
    ed.undo();
    assert!(!spline(&figure_of(&ed))[0].smooth, "undo puts the conversion back");
}

#[test]
fn a_double_tap_on_a_splines_outline_adds_an_anchor_without_changing_the_shape() {
    let mut ed = shaping(Shape::Curve);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = figure_of(&ed).points();
    let mid = before[before.len() / 3];
    {
        let spot_ = ed.view.doc_to_screen(mid);
        double_tap(&mut ed, spot_, 70_000.0);
    }
    let f = figure_of(&ed);
    assert_eq!(f.anchor_count(), 3);
    assert_eq!(ed.selected_anchor(), Some(1), "the new anchor is the selected one");
    let still_on = |p: [f32; 2]| {
        before.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1])).clamp(0.0, 1.0);
            ((a[0] + ab[0] * t - p[0]).powi(2) + (a[1] + ab[1] * t - p[1]).powi(2)).sqrt() < 0.7
        })
    };
    assert!(f.points().iter().all(|&p| still_on(p)), "the shape did not change");
    ed.undo();
    assert_eq!(figure_of(&ed).anchor_count(), 2);
}

// ── rectangles and ellipses ──────────────────────────────────────────────────────────────────

#[test]
fn a_rectangles_anchors_are_the_ends_of_its_diagonal_and_it_stays_a_rectangle() {
    let mut ed = shaping(Shape::Rectangle);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    assert_eq!(figure(&ed).anchor_count(), 2);
    let end = at(&ed, Control::Anchor(1));
    let to = [end[0] + 40.0, end[1] + 25.0];
    drag_at(&mut ed, end, to, 20_000.0);
    let f = figure_of(&ed);
    assert!(near(ed.view.doc_to_screen(f.anchor_pos(1).expect("end")), to));
    for w in ed.ink.strokes()[0].pts.windows(2) {
        assert!(w[0][0] == w[1][0] || w[0][1] == w[1][1], "every edge is horizontal or vertical: {w:?}");
    }
    assert_eq!(ed.ink.strokes()[0].pts, f.points());
    // Nothing else can be done to its anchors: no double-tap conversion, no node added or taken away.
    let start = at(&ed, Control::Anchor(0));
    double_tap(&mut ed, start, 30_000.0);
    {
        let spot_ = at(&ed, Control::Anchor(1));
        barrel_tap(&mut ed, spot_, 40_000.0);
    }
    let spot = on_page(&ed, 200.0, 200.0);
    double_tap(&mut ed, spot, 50_000.0);
    assert_eq!(figure_of(&ed), f, "{:?}", figure_of(&ed));
    assert_eq!(ed.selection.len(), 1);
}

#[test]
fn an_ellipses_anchors_are_the_corners_of_its_bounding_rectangle() {
    let mut ed = shaping(Shape::Ellipse);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    assert_eq!(figure(&ed).anchor_count(), 4);
    let before = figure_of(&ed);
    let top_left = before.anchor_pos(0).expect("corner");
    let bottom_right = before.anchor_pos(2).expect("corner");
    // Pull the bottom right corner out: the top left stays, the ellipse grows to fill the box.
    let corner = at(&ed, Control::Anchor(2));
    let to = [corner[0] + 60.0, corner[1] + 40.0];
    drag_at(&mut ed, corner, to, 20_000.0);
    let after = figure_of(&ed);
    assert_eq!(after.anchor_pos(0), Some(top_left));
    assert!(near(ed.view.doc_to_screen(after.anchor_pos(2).expect("corner")), to));
    let pts = &ed.ink.strokes()[0].pts;
    let b = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
    let want = after.anchor_pos(2).expect("corner");
    assert!(
        (b[0] - top_left[0]).abs() < 0.1 && (b[1] - top_left[1]).abs() < 0.1 && (b[2] - want[0]).abs() < 0.1 && (b[3] - want[1]).abs() < 0.1,
        "{b:?}"
    );
    assert_ne!(after.anchor_pos(2), Some(bottom_right));
    // No handles to pull, nothing to convert or add.
    let any = at(&ed, Control::Anchor(1));
    double_tap(&mut ed, any, 30_000.0);
    assert_eq!(figure_of(&ed), after);
}

// ── axes ──────────────────────────────────────────────────────────────────────────────────────

#[test]
fn an_axis_tip_slides_along_its_axis_and_the_crossing_carries_the_lines_with_it() {
    let mut ed = shaping(Shape::Axes2d);
    draw(&mut ed, [100.0, 200.0], [400.0, 400.0]);
    let before = figure_of(&ed);
    let tip = at(&ed, Control::Anchor(2));
    // Dragged diagonally, the tip only goes as far along the x axis as the drag went.
    let to = [tip[0] - 40.0, tip[1] - 30.0];
    drag_at(&mut ed, tip, to, 170_000.0);
    let moved = figure_of(&ed);
    let (old_tip, new_tip) = (before.anchor_pos(2).expect("tip"), moved.anchor_pos(2).expect("tip"));
    assert_eq!(new_tip[1], old_tip[1], "still on the x axis");
    assert!((new_tip[0] - (old_tip[0] - 40.0)).abs() < 0.1);
    assert_eq!(moved.anchor_pos(1), before.anchor_pos(1), "the tail stayed");
    // Moving the crossing moves each line to pass through it, the ends keeping their places along them.
    let cross = at(&ed, Control::Anchor(0));
    drag_at(&mut ed, cross, [cross[0] + 30.0, cross[1] + 20.0], 180_000.0);
    let all = figure_of(&ed);
    let origin = all.anchor_pos(0).expect("origin");
    assert!(near(ed.view.doc_to_screen(origin), [cross[0] + 30.0, cross[1] + 20.0]));
    assert_eq!(all.anchor_pos(1).expect("tail")[1], origin[1], "the x axis passes through the crossing");
    assert_eq!(all.anchor_pos(1).expect("tail")[0], moved.anchor_pos(1).expect("tail")[0], "and its tail kept its place along it");
    assert_eq!(all.anchor_pos(3).expect("tail")[0], origin[0], "the y axis too");
    // They are still perpendicular, however it was done.
    let (x, y) = (
        [all.anchor_pos(2).expect("a")[0] - all.anchor_pos(1).expect("a")[0], all.anchor_pos(2).expect("a")[1] - all.anchor_pos(1).expect("a")[1]],
        [all.anchor_pos(4).expect("a")[0] - all.anchor_pos(3).expect("a")[0], all.anchor_pos(4).expect("a")[1] - all.anchor_pos(3).expect("a")[1]],
    );
    assert!((x[0] * y[0] + x[1] * y[1]).abs() < 1e-3);
}

// ── selecting and moving ─────────────────────────────────────────────────────────────────────

#[test]
fn a_tap_on_a_shapes_outline_selects_it_and_a_tap_elsewhere_lets_go() {
    let mut ed = shaping(Shape::Rectangle);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let first = ed.selection[0];
    // Draw a second one away from it: it takes over the selection.
    draw(&mut ed, [100.0, 500.0], [300.0, 600.0]);
    assert_ne!(ed.selection, vec![first]);
    // Tap the first one's outline.
    let on_edge = on_page(&ed, 200.0, 200.0);
    pen_at(&mut ed, Phase::Down, on_edge, 130_000.0);
    pen_at(&mut ed, Phase::Up, on_edge, 130_030.0);
    assert_eq!(ed.selection, vec![first]);
    assert!(ed.editing().is_some(), "its anchors show");
    // Tap on empty page.
    let empty = on_page(&ed, 450.0, 700.0);
    pen_at(&mut ed, Phase::Down, empty, 140_000.0);
    pen_at(&mut ed, Phase::Up, empty, 140_030.0);
    assert!(ed.selection.is_empty() && ed.editing().is_none());
    assert_eq!(ed.ink.len(), 2, "taps drew nothing");
}

#[test]
fn dragging_a_selected_shape_by_its_outline_moves_it_with_its_anchors() {
    let mut ed = shaping(Shape::Ellipse);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = figure_of(&ed);
    // On the outline halfway between two anchors: the arc at 45 degrees.
    let grab = on_page(&ed, 270.7, 302.4);
    let to = [grab[0] + 60.0, grab[1] + 40.0];
    drag_at(&mut ed, grab, to, 150_000.0);
    let after = figure_of(&ed);
    assert_eq!(ed.ink.len(), 1);
    for i in 0..before.anchor_count() {
        let (a, b) = (before.anchor_pos(i).expect("a"), after.anchor_pos(i).expect("b"));
        assert!(
            ((b[0] - a[0]) - 60.0 / ed.view.zoom).abs() < 0.05 && ((b[1] - a[1]) - 40.0 / ed.view.zoom).abs() < 0.05,
            "anchor {i} moved by the drag"
        );
    }
    assert_eq!(ed.ink.strokes()[0].pts.len(), after.points().len());
    ed.undo();
    assert_eq!(figure_of(&ed).points(), before.points());
}

#[test]
fn a_press_inside_a_selected_shape_but_off_its_outline_starts_a_new_shape() {
    let mut ed = shaping(Shape::Rectangle);
    draw(&mut ed, [100.0, 200.0], [400.0, 500.0]);
    draw(&mut ed, [200.0, 300.0], [300.0, 400.0]);
    assert_eq!(ed.ink.len(), 2, "a shape drawn inside another");
    assert_eq!(ed.selection.len(), 1);
}

#[test]
fn anchors_and_handles_stay_on_the_page() {
    for shape in [Shape::Line, Shape::Curve, Shape::Rectangle, Shape::Ellipse, Shape::Axes2d] {
        let mut ed = shaping(shape);
        draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
        let a = at(&ed, Control::Anchor(1));
        drag_at(&mut ed, a, [a[0] - 9000.0, a[1] - 9000.0], 160_000.0);
        let f = figure_of(&ed);
        let page = ed.layout.page(0).expect("page");
        for p in nodes(&f) {
            assert!(p[0] >= page.x - 0.01 && p[1] >= page.y - 0.01, "{shape:?}: {p:?} is on the page");
        }
        assert!(f.valid());
    }
}

#[test]
fn the_mouse_cannot_touch_anchors_in_the_tablet_mode() {
    let mut ed = shaping(Shape::Curve);
    ed.set_mode(Mode::Tablet);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = figure_of(&ed);
    let handle = at(&ed, Control::Fwd(0));
    ed.mouse_moved(handle);
    ed.mouse_button(ButtonState::Pressed, MouseButton::Left);
    ed.mouse_moved([handle[0] + 25.0, handle[1] - 12.0]);
    ed.mouse_button(ButtonState::Released, MouseButton::Left);
    assert_eq!(figure_of(&ed), before);
    assert!(ed.is_idle());
}

#[test]
fn the_pen_tool_does_not_edit_anchors_only_the_shape_tool_does() {
    let mut ed = shaping(Shape::Rectangle);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = figure_of(&ed);
    let corner = at(&ed, Control::Anchor(1));
    ed.set_tool(Tool::Pen);
    ed.selection = vec![ed.ink.strokes()[0].id];
    // With the pen tool a press on a selected shape moves the selection instead of reshaping it.
    drag_at(&mut ed, corner, [corner[0] + 30.0, corner[1] + 20.0], 300_000.0);
    let after = figure_of(&ed);
    for i in 0..2 {
        let (a, b) = (before.anchor_pos(i).expect("a"), after.anchor_pos(i).expect("b"));
        assert!(((b[0] - a[0]) - 30.0).abs() < 0.1 && ((b[1] - a[1]) - 20.0).abs() < 0.1, "every anchor moved together: {i}");
    }
}

#[test]
fn the_laser_and_a_finger_leave_anchors_alone() {
    let mut ed = shaping(Shape::Rectangle);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let before = figure_of(&ed);
    let corner = at(&ed, Control::Anchor(1));
    // A finger drag over an anchor pans.
    ed.pointer_event(touch(1, Phase::Down, corner, 30.0));
    ed.pointer_event(touch(1, Phase::Move, [corner[0], corner[1] - 80.0], 30.0));
    ed.pointer_event(touch(1, Phase::Up, [corner[0], corner[1] - 80.0], 30.0));
    assert_eq!(figure_of(&ed), before);
    // The laser draws no shape and edits none.
    ed.set_tool(LASER);
    pen_at(&mut ed, Phase::Down, corner, 200_000.0);
    pen_at(&mut ed, Phase::Move, [corner[0] + 50.0, corner[1] + 50.0], 200_010.0);
    pen_at(&mut ed, Phase::Up, [corner[0] + 50.0, corner[1] + 50.0], 200_020.0);
    assert_eq!(ed.ink.strokes()[0].figure.as_ref(), Some(&before));
}

#[test]
fn shapes_from_other_tools_selection_also_show_anchors_and_plain_ink_does_not() {
    let mut ed = editor();
    let plain = add_stroke(&mut ed, &[[100.0, 300.0], [200.0, 320.0], [300.0, 300.0]]);
    ed.selection = vec![plain];
    assert!(ed.editing().is_none(), "handwriting has no anchors");
    // A shape drawn, then selected again with a tap of the stylus, shows its anchors.
    ed.set_tool(Tool::Geometry);
    ed.shape = Shape::Rectangle;
    draw(&mut ed, [100.0, 500.0], [300.0, 600.0]);
    ed.selection.clear();
    ed.set_tool(Tool::Geometry);
    let on_edge = on_page(&ed, 200.0, 500.0);
    pen_at(&mut ed, Phase::Down, on_edge, 400_000.0);
    pen_at(&mut ed, Phase::Up, on_edge, 400_030.0);
    assert!(ed.editing().is_some());
}

#[test]
fn a_figure_with_a_stale_selection_never_edits_the_wrong_thing() {
    let mut ed = shaping(Shape::Line);
    draw(&mut ed, [100.0, 200.0], [300.0, 320.0]);
    let p = at(&ed, Control::Anchor(0));
    pen_at(&mut ed, Phase::Down, p, 210_000.0);
    pen_at(&mut ed, Phase::Up, p, 210_020.0);
    assert_eq!(ed.selected_anchor(), Some(0));
    // The shape is erased: its anchor selection means nothing now.
    ed.undo();
    assert_eq!(ed.selected_anchor(), None);
    assert!(ed.editing().is_none());
    // A press where its anchor was does nothing.
    pen_at(&mut ed, Phase::Down, p, 220_000.0);
    pen_at(&mut ed, Phase::Up, p, 220_020.0);
    assert_eq!(ed.ink.len(), 0);
}
