use super::*;
use crate::geometry::SHAPES;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    fn point(&mut self, extent: f32) -> P {
        [self.range(-extent, extent), self.range(-extent, extent)]
    }
}

fn near(a: P, b: P) -> bool {
    length(sub(a, b)) < 1e-2
}

fn extent(pts: &[P]) -> [f32; 4] {
    pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

fn line() -> Figure {
    Figure::from_shape(Shape::Line, [0.0, 0.0], [100.0, 0.0]).expect("line")
}

fn rect() -> Figure {
    Figure::from_shape(Shape::Rectangle, [10.0, 20.0], [110.0, 70.0]).expect("rectangle")
}

fn ellipse() -> Figure {
    Figure::from_shape(Shape::Ellipse, [0.0, 0.0], [100.0, 60.0]).expect("ellipse")
}

fn curve() -> Figure {
    Figure::from_shape(Shape::Curve, [0.0, 0.0], [90.0, 0.0]).expect("curve")
}

fn axes(shape: Shape) -> Figure {
    Figure::from_shape(shape, [0.0, 0.0], [200.0, 100.0]).expect("axes")
}

fn poly(points: &[P], closed: bool) -> Figure {
    Figure::Poly { points: points.to_vec(), closed }
}

fn spline_anchors(f: &Figure) -> Vec<Anchor> {
    match f {
        Figure::Path { anchors, .. } => anchors.clone(),
        other => panic!("not a spline: {other:?}"),
    }
}

// ── what each kind of shape is made of ───────────────────────────────────────────────────────

#[test]
fn each_shape_is_the_kind_of_figure_with_the_anchors_the_user_expects() {
    assert!(matches!(line(), Figure::Poly { closed: false, .. }));
    assert_eq!(line().anchor_count(), 2);
    assert_eq!(curve().anchor_count(), 2, "a spline's endpoints (and their tangents)");
    assert!(matches!(curve(), Figure::Path { closed: false, .. }));
    assert!(matches!(rect(), Figure::Rect { .. }));
    assert_eq!(rect().anchor_count(), 2, "a rectangle's anchors are its diagonal");
    assert!(matches!(ellipse(), Figure::Ellipse { .. }));
    assert_eq!(ellipse().anchor_count(), 4, "an ellipse's are the corners of its bounding rectangle");
    assert_eq!(axes(Shape::Axes2d).anchor_count(), 5, "the origin and the ends of two lines");
    assert_eq!(axes(Shape::Axes3d).anchor_count(), 7, "the origin and the ends of three lines");
}

#[test]
fn a_rectangle_is_its_diagonal_and_draws_a_closed_box() {
    let f = rect();
    assert_eq!(f.points(), vec![[10.0, 20.0], [110.0, 20.0], [110.0, 70.0], [10.0, 70.0], [10.0, 20.0]]);
    assert_eq!(f.controls(), vec![(Control::Anchor(0), [10.0, 20.0]), (Control::Anchor(1), [110.0, 70.0])], "no handles");
    assert_eq!(f.guides(), vec![[[10.0, 20.0], [110.0, 70.0]]], "the diagonal is shown");
}

#[test]
fn an_ellipse_fills_its_bounding_box_is_round_and_has_only_the_boxs_corners() {
    let f = ellipse();
    let pts = f.points();
    let b = extent(&pts);
    assert!(b[0].abs() < 0.05 && (b[2] - 100.0).abs() < 0.05 && b[1].abs() < 0.05 && (b[3] - 60.0).abs() < 0.05, "{b:?}");
    for p in &pts {
        let k = ((p[0] - 50.0) / 50.0).powi(2) + ((p[1] - 30.0) / 30.0).powi(2);
        assert!((k - 1.0).abs() < 0.01, "{p:?} is off the ellipse by {}", k - 1.0);
    }
    let corners: Vec<P> = f.controls().iter().map(|c| c.1).collect();
    assert_eq!(corners, vec![[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]], "clockwise from the top left, no handles");
    assert_eq!(f.guides().len(), 4, "the bounding rectangle is shown");
    assert!(pts.len() > 40 && pts.len() < 600);
}

#[test]
fn a_spline_has_two_endpoints_with_tangents_off_the_chord() {
    let f = curve();
    let anchors = spline_anchors(&f);
    assert!(anchors.iter().all(|a| a.smooth));
    assert!(anchors[0].fwd[1] > 0.0 && anchors[1].back[1] < 0.0, "the S: the handles are on opposite sides of the chord");
    let pts = f.points();
    assert_eq!(pts[0], [0.0, 0.0]);
    assert!(near(*pts.last().expect("end"), [90.0, 0.0]));
    let b = extent(&pts);
    assert!(b[1] < -1.0 && b[3] > 1.0, "it swings both ways: {b:?}");
    assert_eq!(f.controls().len(), 2 + 2, "two anchors and a handle each (an end has one)");
}

#[test]
fn axes_have_the_crossing_and_the_ends_as_anchors() {
    let f = axes(Shape::Axes2d);
    assert_eq!(f.anchor_pos(0), Some([100.0, 50.0]));
    assert_eq!(f.anchor_pos(1), Some([0.0, 50.0]), "the x axis' tail");
    assert_eq!(f.anchor_pos(2), Some([200.0, 50.0]), "its tip");
    assert_eq!(f.anchor_pos(3), Some([100.0, 100.0]));
    assert_eq!(f.anchor_pos(4), Some([100.0, 0.0]), "the y axis points up");
    assert_eq!(f.anchor_pos(5), None);
    assert_eq!(extent(&f.points()), [0.0, 0.0, 200.0, 100.0]);
    assert_eq!(f.controls().len(), 5);
}

// ── straight lines: nodes, broken lines, polygons ────────────────────────────────────────────

#[test]
fn straight_lines_stay_straight_whatever_is_done_to_their_nodes() {
    let mut f = line();
    // Add a node in the middle, then drag it off the line by a little: still only straight segments.
    let on = f.nearest_on_path([50.0, 3.0]).expect("on the line");
    let i = f.insert_anchor(on.segment, on.t).expect("a node");
    assert_eq!(f.points(), vec![[0.0, 0.0], [50.0, 0.0], [100.0, 0.0]]);
    f.move_control(Control::Anchor(i), [50.0, 6.0], false);
    assert_eq!(f.points(), vec![[0.0, 0.0], [50.0, 6.0], [100.0, 0.0]], "exactly the nodes: nothing between them to bend");
    // Smooth does not apply to straight lines; nor do handles.
    assert!(!f.toggle_smooth(1));
    assert_eq!(f.controls().len(), 3, "no handles");
    f.move_control(Control::Fwd(1), [60.0, 60.0], true);
    assert_eq!(f.points(), vec![[0.0, 0.0], [50.0, 6.0], [100.0, 0.0]]);
}

#[test]
fn a_broken_line_has_a_node_for_every_corner_and_polygons_close_on_the_first() {
    let broken = poly(&[[0.0, 0.0], [50.0, 40.0], [100.0, 0.0]], false);
    assert_eq!(broken.anchor_count(), 3);
    assert_eq!(broken.points().len(), 3);
    let polygon = poly(&[[0.0, 0.0], [50.0, 40.0], [100.0, 0.0]], true);
    assert_eq!(polygon.anchor_count(), 3, "a closed polygon has one node per corner");
    assert_eq!(polygon.points(), vec![[0.0, 0.0], [50.0, 40.0], [100.0, 0.0], [0.0, 0.0]], "and its outline comes back to the first");
}

#[test]
fn a_line_is_extended_at_either_end() {
    let mut f = line();
    // At the end: a third node there.
    assert_eq!(f.extend([150.0, 30.0], false), Some(2));
    // At the start: the new node goes first.
    assert_eq!(f.extend([-40.0, 30.0], true), Some(0));
    assert_eq!(f.anchor_pos(0), Some([-40.0, 30.0]));
    assert_eq!(f.anchor_count(), 4);
    // A closed polygon and other shapes cannot be extended.
    let mut h = poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0]], true);
    assert_eq!(h.extend([500.0, 500.0], false), None);
    assert_eq!(rect().extend([1.0, 1.0], false), None);
    assert_eq!(line().extend([f32::NAN, 1.0], false), None);
}

#[test]
fn bringing_the_ends_of_a_broken_line_together_makes_a_polygon() {
    let mut f = poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]], false);
    f.move_control(Control::Anchor(3), [0.0, 0.0], false);
    assert!(f.merge_ends());
    assert_eq!(f, poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0]], true), "the two end nodes became one");
    // Ends that are not together stay open; a triangle's worth cannot lose a node.
    let mut g = poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 40.0]], false);
    assert!(!g.merge_ends());
    let mut t = poly(&[[0.0, 0.0], [100.0, 0.0], [0.0, 0.0]], false);
    assert!(!t.merge_ends(), "two nodes of a polygon is not a polygon");
    assert!(!rect().merge_ends());
}

#[test]
fn a_node_can_close_a_run_of_lines_or_open_a_polygon() {
    let mut f = poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0]], false);
    assert!(!f.toggle_closed(1), "only an end node can close it");
    assert!(f.toggle_closed(2));
    assert_eq!(f, poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0]], true));
    // Opening at node 1: it runs round from there and ends where it began.
    assert!(f.toggle_closed(1));
    assert_eq!(f, poly(&[[100.0, 0.0], [100.0, 60.0], [0.0, 0.0], [100.0, 0.0]], false));
    assert!(f.merge_ends() && f == poly(&[[100.0, 0.0], [100.0, 60.0], [0.0, 0.0]], true), "and closes again as it was");
    assert!(!line().toggle_closed(0), "two nodes cannot make a polygon");
    assert!(!curve().toggle_closed(0) && !rect().toggle_closed(0));
}

#[test]
fn nodes_are_added_on_a_segment_and_taken_away_while_enough_are_left() {
    let mut f = poly(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0]], true);
    let before = f.points();
    let on = f.nearest_on_path([50.0, -4.0]).expect("on");
    assert_eq!((on.segment, on.at), (0, [50.0, 0.0]));
    assert_eq!(f.insert_anchor(on.segment, on.t), Some(1));
    assert_eq!(f.anchor_count(), 4);
    assert_eq!(extent(&f.points()), extent(&before), "the polygon did not change");
    // The closing segment can be split too.
    let on = f.nearest_on_path([40.0, 30.0]).expect("closing edge");
    assert_eq!(on.segment, 3);
    assert_eq!(f.insert_anchor(on.segment, on.t), Some(4));
    assert!(f.remove_anchor(4) && f.remove_anchor(1));
    assert!(!f.remove_anchor(0), "a polygon keeps three");
    let mut open = poly(&[[0.0, 0.0], [5.0, 0.0], [9.0, 4.0]], false);
    assert!(open.remove_anchor(1));
    assert_eq!(open.points(), vec![[0.0, 0.0], [9.0, 4.0]]);
    assert!(!open.remove_anchor(0), "a line keeps two");
    assert!(!open.remove_anchor(7));
    assert_eq!(poly(&[[0.0; 2], [1.0, 1.0]], false).insert_anchor(1, 0.5), None, "an open two-node line has one segment");
}

// ── splines: endpoints, tangents, node positions and tangents ───────────────────────────────

#[test]
fn dragging_a_spline_anchor_takes_its_tangents_with_it() {
    let mut f = curve();
    let before = f.control_pos(Control::Fwd(0)).expect("handle");
    f.move_control(Control::Anchor(0), [60.0, -10.0], false);
    assert_eq!(f.control_pos(Control::Anchor(0)), Some([60.0, -10.0]));
    let after = f.control_pos(Control::Fwd(0)).expect("handle");
    assert!(near(sub(after, [60.0, -10.0]), sub(before, [0.0, 0.0])), "the handle kept its offset");
}

#[test]
fn a_smooth_anchors_handles_stay_in_line_and_keep_their_own_lengths() {
    let mut f = Figure::from_shape(Shape::Curve, [0.0, 0.0], [90.0, 0.0]).expect("curve");
    f.insert_anchor(0, 0.5);
    let anchors = spline_anchors(&f);
    let (pos, back_len) = (anchors[1].pos, length(anchors[1].back));
    f.move_control(Control::Fwd(1), add(pos, [20.0, 40.0]), false);
    let anchors = spline_anchors(&f);
    let (fwd, back) = (anchors[1].fwd, anchors[1].back);
    assert!(near(fwd, [20.0, 40.0]));
    assert!((length(back) - back_len).abs() < 1e-3, "the other handle keeps its length");
    assert!((fwd[0] * back[1] - fwd[1] * back[0]).abs() < 1e-2, "and points the opposite way: {fwd:?} {back:?}");
    assert!(dot(fwd, back) < 0.0 && anchors[1].smooth);
}

#[test]
fn moving_a_handle_with_the_independent_key_breaks_the_anchor_into_a_corner() {
    let mut f = curve();
    f.insert_anchor(0, 0.5);
    let anchors = spline_anchors(&f);
    let (pos, back) = (anchors[1].pos, anchors[1].back);
    f.move_control(Control::Fwd(1), add(pos, [30.0, 30.0]), true);
    let anchors = spline_anchors(&f);
    assert_eq!(anchors[1].back, back, "the other handle did not move");
    assert!(!anchors[1].smooth);
    f.move_control(Control::Back(1), add(pos, [-5.0, 5.0]), false);
    assert_eq!(spline_anchors(&f)[1].fwd, [30.0, 30.0], "from now on the handles move alone");
}

#[test]
fn a_handle_pulled_out_of_a_smooth_anchor_with_no_other_handle_is_mirrored() {
    let mut f = Figure::Path { anchors: vec![Anchor { smooth: true, ..Anchor::corner([0.0, 0.0]) }, Anchor::corner([100.0, 0.0])], closed: false };
    f.move_control(Control::Fwd(0), [30.0, 10.0], false);
    assert_eq!(spline_anchors(&f)[0].back, [-30.0, -10.0]);
}

#[test]
fn a_corner_becomes_smooth_and_a_smooth_anchor_a_corner() {
    let mut f = Figure::Path { anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([30.0, 0.0]), Anchor::corner([60.0, 30.0])], closed: false };
    assert!(f.toggle_smooth(1));
    let anchors = spline_anchors(&f);
    assert!(anchors[1].smooth && anchors[1].fwd[0] > 0.0 && anchors[1].back[0] < 0.0, "{:?}", anchors[1]);
    assert!((anchors[1].fwd[0] * anchors[1].back[1] - anchors[1].fwd[1] * anchors[1].back[0]).abs() < 1e-3);
    assert!(f.toggle_smooth(1));
    assert_eq!(spline_anchors(&f)[1], Anchor::corner([30.0, 0.0]), "back to a corner");
    assert!(f.toggle_smooth(0));
    let anchors = spline_anchors(&f);
    assert_eq!(anchors[0].back, [0.0, 0.0]);
    assert!(anchors[0].fwd[0] > 0.0, "an end gets one handle, towards its neighbour");
    assert!(!f.toggle_smooth(9));
    for other in [line(), rect(), ellipse(), axes(Shape::Axes2d)] {
        assert!(!other.clone().toggle_smooth(1), "{other:?} has no smooth points");
    }
}

#[test]
fn a_spline_anchor_inserted_on_a_curve_is_smooth_and_the_shape_does_not_change() {
    let mut f = curve();
    let before = f.points();
    let on = f.nearest_on_path([20.0, 10.0]).expect("on");
    let i = f.insert_anchor(on.segment, on.t).expect("inserted");
    assert!(spline_anchors(&f)[i].smooth);
    assert!(length(sub(f.anchor_pos(i).expect("new"), on.at)) < 0.6);
    for p in f.points() {
        let d = before.windows(2).map(|w| dist_to_segment(p, w[0], w[1])).fold(f32::MAX, f32::min);
        assert!(d < 0.6, "{p:?} left the shape by {d}");
    }
    let mut corners = Figure::Path { anchors: vec![Anchor::corner([0.0; 2]), Anchor::corner([100.0, 0.0])], closed: false };
    let i = corners.insert_anchor(0, 0.5).expect("inserted");
    assert_eq!(spline_anchors(&corners)[i], Anchor::corner([50.0, 0.0]), "on a straight spline segment a corner");
}

fn dist_to_segment(p: P, a: P, b: P) -> f32 {
    let (_, on) = nearest_on_segment(a, b, p);
    length(sub(p, on))
}

// ── rectangles, ellipses: diagonal and bounding box ──────────────────────────────────────────

#[test]
fn a_rectangle_stays_a_rectangle_whichever_end_of_the_diagonal_is_dragged() {
    let mut f = rect();
    f.move_control(Control::Anchor(1), [200.0, 150.0], false);
    assert_eq!(f.points(), vec![[10.0, 20.0], [200.0, 20.0], [200.0, 150.0], [10.0, 150.0], [10.0, 20.0]]);
    f.move_control(Control::Anchor(0), [250.0, 100.0], false);
    // Dragged past the other end it is the same rectangle, the other way round.
    assert_eq!(extent(&f.points()), [200.0, 100.0, 250.0, 150.0]);
    for w in f.points().windows(2) {
        assert!(w[0][0] == w[1][0] || w[0][1] == w[1][1], "every edge is horizontal or vertical: {w:?}");
    }
    assert!(!f.toggle_smooth(0) && !f.remove_anchor(0) && f.insert_anchor(0, 0.5).is_none() && !f.toggle_closed(0));
    assert_eq!(f.anchor_count(), 2);
}

#[test]
fn an_ellipse_corner_moves_with_the_opposite_corner_fixed_and_it_stays_an_ellipse() {
    for corner_index in 0..4 {
        let mut f = ellipse();
        let opposite = f.anchor_pos((corner_index + 2) % 4).expect("opposite");
        let to = [if corner_index == 0 || corner_index == 3 { -30.0 } else { 160.0 }, if corner_index < 2 { -20.0 } else { 100.0 }];
        f.move_control(Control::Anchor(corner_index), to, false);
        let pts = f.points();
        let b = extent(&pts);
        let (lo, hi) = ([to[0].min(opposite[0]), to[1].min(opposite[1])], [to[0].max(opposite[0]), to[1].max(opposite[1])]);
        assert!(near([b[0], b[1]], lo) && near([b[2], b[3]], hi), "corner {corner_index}: {b:?} vs {lo:?} {hi:?}");
        assert_eq!(f.anchor_count(), 4);
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let (rx, ry) = ((hi[0] - lo[0]) / 2.0, (hi[1] - lo[1]) / 2.0);
        for p in &pts {
            let k = ((p[0] - centre[0]) / rx).powi(2) + ((p[1] - centre[1]) / ry).powi(2);
            assert!((k - 1.0).abs() < 0.01, "corner {corner_index}: {p:?} is off the ellipse");
        }
    }
    let mut f = ellipse();
    assert!(!f.toggle_smooth(0) && !f.remove_anchor(0) && f.insert_anchor(0, 0.5).is_none() && f.nearest_on_path([1.0, 1.0]).is_none());
    // Dragged past the opposite corner it flips: still the same ellipse, drawn the other way round.
    f.move_control(Control::Anchor(0), [150.0, 90.0], false);
    assert_eq!(f.anchor_pos(0), Some([100.0, 60.0]), "the corner at the top left is whichever is");
}

// ── axes ──────────────────────────────────────────────────────────────────────────────────────

/// Whether every axis runs the way it did, and each is at right angles to the others (for 2D).
fn perpendicular(f: &Figure) -> bool {
    let Figure::Axes { ends, .. } = f else { return false };
    let dirs: Vec<P> = ends.iter().filter_map(|e| unit(sub(e[1], e[0]))).collect();
    dirs.len() == ends.len() && dot(dirs[0], dirs[1]).abs() < 1e-3
}

#[test]
fn an_axis_end_slides_along_its_axis_and_the_axes_stay_perpendicular() {
    let mut f = axes(Shape::Axes2d);
    // Pull the x axis' tip somewhere off the axis: it only goes as far along the axis as that is.
    f.move_control(Control::Anchor(2), [160.0, 90.0], false);
    assert_eq!(f.anchor_pos(2), Some([160.0, 50.0]), "the tip stayed on the x axis");
    // Pull the y axis' tail: it slides along the vertical.
    f.move_control(Control::Anchor(3), [140.0, 130.0], false);
    assert_eq!(f.anchor_pos(3), Some([100.0, 130.0]));
    assert!(perpendicular(&f));
    // An end can go through the origin to the other side.
    f.move_control(Control::Anchor(1), [130.0, 77.0], false);
    assert_eq!(f.anchor_pos(1), Some([130.0, 50.0]));
    assert!(perpendicular(&f));
}

#[test]
fn moving_where_axes_cross_moves_the_lines_and_each_end_keeps_its_place_along_its_line() {
    let mut f = axes(Shape::Axes2d);
    f.move_control(Control::Anchor(0), [60.0, 80.0], false);
    assert_eq!(f.anchor_pos(0), Some([60.0, 80.0]));
    // The x axis moved down to pass through the new crossing; its ends kept their x.
    assert_eq!((f.anchor_pos(1), f.anchor_pos(2)), (Some([0.0, 80.0]), Some([200.0, 80.0])));
    // The y axis moved left; its ends kept their y.
    assert_eq!((f.anchor_pos(3), f.anchor_pos(4)), (Some([60.0, 100.0]), Some([60.0, 0.0])));
    assert!(perpendicular(&f));
    // Three axes: each stays on its own line through the crossing, whichever way it is dragged.
    let mut g = axes(Shape::Axes3d);
    let dirs = |g: &Figure| {
        let Figure::Axes { ends, .. } = g else { panic!() };
        ends.iter().map(|e| unit(sub(e[1], e[0])).expect("a direction")).collect::<Vec<P>>()
    };
    let before = dirs(&g);
    g.move_control(Control::Anchor(0), [30.0, 70.0], false);
    for (a, b) in before.iter().zip(dirs(&g)) {
        assert!(near(*a, b), "the axes still run the way they did: {a:?} {b:?}");
    }
    let Figure::Axes { origin, ends } = &g else { panic!() };
    assert!(
        ends.iter().all(|e| {
            let cross = (e[1][0] - origin[0]) * (e[0][1] - origin[1]) - (e[1][1] - origin[1]) * (e[0][0] - origin[0]);
            cross.abs() < 1e-2
        }),
        "every axis passes through the crossing"
    );
}

#[test]
fn an_axis_shrunk_almost_to_nothing_and_dragged_about_stays_on_its_line_through_the_crossing() {
    // From a stress failure: the z axis' tail and tip a point apart, far from the crossing.
    let mut f = Figure::Axes {
        origin: [133.5078, 944.4453],
        ends: vec![[[19.7, 944.4453], [247.3, 944.4453]], [[54.63286, 998.81433], [53.84359, 999.357]]],
    };
    for k in 0..400 {
        let to = [100.0 + (k as f32 * 7.3).sin() * 80.0, 900.0 + (k as f32 * 3.1).cos() * 90.0];
        f.move_control(Control::Anchor(if k % 3 == 0 { 0 } else { 3 + k % 2 }), to, false);
        let Figure::Axes { origin, ends } = &f else { panic!() };
        let e = ends[1];
        let d = sub(e[1], e[0]);
        let len = length(d).max(1e-6);
        let off = ((e[0][0] - origin[0]) * d[1] - (e[0][1] - origin[1]) * d[0]) / len;
        assert!(off.abs() < 0.02 || len < 1e-3, "step {k}: the axis is {off} from the crossing");
    }
}

#[test]
fn axes_have_no_handles_and_no_nodes_to_add_or_take_away() {
    let mut f = axes(Shape::Axes3d);
    let before = f.clone();
    f.move_control(Control::Fwd(1), [0.0, 0.0], false);
    assert_eq!(f, before);
    assert!(!f.remove_anchor(1) && f.insert_anchor(0, 0.5).is_none() && !f.toggle_smooth(1) && f.nearest_on_path([1.0, 1.0]).is_none());
}

// ── all of them ──────────────────────────────────────────────────────────────────────────────

#[test]
fn the_control_pressed_is_the_nearest_and_a_handle_beats_an_anchor_it_lies_on() {
    let f = curve();
    assert_eq!(f.control_near([0.5, 0.5], 8.0), Some(Control::Anchor(0)));
    let handle = f.control_pos(Control::Fwd(0)).expect("handle");
    assert_eq!(f.control_near(add(handle, [1.0, 1.0]), 8.0), Some(Control::Fwd(0)));
    assert_eq!(f.control_near([500.0, 500.0], 8.0), None);
    let mut g = Figure::Path { anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([50.0, 0.0])], closed: false };
    g.move_control(Control::Anchor(0), [0.0, 0.0], false);
    assert_eq!(g.control_near([1.0, 0.0], 5.0), Some(Control::Anchor(0)));
}

#[test]
fn the_nearest_point_on_a_path_is_found_with_its_segment() {
    let r = poly(&[[10.0, 20.0], [110.0, 20.0], [110.0, 70.0], [10.0, 70.0]], true);
    let on = r.nearest_on_path([60.0, 14.0]).expect("nearest");
    assert_eq!(on.segment, 0);
    assert!((on.t - 0.5).abs() < 1e-3 && near(on.at, [60.0, 20.0]) && (on.distance - 6.0).abs() < 1e-3);
    assert_eq!(r.nearest_on_path([5.0, 45.0]).expect("nearest").segment, 3, "the closing edge");
    assert!(axes(Shape::Axes2d).nearest_on_path([1.0, 1.0]).is_none());
}

#[test]
fn translating_moves_everything() {
    let all = [line(), poly(&[[0.0; 2], [9.0, 3.0], [4.0, 8.0]], true), rect(), ellipse(), curve(), axes(Shape::Axes3d)];
    for mut f in all {
        let before = f.points();
        f.translate(7.0, -3.0);
        assert!(f.points().iter().zip(&before).all(|(a, b)| near(sub(*a, *b), [7.0, -3.0])));
    }
}

fn kinds() -> Vec<Figure> {
    let mut shaped = curve();
    shaped.insert_anchor(0, 0.5);
    shaped.move_control(Control::Fwd(1), [80.0, 90.0], true);
    shaped.toggle_smooth(0);
    vec![
        line(),
        poly(&[[0.0, 0.0], [50.0, 40.0], [100.0, 0.0]], false),
        poly(&[[0.0, 0.0], [50.0, 40.0], [100.0, 0.0]], true),
        rect(),
        ellipse(),
        curve(),
        shaped,
        Figure::from_shape(Shape::Axes3d, [0.0, 0.0], [99.5, 77.25]).expect("axes"),
        axes(Shape::Axes2d),
    ]
}

#[test]
fn a_figure_survives_being_written_and_read_through_any_coordinate_change() {
    // A flip and a shift, like the PDF's page space; one that also turns by a quarter.
    let flip = |p: P| [p[0] + 100.0, 800.0 - p[1]];
    let unflip = |p: P| [p[0] - 100.0, 800.0 - p[1]];
    let turn = |p: P| [-p[1], p[0]];
    let unturn = |p: P| [p[1], -p[0]];
    for f in kinds() {
        let text = f.encode(flip);
        assert!(text.is_ascii() && !text.contains('(') && !text.contains(')') && !text.contains('\\'), "safe inside a PDF string: {text}");
        let back = Figure::decode(&text, unflip).expect("decoded");
        assert_eq!(std::mem::discriminant(&back), std::mem::discriminant(&f), "{text}: the same kind of shape");
        assert!(
            back.anchor_count() == f.anchor_count() && f.controls().iter().zip(back.controls()).all(|((c, p), (d, q))| c == &d && near(*p, q)),
            "{text}"
        );
        let turned = f.encode(turn);
        let back = Figure::decode(&turned, unturn).expect("decoded");
        assert!(back.points().iter().zip(f.points()).all(|(a, b)| near(*a, b)), "{turned}");
    }
}

#[test]
fn shapes_saved_before_they_had_kinds_of_their_own_come_back_as_what_they_are() {
    let id = |p: P| p;
    // The old rectangle: four corner anchors, closed; the old line: two; the old ellipse: four smooth.
    let old_rect = Figure::Path { anchors: [[10.0, 20.0], [110.0, 20.0], [110.0, 70.0], [10.0, 70.0]].map(Anchor::corner).to_vec(), closed: true };
    assert_eq!(Figure::decode(&old_rect.encode(id), id), Some(Figure::Rect { a: [10.0, 20.0], b: [110.0, 70.0] }));
    let old_line = Figure::Path { anchors: [[0.0, 0.0], [9.0, 4.0]].map(Anchor::corner).to_vec(), closed: false };
    assert_eq!(Figure::decode(&old_line.encode(id), id), Some(poly(&[[0.0, 0.0], [9.0, 4.0]], false)));
    let old_triangle = Figure::Path { anchors: [[0.0, 0.0], [9.0, 4.0], [3.0, 8.0]].map(Anchor::corner).to_vec(), closed: true };
    assert_eq!(Figure::decode(&old_triangle.encode(id), id), Some(poly(&[[0.0, 0.0], [9.0, 4.0], [3.0, 8.0]], true)));
    let skewed = Figure::Path { anchors: [[0.0, 0.0], [100.0, 5.0], [110.0, 70.0], [10.0, 70.0]].map(Anchor::corner).to_vec(), closed: true };
    assert!(
        matches!(Figure::decode(&skewed.encode(id), id), Some(Figure::Poly { closed: true, .. })),
        "a four-node polygon that is no rectangle stays a polygon"
    );
    let old_ellipse = Figure::Path { anchors: ellipse_anchors([0.0, 0.0], [100.0, 60.0]), closed: true };
    assert_eq!(Figure::decode(&old_ellipse.encode(id), id), Some(Figure::Ellipse { a: [0.0, 0.0], b: [100.0, 60.0] }));
    // An ellipse that was bent is a spline still.
    let mut bent = old_ellipse.clone();
    bent.move_control(Control::Fwd(1), [140.0, 90.0], false);
    assert!(matches!(Figure::decode(&bent.encode(id), id), Some(Figure::Path { .. })));
    let curve_text = curve().encode(id);
    assert!(matches!(Figure::decode(&curve_text, id), Some(Figure::Path { .. })), "splines stay splines");
}

#[test]
fn what_is_not_a_figure_is_refused_not_trusted() {
    let id = |p: P| p;
    let good = rect().encode(id);
    for bad in [
        "",
        "x;1,2",
        "p",
        "l",
        "l;1,2",
        "g;1,2;3,4",
        "l;1,2,3;4,5",
        "l;NaN,2;3,4",
        "r;1,2",
        "r;1,2;3,4;5,6",
        "e;1,2;3",
        "p;1,2,3",
        "p;a,b,c,d,e,f,1",
        "c;1,2,1,2,1,2,1",
        "p;1,2,1,2,1,2,1,9",
        "p;NaN,2,1,2,1,2,1;1,2,1,2,1,2,1",
        "a;1,2",
        "a;1,2;1,2,3,4,5,6",
        "p;1e999,2,1,2,1,2,1;3,4,3,4,3,4,1",
        &good[..good.len() / 2],
        "p;;;",
    ] {
        assert_eq!(Figure::decode(bad, id), None, "{bad:?}");
    }
    let many = format!("p;{}", vec!["1,2,1,2,1,2,0"; MAX_ANCHORS + 1].join(";"));
    assert_eq!(Figure::decode(&many, id), None);
    let many = format!("l;{}", vec!["1,2"; MAX_ANCHORS + 1].join(";"));
    assert_eq!(Figure::decode(&many, id), None);
    assert!(Figure::decode(&good, id).is_some());
}

#[test]
fn nothing_is_made_of_nonsense_or_a_drag_that_went_nowhere() {
    for shape in SHAPES {
        assert!(Figure::from_shape(shape, [f32::NAN, 0.0], [1.0, 1.0]).is_none(), "{shape:?}");
        assert!(Figure::from_shape(shape, [0.0, 0.0], [f32::INFINITY, 1.0]).is_none(), "{shape:?}");
        assert!(Figure::from_shape(shape, [5.0, 5.0], [5.0, 5.0]).is_none(), "{shape:?}");
    }
    assert!(Figure::from_shape(Shape::Axes2d, [0.0; 2], [1.5, 100.0]).is_none());
    let bad = Figure::Path { anchors: vec![Anchor::corner([f32::NAN, 0.0]), Anchor::corner([1.0, 1.0])], closed: false };
    assert!(bad.points().is_empty() && !bad.valid());
    assert!(!Figure::Path { anchors: vec![], closed: false }.valid());
    assert!(!Figure::Path { anchors: vec![Anchor::corner([0.0; 2])], closed: true }.valid());
    assert!(!poly(&[[0.0; 2]], false).valid() && !poly(&[[0.0; 2], [1.0, 1.0]], true).valid());
    assert!(!Figure::Rect { a: [f32::NAN, 0.0], b: [1.0, 1.0] }.valid() && !Figure::Ellipse { a: [0.0; 2], b: [f32::INFINITY, 1.0] }.valid());
}

#[test]
fn any_drag_and_any_sequence_of_edits_stays_finite_and_bounded() {
    let mut rng = Lcg(5);
    for round in 0..4000 {
        let extent_of = [1.0, 50.0, 800.0, 1.0e5, 1.0e9][round % 5];
        let (a, b) = (rng.point(extent_of), rng.point(extent_of));
        let shape = SHAPES[round % SHAPES.len()];
        let Some(mut f) = Figure::from_shape(shape, a, b) else { continue };
        for _ in 0..14 {
            let n = f.anchor_count().max(1);
            match rng.range(0.0, 9.0) as u32 {
                0 => {
                    let c = f.controls();
                    if !c.is_empty() {
                        let (control, _) = c[(rng.next() * c.len() as f32) as usize % c.len()];
                        f.move_control(control, rng.point(extent_of), rng.next() < 0.3);
                    }
                }
                1 => {
                    f.toggle_smooth((rng.next() * n as f32) as usize % n);
                }
                2 => {
                    if let Some(on) = f.nearest_on_path(rng.point(extent_of)) {
                        f.insert_anchor(on.segment, on.t);
                    }
                }
                3 => {
                    f.remove_anchor((rng.next() * n as f32) as usize % n);
                }
                4 => f.translate(rng.range(-extent_of, extent_of), rng.range(-extent_of, extent_of)),
                5 => {
                    f.toggle_closed((rng.next() * n as f32) as usize % n);
                }
                6 => {
                    f.extend(rng.point(extent_of), rng.next() < 0.5);
                }
                7 => {
                    f.merge_ends();
                }
                _ => {
                    let _ = Figure::decode(&f.encode(|p| p), |p| p);
                }
            }
            if !f.valid() {
                break;
            }
            let pts = f.points();
            assert!(!pts.is_empty() && pts.iter().all(|&p| finite(p)), "{shape:?} round {round}");
            assert!(pts.len() <= MAX_ANCHORS * PIECES.1 + 10, "{} points", pts.len());
        }
    }
}
