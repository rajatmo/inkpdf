//! Shapes you can edit after drawing them, the way a vector illustration program lets you. Each
//! kind of shape is edited by the anchors that suit it, so that an edit can never turn it into
//! something else:
//!
//! - [`Figure::Poly`] is straight lines: a line, a broken line, a polygon. An anchor for every node;
//!   the segments between them are always straight.
//! - [`Figure::Path`] is a spline: anchors in order, each with two **handles** (tangents) that bend
//!   the segments either side of it. An anchor with no handles is a corner; a *smooth* anchor keeps
//!   its two handles in one straight line, so the path flows through it.
//! - [`Figure::Rect`] is a rectangle: two anchors, the ends of its diagonal. It stays a rectangle.
//! - [`Figure::Ellipse`] is an ellipse: four anchors, the corners of its bounding rectangle. It stays
//!   an ellipse.
//! - [`Figure::Axes`] is cartesian axes, two or three straight lines that cross at the origin, with
//!   an arrowhead at each tip: anchors for the origin and the ends of each axis. An end slides along
//!   its axis, so the axes stay as they were drawn (perpendicular).
//!
//! A figure is drawn as one polyline ([`Figure::points`]), which is what is stored as ink; the figure
//! itself is kept beside it so the anchors can be edited, and is written into the PDF with it
//! ([`Figure::encode`]).
//!
//! Pure logic with no window or GPU types, so it is unit-tested.

use crate::geometry::Shape;
use crate::plane::P;

/// What can be done to a point of a shape being edited, from a button or a key (the pen's barrel button and a double
/// tap do the same with the stylus alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeAction {
    /// Take the point away.
    Remove,
    /// Put a new point on the longest stretch of the line.
    Add,
    /// Turn a corner into a smooth point and a smooth point into a corner.
    Smooth,
}

/// The polyline of `shape` for a drag from `a` to `b`: empty when the drag is too small to make anything of, or is not a
/// finite number.
pub fn shape_points(shape: Shape, a: P, b: P) -> Vec<P> {
    Figure::from_shape(shape, a, b).map(|f| f.points()).unwrap_or_default()
}

/// A handle shorter than this (in points) is no handle: the anchor is a corner on that side.
const NO_HANDLE: f32 = 1e-3;
/// Pieces a curved segment is cut into: at least, at most, and one per this many points of reach.
/// (Curves are drawn exactly as these pieces, so they are fine enough to look round when zoomed in.)
const PIECES: (usize, usize, f32) = (8, 128, 2.0);
/// Most anchors a figure may have, so an edit or a damaged file cannot make an absurd one.
pub const MAX_ANCHORS: usize = 1024;
/// How far a quarter of an ellipse's handle reaches, as a fraction of the radius.
const KAPPA: f32 = 0.552_284_8;
/// Two nodes this close (in points) are one node.
const SAME_NODE: f32 = 1e-3;

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(a: P, k: f32) -> P {
    [a[0] * k, a[1] * k]
}

fn dot(a: P, b: P) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn length(a: P) -> f32 {
    dot(a, a).sqrt()
}

fn finite(p: P) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

/// `v` with the length `len`, pointing the way `v` points (zero if it points nowhere).
fn with_length(v: P, len: f32) -> P {
    let l = length(v);
    if l > 0.0 { scale(v, len / l) } else { [0.0, 0.0] }
}

/// `v` as a direction, if it points anywhere.
fn unit(v: P) -> Option<P> {
    let l = length(v);
    (l > 1e-6 && l.is_finite()).then(|| scale(v, 1.0 / l))
}

/// One anchor point of a spline and its two handles, as offsets from the anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub pos: P,
    /// The handle on the side of the previous anchor.
    pub back: P,
    /// The handle on the side of the next anchor.
    pub fwd: P,
    /// Whether the handles are kept in one line.
    pub smooth: bool,
}

impl Anchor {
    pub fn corner(pos: P) -> Anchor {
        Anchor { pos, back: [0.0; 2], fwd: [0.0; 2], smooth: false }
    }

    fn has_handles(&self) -> bool {
        length(self.back) > NO_HANDLE || length(self.fwd) > NO_HANDLE
    }
}

/// Something about a figure that can be pressed and dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Anchor `i`. (A rectangle's are the two ends of its diagonal; an ellipse's the corners of its
    /// bounding rectangle, clockwise from the top left; axes': 0 is where they cross, then each
    /// axis' tail and tip in turn.)
    Anchor(usize),
    /// The handle of spline anchor `i` towards the previous anchor.
    Back(usize),
    /// The handle of spline anchor `i` towards the next anchor.
    Fwd(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Figure {
    /// Straight segments through the nodes, open (a line, a broken line) or closed (a polygon).
    Poly {
        points: Vec<P>,
        closed: bool,
    },
    /// A spline.
    Path {
        anchors: Vec<Anchor>,
        closed: bool,
    },
    /// The rectangle with `a` and `b` for opposite corners.
    Rect {
        a: P,
        b: P,
    },
    /// The ellipse inscribed in the rectangle with `a` and `b` for opposite corners.
    Ellipse {
        a: P,
        b: P,
    },
    Axes {
        origin: P,
        ends: Vec<[P; 2]>,
    },
}

/// Where on a path a point is nearest: the segment (from anchor `segment` to the next) and how far
/// along it, 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnPath {
    pub segment: usize,
    pub t: f32,
    pub at: P,
    pub distance: f32,
}

/// The four spline anchors of the ellipse inscribed in the rectangle with corners `a` and `b`: top,
/// right, bottom and left, so the curve runs clockwise on screen.
fn ellipse_anchors(a: P, b: P) -> Vec<Anchor> {
    let [x0, y0, x1, y1] = bounds(a, b);
    let (centre, hx, hy) = ([(x0 + x1) / 2.0, (y0 + y1) / 2.0], (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (kx, ky) = (KAPPA * hx, KAPPA * hy);
    let smooth = |pos: P, along: P| Anchor { pos, back: scale(along, -1.0), fwd: along, smooth: true };
    vec![
        smooth([centre[0], y0], [kx, 0.0]),
        smooth([x1, centre[1]], [0.0, ky]),
        smooth([centre[0], y1], [-kx, 0.0]),
        smooth([x0, centre[1]], [0.0, -ky]),
    ]
}

/// `[left, top, right, bottom]` of the rectangle with opposite corners `a` and `b`.
fn bounds(a: P, b: P) -> [f32; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])]
}

/// Corner `i` of the rectangle with corners `a` and `b`: top left, top right, bottom right, bottom
/// left.
fn corner(a: P, b: P, i: usize) -> Option<P> {
    let [x0, y0, x1, y1] = bounds(a, b);
    Some(match i {
        0 => [x0, y0],
        1 => [x1, y0],
        2 => [x1, y1],
        3 => [x0, y1],
        _ => return None,
    })
}

/// The way an axis runs (`origin` and the ends are collinear for axes as drawn). Taken from the longest
/// of the vectors between them, so that an axis shrunk almost to nothing does not drift off its line
/// on rounding error.
fn axis_direction(origin: P, end: [P; 2]) -> P {
    let longest = [sub(end[1], end[0]), sub(end[1], origin), sub(origin, end[0])].into_iter().max_by(|a, b| length(*a).total_cmp(&length(*b)));
    longest.and_then(unit).unwrap_or([1.0, 0.0])
}

/// `p` moved onto the line through `through` running along `dir`.
fn onto_line(p: P, through: P, dir: P) -> P {
    add(through, scale(dir, dot(sub(p, through), dir)))
}

impl Figure {
    /// The figure for `shape` dragged from `a` to `b`; `None` if the drag is too small or not a
    /// finite number.
    pub fn from_shape(shape: Shape, a: P, b: P) -> Option<Figure> {
        if !finite(a) || !finite(b) {
            return None;
        }
        let (centre, hx, hy) = (scale(add(a, b), 0.5), (b[0] - a[0]).abs() / 2.0, (b[1] - a[1]).abs() / 2.0);
        if length(sub(b, a)) < 1e-3 {
            return None;
        }
        let figure = match shape {
            Shape::Line => Figure::Poly { points: vec![a, b], closed: false },
            Shape::Curve => {
                // An S, so the handles are easy to see and to grab.
                let d = sub(b, a);
                let len = length(d);
                let lift = with_length([-d[1], d[0]], 0.3 * len);
                let third = scale(d, 1.0 / 3.0);
                Figure::Path {
                    anchors: vec![
                        Anchor { pos: a, back: [0.0; 2], fwd: add(third, lift), smooth: true },
                        Anchor { pos: b, back: sub(scale(third, -1.0), lift), fwd: [0.0; 2], smooth: true },
                    ],
                    closed: false,
                }
            }
            Shape::Rectangle => Figure::Rect { a, b },
            Shape::Ellipse => Figure::Ellipse { a, b },
            Shape::Axes2d | Shape::Axes3d => {
                if hx < 2.0 || hy < 2.0 {
                    return None;
                }
                let mut ends =
                    vec![[[centre[0] - hx, centre[1]], [centre[0] + hx, centre[1]]], [[centre[0], centre[1] + hy], [centre[0], centre[1] - hy]]];
                if shape == Shape::Axes3d {
                    // z comes towards the viewer, down and to the left.
                    ends.push([[centre[0] + 0.35 * hx, centre[1] - 0.35 * hy], [centre[0] - 0.7 * hx, centre[1] + 0.7 * hy]]);
                }
                Figure::Axes { origin: centre, ends }
            }
        };
        figure.valid().then_some(figure)
    }

    /// The straight lines through `nodes` (a node on top of the one before it is dropped), closed into a
    /// polygon if asked; `None` if there are too few distinct nodes (two, or three for a polygon).
    pub fn polyline(nodes: &[P], closed: bool) -> Option<Figure> {
        let mut points: Vec<P> = Vec::with_capacity(nodes.len());
        for &p in nodes {
            if points.last().is_none_or(|&last| length(sub(p, last)) >= SAME_NODE) {
                points.push(p);
            }
        }
        if closed && points.len() > 1 && length(sub(points[0], points[points.len() - 1])) < SAME_NODE {
            points.pop();
        }
        let figure = Figure::Poly { points, closed };
        figure.valid().then_some(figure)
    }

    /// The spline through `nodes`, every node smooth with its tangents along the line between its
    /// neighbours; closed into a loop if asked. `None` if there are too few distinct nodes.
    pub fn spline(nodes: &[P], closed: bool) -> Option<Figure> {
        let Some(Figure::Poly { points, closed }) = Figure::polyline(nodes, closed) else { return None };
        let mut figure = Figure::Path { anchors: points.into_iter().map(Anchor::corner).collect(), closed };
        for i in 0..figure.anchor_count() {
            figure.toggle_smooth(i);
        }
        figure.valid().then_some(figure)
    }

    /// Whether every number in it is finite and there are no more anchors than allowed.
    pub fn valid(&self) -> bool {
        match self {
            Figure::Poly { points, closed } => {
                (if *closed { 3 } else { 2 }..=MAX_ANCHORS).contains(&points.len()) && points.iter().all(|&p| finite(p))
            }
            Figure::Path { anchors, closed } => {
                !anchors.is_empty()
                    && anchors.len() <= MAX_ANCHORS
                    && (anchors.len() >= 2 || !closed)
                    && anchors.iter().all(|a| finite(a.pos) && finite(a.back) && finite(a.fwd))
            }
            Figure::Rect { a, b } | Figure::Ellipse { a, b } => finite(*a) && finite(*b),
            Figure::Axes { origin, ends } => finite(*origin) && (1..=3).contains(&ends.len()) && ends.iter().all(|e| finite(e[0]) && finite(e[1])),
        }
    }

    /// The figure as one polyline. Empty if the figure is not valid.
    pub fn points(&self) -> Vec<P> {
        if !self.valid() {
            return Vec::new();
        }
        match self {
            Figure::Poly { points, closed } => {
                let mut out = points.clone();
                if *closed {
                    out.push(points[0]);
                }
                out
            }
            Figure::Path { anchors, closed } => path_points(anchors, *closed),
            Figure::Rect { a, b } => vec![*a, [b[0], a[1]], *b, [a[0], b[1]], *a],
            Figure::Ellipse { a, b } => path_points(&ellipse_anchors(*a, *b), true),
            Figure::Axes { origin, ends } => axes_points(*origin, ends),
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        let by = [dx, dy];
        match self {
            Figure::Poly { points, .. } => points.iter_mut().for_each(|p| *p = add(*p, by)),
            Figure::Path { anchors, .. } => anchors.iter_mut().for_each(|a| a.pos = add(a.pos, by)),
            Figure::Rect { a, b } | Figure::Ellipse { a, b } => {
                *a = add(*a, by);
                *b = add(*b, by);
            }
            Figure::Axes { origin, ends } => {
                *origin = add(*origin, by);
                ends.iter_mut().for_each(|e| *e = e.map(|p| add(p, by)));
            }
        }
    }

    /// How many anchors there are.
    pub fn anchor_count(&self) -> usize {
        match self {
            Figure::Poly { points, .. } => points.len(),
            Figure::Path { anchors, .. } => anchors.len(),
            Figure::Rect { .. } => 2,
            Figure::Ellipse { .. } => 4,
            Figure::Axes { ends, .. } => 1 + 2 * ends.len(),
        }
    }

    pub fn anchor_pos(&self, i: usize) -> Option<P> {
        match self {
            Figure::Poly { points, .. } => points.get(i).copied(),
            Figure::Path { anchors, .. } => anchors.get(i).map(|a| a.pos),
            Figure::Rect { a, b } => match i {
                0 => Some(*a),
                1 => Some(*b),
                _ => None,
            },
            Figure::Ellipse { a, b } => corner(*a, *b, i),
            Figure::Axes { origin, ends } => match i {
                0 => Some(*origin),
                _ => ends.get((i - 1) / 2).map(|e| e[(i - 1) % 2]),
            },
        }
    }

    /// Dashed guide lines to show with the anchors: a rectangle's diagonal, an ellipse's bounding
    /// rectangle.
    pub fn guides(&self) -> Vec<[P; 2]> {
        match self {
            Figure::Rect { a, b } => vec![[*a, *b]],
            Figure::Ellipse { a, b } => (0..4).filter_map(|i| Some([corner(*a, *b, i)?, corner(*a, *b, (i + 1) % 4)?])).collect(),
            _ => Vec::new(),
        }
    }

    /// Every control that can be pressed, with where it is: the anchors, and the handles that exist.
    pub fn controls(&self) -> Vec<(Control, P)> {
        let mut out = Vec::new();
        for i in 0..self.anchor_count() {
            if let Some(p) = self.anchor_pos(i) {
                out.push((Control::Anchor(i), p));
            }
        }
        if let Figure::Path { anchors, .. } = self {
            for (i, a) in anchors.iter().enumerate() {
                if length(a.back) > NO_HANDLE {
                    out.push((Control::Back(i), add(a.pos, a.back)));
                }
                if length(a.fwd) > NO_HANDLE {
                    out.push((Control::Fwd(i), add(a.pos, a.fwd)));
                }
            }
        }
        out
    }

    /// The control nearest `at` within `radius`, handles winning over anchors where they overlap
    /// (a handle can sit right on its anchor while it is being pulled out).
    pub fn control_near(&self, at: P, radius: f32) -> Option<Control> {
        let controls = self.controls();
        let nearest = |want_anchor: bool| {
            controls
                .iter()
                .filter(|(c, _)| matches!(c, Control::Anchor(_)) == want_anchor)
                .map(|&(c, p)| (c, length(sub(p, at))))
                .filter(|&(_, d)| d <= radius)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(c, _)| c)
        };
        nearest(true).or_else(|| nearest(false))
    }

    /// Where a control is now.
    #[cfg(test)]
    pub fn control_pos(&self, control: Control) -> Option<P> {
        match (self, control) {
            (_, Control::Anchor(i)) => self.anchor_pos(i),
            (Figure::Path { anchors, .. }, Control::Back(i)) => anchors.get(i).map(|a| add(a.pos, a.back)),
            (Figure::Path { anchors, .. }, Control::Fwd(i)) => anchors.get(i).map(|a| add(a.pos, a.fwd)),
            _ => None,
        }
    }

    /// Move a control to `to`.
    ///
    /// - A node of straight lines goes where it is put; the segments stay straight.
    /// - Moving a spline anchor takes its handles along. Moving a handle of a smooth anchor swings the
    ///   other handle to stay in line with it, keeping its own length; with `independent` (the barrel
    ///   button) the handle moves alone and the anchor becomes a corner.
    /// - A rectangle's anchor is one end of its diagonal; an ellipse's is a corner of its bounding
    ///   rectangle, the opposite corner staying where it is.
    /// - Where axes cross: the crossing moves, and each axis follows it (keeping its ends' places
    ///   along it). An end of an axis slides along the axis.
    pub fn move_control(&mut self, control: Control, to: P, independent: bool) {
        if !finite(to) {
            return;
        }
        match (self, control) {
            (Figure::Poly { points, .. }, Control::Anchor(i)) => {
                if let Some(p) = points.get_mut(i) {
                    *p = to;
                }
            }
            (Figure::Path { anchors, .. }, Control::Anchor(i)) => {
                if let Some(a) = anchors.get_mut(i) {
                    a.pos = to;
                }
            }
            (Figure::Path { anchors, .. }, Control::Back(i) | Control::Fwd(i)) => {
                let Some(a) = anchors.get_mut(i) else { return };
                let moved = sub(to, a.pos);
                let back = matches!(control, Control::Back(_));
                let (this, other) = if back { (&mut a.back, &mut a.fwd) } else { (&mut a.fwd, &mut a.back) };
                *this = moved;
                if independent {
                    a.smooth = false;
                } else if a.smooth {
                    let keep = length(*other);
                    // A handle with no length has no direction to be in line with: the other one
                    // simply mirrors it.
                    *other = if keep > NO_HANDLE { with_length(scale(moved, -1.0), keep) } else { scale(moved, -1.0) };
                }
            }
            (Figure::Rect { a, b }, Control::Anchor(i)) => match i {
                0 => *a = to,
                1 => *b = to,
                _ => {}
            },
            (Figure::Ellipse { a, b }, Control::Anchor(i)) => {
                if let Some(opposite) = corner(*a, *b, (i + 2) % 4).filter(|_| i < 4) {
                    *a = opposite;
                    *b = to;
                }
            }
            (Figure::Axes { origin, ends }, Control::Anchor(0)) => {
                for e in ends.iter_mut() {
                    let dir = axis_direction(*origin, *e);
                    *e = e.map(|p| onto_line(p, to, dir));
                }
                *origin = to;
            }
            (Figure::Axes { origin, ends }, Control::Anchor(i)) => {
                if let Some(e) = ends.get_mut((i - 1) / 2) {
                    let dir = axis_direction(*origin, *e);
                    e[(i - 1) % 2] = onto_line(to, *origin, dir);
                }
            }
            _ => {}
        }
    }

    /// Switch spline anchor `i` between a corner (no handles) and a smooth point: one with handles is
    /// made a corner; a corner gets handles along the line between its neighbours. Returns whether
    /// anything changed. (Only a spline has smooth points: straight lines stay straight.)
    pub fn toggle_smooth(&mut self, i: usize) -> bool {
        let Figure::Path { anchors, closed } = self else { return false };
        let n = anchors.len();
        let Some(&a) = anchors.get(i) else { return false };
        if a.has_handles() {
            anchors[i] = Anchor::corner(a.pos);
            return true;
        }
        let wrap = |k: isize| -> Option<P> {
            let k = if *closed { k.rem_euclid(n as isize) as usize } else { usize::try_from(k).ok().filter(|&k| k < n)? };
            anchors.get(k).map(|a| a.pos)
        };
        let (prev, next) = (wrap(i as isize - 1), wrap(i as isize + 1));
        // The line the path flows along through this point; at an open end, towards the one neighbour.
        let direction = match (prev, next) {
            (Some(p), Some(q)) => sub(q, p),
            (None, Some(q)) => sub(q, a.pos),
            (Some(p), None) => sub(a.pos, p),
            (None, None) => return false,
        };
        let unit = with_length(direction, 1.0);
        if unit == [0.0, 0.0] {
            return false;
        }
        let reach = |other: Option<P>| other.map_or(0.0, |q| length(sub(q, a.pos)) / 3.0);
        anchors[i] = Anchor { pos: a.pos, back: scale(unit, -reach(prev)), fwd: scale(unit, reach(next)), smooth: true };
        true
    }

    /// Close an open run of straight lines into a polygon at its end node `i` (with at least three
    /// nodes), or open a polygon at node `i`: the polygon becomes a line that runs all the way round
    /// and ends where it began, ready to have an end pulled away. Returns whether anything changed.
    pub fn toggle_closed(&mut self, i: usize) -> bool {
        let Figure::Poly { points, closed } = self else { return false };
        let n = points.len();
        if i >= n {
            return false;
        }
        if *closed {
            if n + 1 > MAX_ANCHORS {
                return false;
            }
            let mut opened: Vec<P> = (0..n).map(|k| points[(i + k) % n]).collect();
            opened.push(points[i]);
            *points = opened;
            *closed = false;
            true
        } else if n >= 3 && (i == 0 || i == n - 1) {
            if length(sub(points[0], points[n - 1])) < SAME_NODE {
                // Its ends are one node already: that node is the polygon's.
                if n < 4 {
                    return false;
                }
                points.pop();
            }
            *closed = true;
            true
        } else {
            false
        }
    }

    /// Add a node to an open run of straight lines at `at`, at its end (or its start, with
    /// `at_start`), and return its index.
    pub fn extend(&mut self, at: P, at_start: bool) -> Option<usize> {
        let Figure::Poly { points, closed: false } = self else { return None };
        let n = points.len();
        if !finite(at) || !(2..MAX_ANCHORS).contains(&n) {
            return None;
        }
        if at_start {
            points.insert(0, at);
            Some(0)
        } else {
            points.push(at);
            Some(n)
        }
    }

    /// An open run of straight lines whose ends have been brought together is a polygon: join them.
    /// Returns whether it was.
    pub fn merge_ends(&mut self) -> bool {
        let Figure::Poly { points, closed: false } = self else { return false };
        let n = points.len();
        if n < 4 || length(sub(points[0], points[n - 1])) >= SAME_NODE {
            return false;
        }
        points.pop();
        if let Figure::Poly { closed, .. } = self {
            *closed = true;
        }
        true
    }

    /// The point on the path nearest `at`, if the figure is straight lines or a spline.
    pub fn nearest_on_path(&self, at: P) -> Option<OnPath> {
        match self {
            Figure::Poly { points, closed } => {
                let n = points.len();
                let segments = if *closed { n } else { n.saturating_sub(1) };
                let mut best: Option<OnPath> = None;
                for segment in 0..segments {
                    let (a, b) = (points[segment], points[(segment + 1) % n]);
                    let (t, on) = nearest_on_segment(a, b, at);
                    let distance = length(sub(at, on));
                    if best.is_none_or(|b| distance < b.distance) {
                        best = Some(OnPath { segment, t, at: on, distance });
                    }
                }
                best
            }
            Figure::Path { anchors, closed } => {
                let n = anchors.len();
                let segments = if *closed { n } else { n.saturating_sub(1) };
                let mut best: Option<OnPath> = None;
                for segment in 0..segments {
                    let [p0, p1, p2, p3] = segment_controls(anchors, segment);
                    let pieces = pieces_for(&[p0, p1, p2, p3]);
                    for k in 0..pieces {
                        let (ta, tb) = (k as f32 / pieces as f32, (k + 1) as f32 / pieces as f32);
                        let (a, b) = (bezier([p0, p1, p2, p3], ta), bezier([p0, p1, p2, p3], tb));
                        let (s, on) = nearest_on_segment(a, b, at);
                        let distance = length(sub(at, on));
                        if best.is_none_or(|b| distance < b.distance) {
                            best = Some(OnPath { segment, t: ta + (tb - ta) * s, at: on, distance });
                        }
                    }
                }
                best
            }
            _ => None,
        }
    }

    /// Add an anchor on `segment` at `t` (0 to 1), splitting the segment so the shape does not
    /// change. Returns the new anchor's index. (Straight lines and splines only.)
    pub fn insert_anchor(&mut self, segment: usize, t: f32) -> Option<usize> {
        if let Figure::Poly { points, closed } = self {
            let n = points.len();
            if n >= MAX_ANCHORS || segment >= if *closed { n } else { n.saturating_sub(1) } || !t.is_finite() {
                return None;
            }
            let (a, b) = (points[segment], points[(segment + 1) % n]);
            points.insert(segment + 1, add(a, scale(sub(b, a), t.clamp(0.0, 1.0))));
            return Some(segment + 1);
        }
        let Figure::Path { anchors, closed } = self else { return None };
        let n = anchors.len();
        if n >= MAX_ANCHORS || segment >= if *closed { n } else { n.saturating_sub(1) } || !t.is_finite() {
            return None;
        }
        let t = t.clamp(0.0, 1.0);
        let [p0, p1, p2, p3] = segment_controls(anchors, segment);
        // de Casteljau: the two halves of the cubic.
        let lerp = |a: P, b: P| add(a, scale(sub(b, a), t));
        let (q0, q1, q2) = (lerp(p0, p1), lerp(p1, p2), lerp(p2, p3));
        let (r0, r1) = (lerp(q0, q1), lerp(q1, q2));
        let mid = lerp(r0, r1);
        let next = (segment + 1) % n;
        let straight = length(anchors[segment].fwd) <= NO_HANDLE && length(anchors[next].back) <= NO_HANDLE;
        anchors[segment].fwd = sub(q0, p0);
        anchors[next].back = sub(q2, p3);
        let new = if straight { Anchor::corner(mid) } else { Anchor { pos: mid, back: sub(r0, mid), fwd: sub(r1, mid), smooth: true } };
        let at = segment + 1;
        anchors.insert(at, new);
        Some(at)
    }

    /// Take anchor `i` away, joining its neighbours. `false` (and nothing changes) if the figure
    /// would be left with too few anchors, or has none that can go (a rectangle, an ellipse, axes).
    pub fn remove_anchor(&mut self, i: usize) -> bool {
        match self {
            Figure::Poly { points, closed } => {
                if points.len() <= if *closed { 3 } else { 2 } || i >= points.len() {
                    return false;
                }
                points.remove(i);
                true
            }
            Figure::Path { anchors, closed } => {
                if anchors.len() <= if *closed { 3 } else { 2 } || i >= anchors.len() {
                    return false;
                }
                anchors.remove(i);
                true
            }
            _ => false,
        }
    }

    // ── the figure in a file ────────────────────────────────────────────────────────────────

    /// The figure as text, to go in the file beside the polyline; `map` turns each point into the
    /// file's own coordinates. Handles are written as points, not offsets, so any map (a flip, a
    /// rotation) carries them correctly.
    pub fn encode(&self, map: impl Fn(P) -> P) -> String {
        let point = |p: P| {
            let q = map(p);
            format!("{},{}", q[0], q[1])
        };
        match self {
            Figure::Poly { points, closed } => {
                let rows: Vec<String> = points.iter().map(|&p| point(p)).collect();
                format!("{};{}", if *closed { "g" } else { "l" }, rows.join(";"))
            }
            Figure::Path { anchors, closed } => {
                let rows: Vec<String> = anchors
                    .iter()
                    .map(|a| format!("{},{},{},{}", point(a.pos), point(add(a.pos, a.back)), point(add(a.pos, a.fwd)), u8::from(a.smooth)))
                    .collect();
                format!("{};{}", if *closed { "c" } else { "p" }, rows.join(";"))
            }
            Figure::Rect { a, b } => format!("r;{};{}", point(*a), point(*b)),
            Figure::Ellipse { a, b } => format!("e;{};{}", point(*a), point(*b)),
            Figure::Axes { origin, ends } => {
                let rows: Vec<String> = ends.iter().map(|e| format!("{},{}", point(e[0]), point(e[1]))).collect();
                format!("a;{};{}", point(*origin), rows.join(";"))
            }
        }
    }

    /// The figure written by [`Figure::encode`], with `map` undoing the file's coordinates.
    /// `None` for anything that is not one (damaged, foreign, absurd): the ink is then plain ink.
    /// (Files from before shapes had their own kinds have lines, rectangles and ellipses as splines:
    /// they come back as what they are.)
    pub fn decode(text: &str, map: impl Fn(P) -> P) -> Option<Figure> {
        let mut parts = text.split(';');
        let kind = parts.next()?;
        let numbers = |row: &str| -> Option<Vec<f32>> { row.split(',').map(|n| n.parse::<f32>().ok().filter(|v| v.is_finite())).collect() };
        let point = |n: &[f32], at: usize| Some(map([*n.get(at)?, *n.get(at + 1)?]));
        let figure = match kind {
            "l" | "g" => {
                let mut points = Vec::new();
                for row in parts {
                    let n = numbers(row)?;
                    if n.len() != 2 || points.len() >= MAX_ANCHORS {
                        return None;
                    }
                    points.push(point(&n, 0)?);
                }
                Figure::Poly { points, closed: kind == "g" }
            }
            "r" | "e" => {
                let (a, b) = (numbers(parts.next()?)?, numbers(parts.next()?)?);
                if parts.next().is_some() || a.len() != 2 || b.len() != 2 {
                    return None;
                }
                let (a, b) = (point(&a, 0)?, point(&b, 0)?);
                if kind == "r" { Figure::Rect { a, b } } else { Figure::Ellipse { a, b } }
            }
            "p" | "c" => {
                let mut anchors = Vec::new();
                for row in parts {
                    let n = numbers(row)?;
                    if n.len() != 7 || anchors.len() >= MAX_ANCHORS {
                        return None;
                    }
                    let pos = point(&n, 0)?;
                    anchors.push(Anchor { pos, back: sub(point(&n, 2)?, pos), fwd: sub(point(&n, 4)?, pos), smooth: n[6] > 0.5 });
                }
                from_spline(anchors, kind == "c")
            }
            "a" => {
                let origin = point(&numbers(parts.next()?)?, 0)?;
                let mut ends = Vec::new();
                for row in parts {
                    let n = numbers(row)?;
                    if n.len() != 4 || ends.len() >= 3 {
                        return None;
                    }
                    ends.push([point(&n, 0)?, point(&n, 2)?]);
                }
                Figure::Axes { origin, ends }
            }
            _ => return None,
        };
        figure.valid().then_some(figure)
    }
}

/// What a spline read from a file really is: one with no handles anywhere is straight lines, and
/// the four-anchor splines that rectangles and ellipses used to be saved as are those again.
fn from_spline(anchors: Vec<Anchor>, closed: bool) -> Figure {
    let near = |a: f32, b: f32| (a - b).abs() <= 0.02 + 1e-4 * a.abs().max(b.abs());
    if anchors.iter().all(|a| !a.has_handles() && !a.smooth) {
        if let (true, [p0, p1, p2, p3]) = (closed, &anchors[..]) {
            let (p0, p1, p2, p3) = (p0.pos, p1.pos, p2.pos, p3.pos);
            if near(p1[1], p0[1]) && near(p2[0], p1[0]) && near(p3[1], p2[1]) && near(p3[0], p0[0]) {
                return Figure::Rect { a: p0, b: p2 };
            }
        }
        return Figure::Poly { points: anchors.iter().map(|a| a.pos).collect(), closed };
    }
    if let (true, [top, right, bottom, left]) = (closed, &anchors[..]) {
        let (hx, hy) = ((right.pos[0] - left.pos[0]) / 2.0, (bottom.pos[1] - top.pos[1]) / 2.0);
        let like = ellipse_anchors([left.pos[0], top.pos[1]], [right.pos[0], bottom.pos[1]]);
        let same = |a: &Anchor, b: &Anchor| {
            a.smooth
                && near(a.pos[0], b.pos[0])
                && near(a.pos[1], b.pos[1])
                && near(a.fwd[0], b.fwd[0])
                && near(a.fwd[1], b.fwd[1])
                && near(a.back[0], b.back[0])
                && near(a.back[1], b.back[1])
        };
        if hx > 0.0 && hy > 0.0 && [top, right, bottom, left].into_iter().zip(&like).all(|(a, b)| same(a, b)) {
            return Figure::Ellipse { a: [left.pos[0], top.pos[1]], b: [right.pos[0], bottom.pos[1]] };
        }
    }
    Figure::Path { anchors, closed }
}

/// The parameter (0 to 1) and the point on the segment `a`–`b` nearest `p`.
fn nearest_on_segment(a: P, b: P, p: P) -> (f32, P) {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let s = if len2 > 0.0 { (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (s, add(a, scale(ab, s)))
}

/// The four control points of the cubic from anchor `segment` to the next.
fn segment_controls(anchors: &[Anchor], segment: usize) -> [P; 4] {
    let a = anchors[segment];
    let b = anchors[(segment + 1) % anchors.len()];
    [a.pos, add(a.pos, a.fwd), add(b.pos, b.back), b.pos]
}

fn bezier(c: [P; 4], t: f32) -> P {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    [0, 1].map(|axis| (0..4).map(|k| w[k] * c[k][axis]).sum())
}

fn pieces_for(c: &[P; 4]) -> usize {
    let reach: f32 = c.windows(2).map(|w| length(sub(w[1], w[0]))).sum();
    ((reach / PIECES.2).ceil() as usize).clamp(PIECES.0, PIECES.1)
}

fn path_points(anchors: &[Anchor], closed: bool) -> Vec<P> {
    let n = anchors.len();
    let segments = if closed { n } else { n - 1 };
    let mut out = vec![anchors[0].pos];
    for segment in 0..segments {
        let c = segment_controls(anchors, segment);
        let straight = length(sub(c[1], c[0])) <= NO_HANDLE && length(sub(c[2], c[3])) <= NO_HANDLE;
        if straight {
            out.push(c[3]);
        } else {
            let pieces = pieces_for(&c);
            out.extend((1..=pieces).map(|k| bezier(c, k as f32 / pieces as f32)));
        }
    }
    out
}

/// Axes as one stroke: for each axis out to its tail and back, then to its tip with the arrowhead,
/// and back to where they cross, ready for the next.
fn axes_points(origin: P, ends: &[[P; 2]]) -> Vec<P> {
    let mut out = vec![origin];
    for &[tail, tip] in ends {
        let axis = sub(tip, origin);
        let len = length(axis);
        let dir = if len > 0.0 { scale(axis, 1.0 / len) } else { [1.0, 0.0] };
        let size = (0.2 * len).min(12.0);
        let across = [-dir[1], dir[0]];
        let back = sub(tip, scale(dir, size));
        let (one, two) = (add(back, scale(across, size * 0.45)), sub(back, scale(across, size * 0.45)));
        out.extend([tail, origin, tip, one, tip, two, tip, origin]);
    }
    out
}

#[cfg(test)]
mod tests;
