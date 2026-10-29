//! What the interface and the overlays are drawn with, as plain data: round-ended lines (capsules) and rounded
//! rectangles (panels). The toolbar and the tools describe what they want drawn in these, and the GPU layer draws them, so
//! that neither has to know about the other. (The layout is the one the GPU reads, so a frame's buffer is these values,
//! uploaded as they are.)

/// A change of place, size and opacity for a group of things drawn: a scale about `origin`, then a shift, and a fade.
/// (How an interface comes in, goes out, grows from a button or slides: the drawing code draws the thing as it rests and
/// this is put over it.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub origin: [f32; 2],
    pub scale: f32,
    pub shift: [f32; 2],
    pub alpha: f32,
}

impl Xform {
    /// Only fading.
    pub fn fade(alpha: f32) -> Self {
        Self { origin: [0.0; 2], scale: 1.0, shift: [0.0; 2], alpha }
    }

    /// Growing from (or shrinking to) `scale` about `origin`, and fading with it.
    pub fn grow(origin: [f32; 2], scale: f32, alpha: f32) -> Self {
        Self { origin, scale, shift: [0.0; 2], alpha }
    }

    /// Sliding by `shift`, and fading.
    pub fn slide(shift: [f32; 2], alpha: f32) -> Self {
        Self { origin: [0.0; 2], scale: 1.0, shift, alpha }
    }

    pub fn point(&self, p: [f32; 2]) -> [f32; 2] {
        [self.origin[0] + (p[0] - self.origin[0]) * self.scale + self.shift[0], self.origin[1] + (p[1] - self.origin[1]) * self.scale + self.shift[1]]
    }

    pub fn rect(&self, r: [f32; 4]) -> [f32; 4] {
        let (a, b) = (self.point([r[0], r[1]]), self.point([r[2], r[3]]));
        [a[0], a[1], b[0], b[1]]
    }

    /// `colour` as opaque as it is, times the fade.
    pub fn colour(&self, c: [u8; 4]) -> [u8; 4] {
        [c[0], c[1], c[2], (f32::from(c[3]) * self.alpha.clamp(0.0, 1.0)).round() as u8]
    }

    /// Nothing to do: drawn as it rests.
    pub fn is_identity(&self) -> bool {
        self.scale == 1.0 && self.shift == [0.0; 2] && self.alpha >= 1.0
    }
}

/// A round-ended line segment: `width` is its thickness, in the units of the space it is drawn
/// in (points for ink, pixels for the interface). `a == b` is a dot.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Capsule {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub width: f32,
    /// Straight (not premultiplied) RGBA.
    pub color: [u8; 4],
}

impl Capsule {
    pub const SIZE: u64 = std::mem::size_of::<Capsule>() as u64;

    pub fn new(a: [f32; 2], b: [f32; 2], width: f32, color: [u8; 4]) -> Self {
        Self { a, b, width, color }
    }

    pub fn transformed(self, x: &Xform) -> Self {
        Self { a: x.point(self.a), b: x.point(self.b), width: self.width * x.scale, color: x.colour(self.color) }
    }
}

/// A rounded rectangle in pixels. `soft` is how many pixels its edge fades over (1 is a crisp
/// edge; more makes a shadow).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Panel {
    /// `x0, y0, x1, y1`.
    pub rect: [f32; 4],
    pub radius: f32,
    pub soft: f32,
    /// Straight (not premultiplied) RGBA.
    pub color: [u8; 4],
    pad: f32,
}

impl Panel {
    pub fn new(rect: [f32; 4], radius: f32, soft: f32, color: [u8; 4]) -> Self {
        Self { rect, radius, soft, color, pad: 0.0 }
    }

    /// A crisp-edged panel.
    pub fn solid(rect: [f32; 4], radius: f32, color: [u8; 4]) -> Self {
        Self::new(rect, radius, 1.0, color)
    }

    pub fn transformed(self, x: &Xform) -> Self {
        // (An edge's softness is a size too: a shadow grows with what it is under.)
        Self { rect: x.rect(self.rect), radius: self.radius * x.scale, soft: (self.soft * x.scale).max(1.0).min(self.soft.max(1.0)), color: x.colour(self.color), pad: self.pad }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transform_scales_about_its_origin_then_shifts_and_fades() {
        let x = Xform { origin: [100.0, 100.0], scale: 0.5, shift: [10.0, -4.0], alpha: 0.5 };
        assert_eq!(x.point([100.0, 100.0]), [110.0, 96.0], "the origin stays (but for the shift)");
        assert_eq!(x.point([200.0, 140.0]), [160.0, 116.0]);
        assert_eq!(x.rect([100.0, 100.0, 200.0, 140.0]), [110.0, 96.0, 160.0, 116.0]);
        assert_eq!(x.colour([10, 20, 30, 200]), [10, 20, 30, 100]);
        let line = Capsule::new([100.0, 100.0], [200.0, 100.0], 8.0, [1, 2, 3, 255]).transformed(&x);
        assert_eq!((line.width, line.color[3]), (4.0, 128));
        let panel = Panel::new([100.0, 100.0, 200.0, 140.0], 10.0, 1.0, [1, 2, 3, 255]).transformed(&x);
        assert_eq!((panel.radius, panel.color[3]), (5.0, 128));
        assert!(Xform::fade(1.0).is_identity() && !Xform::fade(0.9).is_identity() && !Xform::slide([1.0, 0.0], 1.0).is_identity());
    }

    #[test]
    fn the_identity_changes_nothing_and_a_fade_beyond_its_range_is_kept_within_it() {
        let x = Xform::fade(1.0);
        let p = Panel::new([1.0, 2.0, 30.0, 40.0], 5.0, 3.0, [9, 9, 9, 200]);
        assert_eq!(p.transformed(&x), p);
        assert_eq!(Xform::fade(7.0).colour([0, 0, 0, 200])[3], 200);
        assert_eq!(Xform::fade(-1.0).colour([0, 0, 0, 200])[3], 0);
    }
}
