//! The geometry tool's shapes: lines, Bézier curves, 2D and 3D cartesian axes, rectangles and
//! ellipses. Each is made by a drag from one corner to the other; what it is made of, and how it is
//! edited afterwards (by its anchor points and handles), is in [`crate::figure`].
//!
//! Pure logic with no window or GPU types, so it is unit-tested.

use crate::figure::Figure;
use crate::ink::P;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Line,
    Curve,
    Axes2d,
    Axes3d,
    Rectangle,
    Ellipse,
}

/// All of them, in the order the panel shows them.
pub const SHAPES: [Shape; 6] = [Shape::Line, Shape::Curve, Shape::Axes2d, Shape::Axes3d, Shape::Rectangle, Shape::Ellipse];

impl Shape {
    /// The name used in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Shape::Line => "line",
            Shape::Curve => "curve",
            Shape::Axes2d => "axes2d",
            Shape::Axes3d => "axes3d",
            Shape::Rectangle => "rectangle",
            Shape::Ellipse => "ellipse",
        }
    }

    pub fn from_name(name: &str) -> Option<Shape> {
        SHAPES.into_iter().find(|s| s.name() == name)
    }
}

/// The polyline of `shape` for a drag from `a` to `b`: empty when the drag is too small to make
/// anything of, or is not a finite number.
pub fn points(shape: Shape, a: P, b: P) -> Vec<P> {
    Figure::from_shape(shape, a, b).map(|f| f.points()).unwrap_or_default()
}

/// `p` moved into `rect` (`x0, y0, x1, y1`).
pub fn clamp_into(rect: [f32; 4], p: P) -> P {
    [p[0].clamp(rect[0].min(rect[2]), rect[0].max(rect[2])), p[1].clamp(rect[1].min(rect[3]), rect[1].max(rect[3]))]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32) / (1u64 << 31) as f32
        }

        fn point(&mut self, extent: f32) -> P {
            [(self.next() * 2.0 - 1.0) * extent, (self.next() * 2.0 - 1.0) * extent]
        }
    }

    fn bounds(pts: &[P]) -> [f32; 4] {
        pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
    }

    #[test]
    fn names_round_trip_and_unknown_names_are_refused() {
        for shape in SHAPES {
            assert_eq!(Shape::from_name(shape.name()), Some(shape));
        }
        assert_eq!(Shape::from_name("hexagon"), None);
        assert_eq!(Shape::from_name(""), None);
    }

    #[test]
    fn the_simple_shapes_are_what_was_dragged() {
        assert_eq!(points(Shape::Line, [1.0, 2.0], [30.0, 40.0]), vec![[1.0, 2.0], [30.0, 40.0]]);
        assert_eq!(points(Shape::Rectangle, [10.0, 20.0], [50.0, 80.0]), vec![[10.0, 20.0], [50.0, 20.0], [50.0, 80.0], [10.0, 80.0], [10.0, 20.0]]);
        // Dragged backwards, a rectangle is the same rectangle.
        assert_eq!(bounds(&points(Shape::Rectangle, [50.0, 80.0], [10.0, 20.0])), [10.0, 20.0, 50.0, 80.0]);
        let e = bounds(&points(Shape::Ellipse, [10.0, 20.0], [110.0, 60.0]));
        assert!((e[0] - 10.0).abs() < 0.05 && (e[2] - 110.0).abs() < 0.05 && (e[1] - 20.0).abs() < 0.05 && (e[3] - 60.0).abs() < 0.05, "{e:?}");
    }

    #[test]
    fn axes_fill_their_box_and_the_curve_swings_both_ways() {
        assert_eq!(bounds(&points(Shape::Axes2d, [0.0, 0.0], [200.0, 100.0])), [0.0, 0.0, 200.0, 100.0]);
        let b = bounds(&points(Shape::Axes3d, [0.0, 0.0], [300.0, 200.0]));
        assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= 300.0 && b[3] <= 200.0, "{b:?}");
        let c = bounds(&points(Shape::Curve, [0.0, 0.0], [90.0, 0.0]));
        assert!(c[1] < -1.0 && c[3] > 1.0, "{c:?}");
    }

    #[test]
    fn a_point_is_pulled_into_a_rectangle_whichever_way_it_is_given() {
        assert_eq!(clamp_into([0.0, 0.0, 10.0, 20.0], [-5.0, 30.0]), [0.0, 20.0]);
        assert_eq!(clamp_into([10.0, 20.0, 0.0, 0.0], [5.0, 5.0]), [5.0, 5.0], "a rectangle given the other way round");
        assert_eq!(clamp_into([0.0, 0.0, 10.0, 10.0], [3.0, 4.0]), [3.0, 4.0]);
    }

    #[test]
    fn nothing_is_made_of_nonsense_or_a_drag_that_went_nowhere() {
        for shape in SHAPES {
            assert!(points(shape, [f32::NAN, 0.0], [1.0, 1.0]).is_empty(), "{shape:?}");
            assert!(points(shape, [0.0, 0.0], [f32::INFINITY, 1.0]).is_empty(), "{shape:?}");
            assert!(points(shape, [5.0, 5.0], [5.0, 5.0]).is_empty(), "{shape:?} for a press that did not move");
        }
    }

    #[test]
    fn shapes_stay_finite_bounded_and_the_simple_ones_inside_their_box_for_any_drag() {
        let mut rng = Lcg(7);
        for round in 0..4000 {
            // Everything from a speck to far beyond the page, and the odd exact repeat.
            let extent = [0.5, 5.0, 200.0, 5000.0, 1.0e6, 1.0e9][round % 6];
            let a = rng.point(extent);
            let b = if round % 17 == 0 { a } else { rng.point(extent) };
            for shape in SHAPES {
                let pts = points(shape, a, b);
                assert!(pts.len() <= 600, "{shape:?}: {} points", pts.len());
                assert!(pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "{shape:?} {a:?} {b:?}");
                if !matches!(shape, Shape::Line | Shape::Rectangle | Shape::Ellipse) || pts.is_empty() {
                    continue;
                }
                let (lo, hi) = ([a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])]);
                let slack = 1e-3 * extent.max(1.0);
                for p in &pts {
                    assert!(
                        p[0] >= lo[0] - slack && p[0] <= hi[0] + slack && p[1] >= lo[1] - slack && p[1] <= hi[1] + slack,
                        "{shape:?}: {p:?} outside {lo:?} {hi:?}"
                    );
                }
            }
        }
    }
}
