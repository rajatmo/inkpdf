//! What the interface and the overlays are drawn with, as plain data: round-ended lines (capsules) and rounded
//! rectangles (panels). The toolbar and the tools describe what they want drawn in these, and the GPU layer draws them, so
//! that neither has to know about the other. (The layout is the one the GPU reads, so a frame's buffer is these values,
//! uploaded as they are.)

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
}
