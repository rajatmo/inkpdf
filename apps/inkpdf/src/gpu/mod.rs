//! The wgpu side: a window surface, a textured-quad pipeline for the pages and a capsule
//! pipeline for ink and interface lines.
//!
//! Quads are in device pixels and blend premultiplied alpha, which is what the page renderer
//! produces. Capsules are drawn in document space (ink) or in pixels (toolbar, cursors); panels
//! (rounded rectangles, shadows) in pixels.

mod buffers;
mod device;
mod pipelines;
mod shaders;
mod textures;
mod types;

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use winit::window::Window;

use crate::tiles::{Key, Residency};
use buffers::{GrowBuffer, InstanceBuffer};
use pipelines::{CapsulePipeline, PanelPipeline, QuadPipeline};
use textures::PageTexture;
use types::Vertex;

pub use device::{GpuChoice, list_gpus};
pub use types::{Capsule, Drawn, InkScene, InkThumb, Panel, Quad, Tex};

/// Quads are drawn as two triangles each.
const VERTICES_PER_QUAD: usize = 6;

/// A stretch of quads in the vertex buffer that share a texture.
type Run = (Tex, Range<u32>);

pub struct Gpu {
    window: Arc<Window>,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    needs_reconfigure: bool,
    quads: QuadPipeline,
    capsules: CapsulePipeline,
    panels: PanelPipeline,
    white: PageTexture,
    vertices: GrowBuffer,
    /// Committed ink, kept between frames and rewritten only when the ink changes.
    ink: InstanceBuffer,
    /// The toolbar and pop-ups, kept between frames and rewritten only when they change: the
    /// rounded shapes first, then the line work on top of them.
    ui_panels: InstanceBuffer,
    ui_lines: InstanceBuffer,
    /// The per-frame overlays.
    overlays: GrowBuffer,
    textures: HashMap<Key, PageTexture>,
    /// Bitmaps of typed text, by the key they were uploaded under.
    texts: HashMap<u64, PageTexture>,
    pub residency: Residency,
    pub adapter_name: String,
    pub backend: String,
    pub device_type: String,
    /// How long the last frame waited to be given a surface texture (the vsync wait), in µs.
    pub last_wait_us: f64,
}

/// How many overlay capsules a frame has in each space.
#[derive(Clone, Copy)]
struct OverlayCounts {
    doc: u32,
    screen: u32,
}

impl OverlayCounts {
    fn total(self) -> u32 {
        self.doc + self.screen
    }
}

impl Gpu {
    pub fn new(window: Arc<Window>, choice: &GpuChoice) -> Result<Self, String> {
        pollster::block_on(Self::create(window, choice))
    }

    async fn create(window: Arc<Window>, choice: &GpuChoice) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = device::new_instance();
        let surface = instance.create_surface(window.clone()).map_err(|e| format!("no window surface: {e}"))?;
        let adapter = device::pick_adapter(&instance, &surface, choice).await?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("inkpdf device"),
                required_limits: wgpu::Limits { max_texture_dimension_2d: 8192, ..wgpu::Limits::default() },
                ..Default::default()
            })
            .await
            .map_err(|e| format!("could not open the GPU: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format =
            caps.formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| caps.formats.first().copied()).ok_or("the surface has no formats")?;
        let mut config =
            surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).ok_or("the surface is not supported by this GPU")?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        // One frame in flight: the least latency between an input and the pixels showing it.
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);

        let quads = QuadPipeline::new(&device, format);
        let capsules = CapsulePipeline::new(&device, format);
        let panels = PanelPipeline::new(&device, format, &capsules.xf_layout);
        let white = PageTexture::new(&device, &queue, &quads.texture_layout, &quads.sampler, 1, 1, &[255, 255, 255, 255]);
        let vertices = GrowBuffer::new(&device, "quads", 4096 * std::mem::size_of::<Vertex>() as u64);
        let ink = InstanceBuffer::new::<Capsule>(&device, "committed ink", 4096);
        let ui_panels = InstanceBuffer::new::<Panel>(&device, "interface panels", 64);
        let ui_lines = InstanceBuffer::new::<Capsule>(&device, "interface lines", 512);
        let overlays = GrowBuffer::new(&device, "overlays", 1024 * Capsule::SIZE);

        let gpu = Self {
            window,
            instance,
            surface,
            device,
            queue,
            config,
            needs_reconfigure: false,
            quads,
            capsules,
            panels,
            white,
            vertices,
            ink,
            ui_panels,
            ui_lines,
            overlays,
            textures: HashMap::new(),
            texts: HashMap::new(),
            residency: Residency::default(),
            adapter_name: info.name,
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            last_wait_us: 0.0,
        };
        gpu.write_globals();
        Ok(gpu)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.write_globals();
    }

    fn viewport(&self) -> [f32; 2] {
        [self.config.width as f32, self.config.height as f32]
    }

    fn write_globals(&self) {
        self.quads.set_viewport(&self.queue, self.viewport());
    }

    /// Replace the committed ink. Only called when the ink changes; every frame draws from it.
    pub fn set_committed(&mut self, capsules: &[Capsule]) {
        self.ink.set(&self.device, &self.queue, capsules);
    }

    /// How many capsules of committed ink the buffer holds.
    pub fn committed_len(&self) -> u32 {
        self.ink.len()
    }

    /// Add capsules after the committed ink. `false` when they do not fit; replace it whole then.
    pub fn append_committed(&mut self, capsules: &[Capsule]) -> bool {
        self.ink.append(&self.queue, capsules)
    }

    /// Replace the toolbar and pop-ups (pixels). Only called when they change.
    pub fn set_ui(&mut self, panels: &[Panel], lines: &[Capsule]) {
        self.ui_panels.set(&self.device, &self.queue, panels);
        self.ui_lines.set(&self.device, &self.queue, lines);
    }

    pub fn has(&self, key: Key) -> bool {
        self.textures.contains_key(&key)
    }

    pub fn texture_size(&self, key: Key) -> Option<[u32; 2]> {
        self.textures.get(&key).map(|t| t.size)
    }

    pub fn keys(&self) -> impl Iterator<Item = Key> + '_ {
        self.textures.keys().copied()
    }

    /// Upload premultiplied RGBA8 pixels as the texture for `key`. Mismatched sizes are ignored.
    pub fn upload(&mut self, key: Key, width: u32, height: u32, rgba: &[u8], frame: u64) {
        let limit = self.device.limits().max_texture_dimension_2d;
        let expected = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || width > limit || height > limit || expected != Some(rgba.len()) {
            return;
        }
        let tex = PageTexture::new(&self.device, &self.queue, &self.quads.texture_layout, &self.quads.sampler, width, height, rgba);
        self.textures.insert(key, tex);
        self.residency.insert(key, rgba.len(), frame);
    }

    /// Upload premultiplied RGBA8 pixels of typed text under `key`.
    pub fn upload_text(&mut self, key: u64, width: u32, height: u32, rgba: &[u8]) {
        let limit = self.device.limits().max_texture_dimension_2d;
        let expected = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || width > limit || height > limit || expected != Some(rgba.len()) {
            return;
        }
        let tex = PageTexture::new(&self.device, &self.queue, &self.quads.texture_layout, &self.quads.sampler, width, height, rgba);
        self.texts.insert(key, tex);
    }

    pub fn text_size(&self, key: u64) -> Option<[u32; 2]> {
        self.texts.get(&key).map(|t| t.size)
    }

    /// Forget the text bitmaps not in `used`, once there are more than `keep` of them.
    pub fn trim_texts(&mut self, used: &std::collections::HashSet<u64>, keep: usize) {
        if self.texts.len() > keep {
            self.texts.retain(|key, _| used.contains(key));
        }
    }

    pub fn touch(&mut self, key: Key, frame: u64) {
        self.residency.touch(key, frame);
    }

    /// Free least-recently-drawn textures beyond `budget` bytes.
    pub fn evict(&mut self, budget: usize, frame: u64) {
        for key in self.residency.evict(budget, frame) {
            self.textures.remove(&key);
        }
    }

    pub fn clear_textures(&mut self) {
        self.textures.clear();
        self.texts.clear();
        self.residency.clear();
    }

    /// Get the next frame to draw into. This is where a frame waits for the display's refresh, so
    /// the app calls it *first* and reads the pen only afterwards: the ink it draws then carries
    /// the newest samples, not ones a whole refresh old. `None` when nothing can be drawn now
    /// (minimised, occluded, surface being rebuilt).
    pub fn acquire(&mut self) -> Option<wgpu::SurfaceTexture> {
        if self.needs_reconfigure {
            self.surface.configure(&self.device, &self.config);
            self.needs_reconfigure = false;
        }
        let waited = std::time::Instant::now();
        let acquired = self.surface.get_current_texture();
        self.last_wait_us = waited.elapsed().as_secs_f64() * 1e6;
        match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.needs_reconfigure = true;
                Some(frame)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                if let Ok(surface) = self.instance.create_surface(self.window.clone()) {
                    self.surface = surface;
                }
                self.needs_reconfigure = true;
                None
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.needs_reconfigure = true;
                None
            }
            _ => None,
        }
    }

    /// Draw `quads` in order over `clear`, then the ink and overlays, then the interface (`ui_quads` are its pictures and
    /// text, between its panels and its lines), into `frame` and present.
    pub fn draw(&mut self, frame: Option<wgpu::SurfaceTexture>, quads: &[Quad], ui_quads: &[Quad], ink: &InkScene, clear: [f64; 4]) -> Drawn {
        let Some(frame) = frame else { return Drawn::Skipped };
        let (runs, ui_runs) = self.upload_quads(quads, ui_quads);
        let counts = self.upload_overlays(ink);
        self.capsules.set_transforms(&self.queue, ink.scroll, ink.zoom, self.viewport());
        let viewport = self.viewport();
        for (i, t) in ink.thumbs.iter().enumerate() {
            self.capsules.set_thumb(&self.device, &self.queue, i, types::Xf::new(t.origin, t.scale, viewport));
        }

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("inkpdf frame") });
        self.record(&mut encoder, &view, clear, (&runs, &ui_runs), ink, counts);
        self.queue.submit(Some(encoder.finish()));
        self.window.pre_present_notify();
        self.queue.present(frame);
        Drawn::Presented
    }

    /// Write the quads' vertices and group them into runs that share a texture: those of the pages, and those of the
    /// interface after them.
    fn upload_quads(&mut self, quads: &[Quad], ui_quads: &[Quad]) -> (Vec<Run>, Vec<Run>) {
        let mut verts: Vec<Vertex> = Vec::with_capacity((quads.len() + ui_quads.len()) * VERTICES_PER_QUAD);
        let main = self.quad_runs(quads, &mut verts);
        let ui = self.quad_runs(ui_quads, &mut verts);
        self.vertices.reserve(&self.device, (verts.len() * std::mem::size_of::<Vertex>()) as u64);
        self.vertices.write(&self.queue, 0, bytemuck::cast_slice(&verts));
        (main, ui)
    }

    /// Add `quads` that have a texture to `verts`, and say which ranges of them share one.
    fn quad_runs(&self, quads: &[Quad], verts: &mut Vec<Vertex>) -> Vec<(Tex, Range<u32>)> {
        let mut runs: Vec<(Tex, Range<u32>)> = Vec::new();
        for q in quads {
            let missing = match q.tex {
                Tex::White => false,
                Tex::Page(key) => !self.textures.contains_key(&key),
                Tex::Text(key) => !self.texts.contains_key(&key),
            };
            if missing {
                continue;
            }
            let start = verts.len() as u32;
            verts.extend_from_slice(&quad_vertices(q));
            let end = verts.len() as u32;
            match runs.last_mut() {
                Some((tex, range)) if *tex == q.tex => range.end = end,
                _ => runs.push((q.tex, start..end)),
            }
        }
        runs
    }

    /// Overlays: document-space ones first, then screen-space, in one buffer.
    fn upload_overlays(&mut self, ink: &InkScene) -> OverlayCounts {
        let counts = OverlayCounts { doc: ink.overlay_doc.len() as u32, screen: ink.overlay_screen.len() as u32 };
        self.overlays.reserve(&self.device, u64::from(counts.total()) * Capsule::SIZE);
        self.overlays.write(&self.queue, 0, bytemuck::cast_slice(ink.overlay_doc));
        self.overlays.write(&self.queue, u64::from(counts.doc) * Capsule::SIZE, bytemuck::cast_slice(ink.overlay_screen));
        counts
    }

    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear: [f64; 4],
        runs: (&[Run], &[Run]),
        ink: &InkScene,
        counts: OverlayCounts,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("inkpdf pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: clear[3] }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.record_pages(&mut pass, runs.0);
        self.record_lines(&mut pass, ink, counts, runs.1);
    }

    fn record_pages(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[(Tex, Range<u32>)]) {
        pass.set_pipeline(&self.quads.pipeline);
        pass.set_bind_group(0, &self.quads.globals_bind, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice());
        for (tex, range) in runs {
            let bind = match tex {
                Tex::White => &self.white.bind,
                Tex::Page(key) => match self.textures.get(key) {
                    Some(t) => &t.bind,
                    None => continue,
                },
                Tex::Text(key) => match self.texts.get(key) {
                    Some(t) => &t.bind,
                    None => continue,
                },
            };
            pass.set_bind_group(1, bind, &[]);
            pass.draw(range.clone(), 0..1);
        }
    }

    /// Committed ink, then the document and screen overlays, then the interface on top: its
    /// panels, then its lines.
    fn record_lines(&self, pass: &mut wgpu::RenderPass<'_>, ink: &InkScene, counts: OverlayCounts, ui_runs: &[(Tex, Range<u32>)]) {
        if ink.committed.is_empty() && counts.total() == 0 && self.ui_lines.len() == 0 && self.ui_panels.len() == 0 && ui_runs.is_empty() {
            return;
        }
        pass.set_pipeline(&self.capsules.pipeline);
        if !ink.committed.is_empty() && self.ink.len() > 0 {
            pass.set_bind_group(0, self.capsules.doc_bind(), &[]);
            pass.set_vertex_buffer(0, self.ink.slice());
            for r in ink.committed {
                let end = r.end.min(self.ink.len());
                if r.start < end {
                    pass.draw(0..4, r.start..end);
                }
            }
        }
        if counts.total() > 0 {
            pass.set_vertex_buffer(0, self.overlays.slice());
            if counts.doc > 0 {
                pass.set_bind_group(0, self.capsules.doc_bind(), &[]);
                pass.draw(0..4, 0..counts.doc);
            }
            if counts.screen > 0 {
                pass.set_bind_group(0, self.capsules.screen_bind(), &[]);
                pass.draw(0..4, counts.doc..counts.total());
            }
        }
        if self.ui_panels.len() > 0 {
            pass.set_pipeline(&self.panels.pipeline);
            pass.set_bind_group(0, self.capsules.screen_bind(), &[]);
            pass.set_vertex_buffer(0, self.ui_panels.slice());
            pass.draw(0..4, 0..self.ui_panels.len());
            pass.set_pipeline(&self.capsules.pipeline);
        }
        // The interface's pictures and text, over its panels.
        if !ui_runs.is_empty() {
            self.record_pages(pass, ui_runs);
            pass.set_pipeline(&self.capsules.pipeline);
        }
        // The ink in the pictures of pages, each in its own place and no further than its picture reaches.
        if !ink.thumbs.is_empty() && self.ink.len() > 0 {
            pass.set_vertex_buffer(0, self.ink.slice());
            let [vw, vh] = [self.config.width, self.config.height];
            for (i, t) in ink.thumbs.iter().enumerate() {
                let (Some(bind), [x, y, w, h]) = (self.capsules.thumb_bind(i), t.clip) else { continue };
                if x >= vw || y >= vh || w == 0 || h == 0 {
                    continue;
                }
                pass.set_bind_group(0, bind, &[]);
                pass.set_scissor_rect(x, y, w.min(vw - x), h.min(vh - y));
                for r in &t.ranges {
                    let end = r.end.min(self.ink.len());
                    if r.start < end {
                        pass.draw(0..4, r.start..end);
                    }
                }
            }
            pass.set_scissor_rect(0, 0, vw, vh);
        }
        if self.ui_lines.len() > 0 {
            pass.set_bind_group(0, self.capsules.screen_bind(), &[]);
            pass.set_vertex_buffer(0, self.ui_lines.slice());
            pass.draw(0..4, 0..self.ui_lines.len());
        }
    }
}

fn quad_vertices(q: &Quad) -> [Vertex; VERTICES_PER_QUAD] {
    let [x0, y0, x1, y1] = q.rect;
    let [u0, v0, u1, v1] = q.uv;
    let c = q.color;
    let v = |x: f32, y: f32, u: f32, w: f32| Vertex { pos: [x, y], uv: [u, w], color: c };
    [v(x0, y0, u0, v0), v(x1, y0, u1, v0), v(x0, y1, u0, v1), v(x1, y0, u1, v0), v(x1, y1, u1, v1), v(x0, y1, u0, v1)]
}
