//! What a tool needs from the renderer: lines. Tools paint into a [`Painter`] and never see the GPU; the
//! shell implements it. The helpers here draw the shapes the tools share (polylines, dashes, squares).

use crate::ink::P;

pub const ACCENT: [u8; 4] = [64, 132, 255, 255];
/// Length in points of the short pieces a stroke's curve is drawn in.
pub const CURVE_STEP: f32 = 0.75;

/// A sink for round-ended line segments, in document space (they follow the page as it scrolls and zooms)
/// or in screen pixels (they stay where they are).
pub trait Painter {
    fn doc_line(&mut self, a: P, b: P, width: f32, rgba: [u8; 4]);
    fn screen_line(&mut self, a: P, b: P, width: f32, rgba: [u8; 4]);
    /// The pen clock, in milliseconds, for things that fade.
    fn now_ms(&self) -> f64;
}

/// A dot or a connected line through `pts`.
pub fn polyline(out: &mut dyn Painter, pts: &[P], width: f32, rgba: [u8; 4]) {
    match pts {
        [] => {}
        [p] => out.doc_line(*p, *p, width, rgba),
        _ => pts.windows(2).for_each(|w| out.doc_line(w[0], w[1], width, rgba)),
    }
}

/// The line `a`–`b` as dashes `dash` long with `gap` between them.
pub fn dashed(out: &mut dyn Painter, a: P, b: P, width: f32, rgba: [u8; 4], dash: f32, gap: f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if !(len.is_finite() && len > 0.0 && dash > 0.0) {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut at = 0.0;
    // (Bounded, so a shape the size of the universe cannot make a million dashes.)
    for _ in 0..4096 {
        if at >= len {
            break;
        }
        let end = (at + dash).min(len);
        out.doc_line([a[0] + ux * at, a[1] + uy * at], [a[0] + ux * end, a[1] + uy * end], width, rgba);
        at = end + gap;
    }
}

/// A filled square of half-side `half` about `centre`, drawn as thin horizontal lines `line` apart.
pub fn fill_square(out: &mut dyn Painter, centre: P, half: f32, line: f32, rgba: [u8; 4]) {
    let rows = ((2.0 * half / line).ceil() as usize).max(1);
    for i in 0..=rows {
        let y = centre[1] - half + 2.0 * half * i as f32 / rows as f32;
        out.doc_line([centre[0] - half, y], [centre[0] + half, y], line * 1.05, rgba);
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A painter that keeps what it was given, for tests of tools' overlays.
    #[derive(Default)]
    pub struct Recorder {
        pub doc: Vec<(P, P, f32, [u8; 4])>,
        pub screen: Vec<(P, P, f32, [u8; 4])>,
        pub now: f64,
    }

    impl Painter for Recorder {
        fn doc_line(&mut self, a: P, b: P, width: f32, rgba: [u8; 4]) {
            self.doc.push((a, b, width, rgba));
        }

        fn screen_line(&mut self, a: P, b: P, width: f32, rgba: [u8; 4]) {
            self.screen.push((a, b, width, rgba));
        }

        fn now_ms(&self) -> f64 {
            self.now
        }
    }

    #[test]
    fn dashes_cover_the_line_and_never_run_away() {
        let mut out = Recorder::default();
        dashed(&mut out, [0.0, 0.0], [100.0, 0.0], 1.0, [0; 4], 6.0, 4.0);
        assert_eq!(out.doc.len(), 10);
        assert_eq!((out.doc[0].0, out.doc[9].1), ([0.0, 0.0], [96.0, 0.0]));
        let mut many = Recorder::default();
        dashed(&mut many, [0.0, 0.0], [1.0e9, 0.0], 1.0, [0; 4], 6.0, 4.0);
        assert!(many.doc.len() <= 4096, "bounded");
        let mut none = Recorder::default();
        dashed(&mut none, [0.0, 0.0], [f32::NAN, 0.0], 1.0, [0; 4], 6.0, 4.0);
        dashed(&mut none, [1.0, 1.0], [1.0, 1.0], 1.0, [0; 4], 6.0, 4.0);
        assert!(none.doc.is_empty());
    }
}
