//! WGSL for the two pipelines.

/// Textured quads in device pixels (pages, the paper, solid fills).
pub(super) const QUADS: &str = "
struct Globals { size: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var t: texture_2d<f32>;
@group(1) @binding(1) var s: sampler;

struct VIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.pos = vec4<f32>(v.pos.x / g.size.x * 2.0 - 1.0, 1.0 - v.pos.y / g.size.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(t, s, i.uv) * i.color;
}
";

/// Ink and interface lines: each instance is a round-ended segment, expanded to a quad in the
/// vertex shader and anti-aliased in the fragment shader from its distance to the segment. One
/// primitive draws strokes, dots, selection boxes and toolbar icons.
pub(super) const CAPSULES: &str = "
struct Xf { origin: vec2<f32>, scale: f32, pad: f32, viewport: vec2<f32>, pad2: vec2<f32> };
@group(0) @binding(0) var<uniform> xf: Xf;

struct VIn {
    @builtin(vertex_index) vi: u32,
    @location(0) a: vec2<f32>,
    @location(1) b: vec2<f32>,
    @location(2) width: f32,
    @location(3) color: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) sa: vec2<f32>,
    @location(1) @interpolate(flat) sb: vec2<f32>,
    @location(2) @interpolate(flat) hw: f32,
    @location(3) @interpolate(flat) color: vec4<f32>,
};

@vertex
fn vs_capsule(i: VIn) -> VOut {
    let sa = (i.a - xf.origin) * xf.scale;
    let sb = (i.b - xf.origin) * xf.scale;
    // Never thinner than one pixel, so a hairline stays visible when zoomed out.
    let hw = max(i.width * xf.scale * 0.5, 0.5);
    let d = sb - sa;
    let len = length(d);
    var dir = vec2<f32>(1.0, 0.0);
    if (len > 0.0001) { dir = d / len; }
    let nrm = vec2<f32>(-dir.y, dir.x);
    let ext = hw + 1.5;
    let side = select(-1.0, 1.0, (i.vi & 1u) == 1u);
    var p: vec2<f32>;
    if (i.vi < 2u) { p = sa - dir * ext + nrm * ext * side; } else { p = sb + dir * ext + nrm * ext * side; }
    var o: VOut;
    o.pos = vec4<f32>(p.x / xf.viewport.x * 2.0 - 1.0, 1.0 - p.y / xf.viewport.y * 2.0, 0.0, 1.0);
    o.sa = sa;
    o.sb = sb;
    o.hw = hw;
    o.color = i.color;
    return o;
}

@fragment
fn fs_capsule(i: VOut) -> @location(0) vec4<f32> {
    let p = i.pos.xy;
    let ab = i.sb - i.sa;
    let l2 = dot(ab, ab);
    var t = 0.0;
    if (l2 > 0.00000001) { t = clamp(dot(p - i.sa, ab) / l2, 0.0, 1.0); }
    let dist = length(p - (i.sa + ab * t));
    let cover = clamp(i.hw + 0.5 - dist, 0.0, 1.0);
    let a = i.color.a * cover;
    return vec4<f32>(i.color.rgb * a, a);
}
";

/// Rounded rectangles in pixels: the toolbar, pop-ups, button highlights and their shadows. The
/// edge is a signed distance, so corners are exact at any size, and `soft` widens it into a blur.
pub(super) const PANELS: &str = "
struct Xf { origin: vec2<f32>, scale: f32, pad: f32, viewport: vec2<f32>, pad2: vec2<f32> };
@group(0) @binding(0) var<uniform> xf: Xf;

struct VIn {
    @builtin(vertex_index) vi: u32,
    @location(0) rect: vec4<f32>,
    @location(1) shape: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) rect: vec4<f32>,
    @location(1) @interpolate(flat) shape: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
};

@vertex
fn vs_panel(i: VIn) -> VOut {
    let m = max(i.shape.y, 1.0) * 0.5 + 1.0;
    let x = select(i.rect.x - m, i.rect.z + m, (i.vi & 1u) == 1u);
    let y = select(i.rect.y - m, i.rect.w + m, i.vi >= 2u);
    var o: VOut;
    o.pos = vec4<f32>(x / xf.viewport.x * 2.0 - 1.0, 1.0 - y / xf.viewport.y * 2.0, 0.0, 1.0);
    o.rect = i.rect;
    o.shape = i.shape;
    o.color = i.color;
    return o;
}

@fragment
fn fs_panel(i: VOut) -> @location(0) vec4<f32> {
    let centre = (i.rect.xy + i.rect.zw) * 0.5;
    let half = (i.rect.zw - i.rect.xy) * 0.5;
    let r = min(i.shape.x, min(half.x, half.y));
    let q = abs(i.pos.xy - centre) - (half - vec2<f32>(r));
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let s = max(i.shape.y, 1.0);
    let a = i.color.a * (1.0 - smoothstep(-s * 0.5, s * 0.5, d));
    return vec4<f32>(i.color.rgb * a, a);
}
";
