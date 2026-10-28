//! The data the app hands to the GPU layer each frame, and the vertex layouts behind it.

use std::ops::Range;

use crate::tiles::Key;

pub use crate::scene::{Capsule, Panel};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// The capsule shader's transform: `(point - origin) * scale`, then to clip space by `viewport`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Xf {
    pub origin: [f32; 2],
    pub scale: f32,
    pub pad: f32,
    pub viewport: [f32; 2],
    pub pad2: [f32; 2],
}

impl Xf {
    pub fn new(origin: [f32; 2], scale: f32, viewport: [f32; 2]) -> Self {
        Self { origin, scale, pad: 0.0, viewport, pad2: [0.0; 2] }
    }
}

/// The quad shader's viewport size.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Globals {
    pub size: [f32; 2],
    pub pad: [f32; 2],
}

/// The line work of a frame, drawn over the pages.
pub struct InkScene<'a> {
    /// Document point at the viewport's top-left corner, and pixels per point.
    pub scroll: [f32; 2],
    pub zoom: f32,
    /// Ranges of the committed-ink buffer (see [`super::Gpu::set_committed`]) to draw.
    pub committed: &'a [Range<u32>],
    /// In document space, over the committed ink: the stroke being drawn, selection, lasso.
    pub overlay_doc: &'a [Capsule],
    /// In pixels, over everything: the toolbar and cursors.
    pub overlay_screen: &'a [Capsule],
}

/// What a quad is textured with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tex {
    /// A single white texel: a solid colour.
    White,
    Page(Key),
    /// Typed text drawn to a bitmap (see [`super::Gpu::upload_text`]), by the key it was uploaded under.
    Text(u64),
}

/// A textured rectangle in device pixels.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub tex: Tex,
    /// `x0, y0, x1, y1`.
    pub rect: [f32; 4],
    /// `u0, v0, u1, v1`.
    pub uv: [f32; 4],
    /// Premultiplied colour the texture is multiplied by.
    pub color: [f32; 4],
}

impl Quad {
    pub fn solid(rect: [f32; 4], color: [f32; 4]) -> Self {
        Self { tex: Tex::White, rect, uv: [0.0, 0.0, 1.0, 1.0], color }
    }

    pub fn page(key: Key, rect: [f32; 4]) -> Self {
        Self { tex: Tex::Page(key), rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4] }
    }

    pub fn text(key: u64, rect: [f32; 4]) -> Self {
        Self { tex: Tex::Text(key), rect, uv: [0.0, 0.0, 1.0, 1.0], color: [1.0; 4] }
    }
}

/// How a frame ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawn {
    Presented,
    /// No frame could be drawn now (minimised, occluded, surface being rebuilt); try again later.
    Skipped,
}
