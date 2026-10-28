//! The two render pipelines and the uniforms they read.

use super::shaders;
use super::types::{Capsule, Globals, Panel, Vertex, Xf};

fn premultiplied_blend() -> wgpu::BlendState {
    let component = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState { color: component, alpha: component }
}

/// A bind group layout with one uniform buffer visible to the vertex stage.
fn uniform_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    })
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bind_uniform(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
    })
}

fn shader(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(source.into()) })
}

/// Textured quads: the paper and the page tiles.
pub(super) struct QuadPipeline {
    pub pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    pub globals_bind: wgpu::BindGroup,
    pub texture_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}

impl QuadPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = shader(device, "inkpdf", shaders::QUADS);
        let globals = uniform_buffer(device, "globals", std::mem::size_of::<Globals>());
        let globals_layout = uniform_layout(device, "globals layout");
        let globals_bind = bind_uniform(device, "globals", &globals_layout, &globals);
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("inkpdf pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("inkpdf quads"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(premultiplied_blend()), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, globals, globals_bind, texture_layout, sampler }
    }

    pub fn set_viewport(&self, queue: &wgpu::Queue, size: [f32; 2]) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&Globals { size, pad: [0.0; 2] }));
    }
}

/// A capsule transform and the bytes last written to it: the screen transform changes with the
/// window, not per frame, so unchanged ones are not written again.
struct Transform {
    buffer: wgpu::Buffer,
    pub bind: wgpu::BindGroup,
    last: [u8; 32],
}

impl Transform {
    fn new(device: &wgpu::Device, label: &str, layout: &wgpu::BindGroupLayout) -> Self {
        let buffer = uniform_buffer(device, label, std::mem::size_of::<Xf>());
        let bind = bind_uniform(device, label, layout, &buffer);
        Self { buffer, bind, last: [0; 32] }
    }

    fn update(&mut self, queue: &wgpu::Queue, xf: Xf) {
        let bytes: [u8; 32] = bytemuck::cast(xf);
        if bytes != self.last {
            queue.write_buffer(&self.buffer, 0, &bytes);
            self.last = bytes;
        }
    }
}

/// Line work: one pipeline, two transforms (document space and screen pixels).
pub(super) struct CapsulePipeline {
    pub pipeline: wgpu::RenderPipeline,
    /// The layout of the transform bind groups, shared with the panel pipeline.
    pub xf_layout: wgpu::BindGroupLayout,
    doc: Transform,
    screen: Transform,
}

impl CapsulePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = shader(device, "inkpdf capsules", shaders::CAPSULES);
        let xf_layout = uniform_layout(device, "transform layout");
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("capsule layout"),
            bind_group_layouts: &[Some(&xf_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("inkpdf capsules"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_capsule"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: Capsule::SIZE,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Unorm8x4],
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_capsule"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(premultiplied_blend()), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let (doc, screen) = (Transform::new(device, "document transform", &xf_layout), Transform::new(device, "screen transform", &xf_layout));
        Self { pipeline, xf_layout, doc, screen }
    }

    pub fn set_transforms(&mut self, queue: &wgpu::Queue, scroll: [f32; 2], zoom: f32, viewport: [f32; 2]) {
        self.doc.update(queue, Xf::new(scroll, zoom, viewport));
        self.screen.update(queue, Xf::new([0.0; 2], 1.0, viewport));
    }

    pub fn doc_bind(&self) -> &wgpu::BindGroup {
        &self.doc.bind
    }

    pub fn screen_bind(&self) -> &wgpu::BindGroup {
        &self.screen.bind
    }
}

/// Rounded rectangles in screen pixels, drawn with the capsule pipeline's screen transform.
pub(super) struct PanelPipeline {
    pub pipeline: wgpu::RenderPipeline,
}

impl PanelPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, xf_layout: &wgpu::BindGroupLayout) -> Self {
        let module = shader(device, "inkpdf panels", shaders::PANELS);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panel layout"),
            bind_group_layouts: &[Some(xf_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("inkpdf panels"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_panel"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Panel>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Unorm8x4],
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_panel"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(premultiplied_blend()), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline }
    }
}
