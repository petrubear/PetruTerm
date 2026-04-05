use std::mem;

/// WGSL shader for GPU-side SDF rounded rectangle rendering (TD-013).
const ROUNDED_RECT_SHADER: &str = r#"
struct RectUniforms {
    viewport_size: vec2<f32>,
    _pad:          vec2<f32>,
}

struct RectInstance {
    @location(0) rect:   vec4<f32>,   // x, y, w, h in physical pixels
    @location(1) color:  vec4<f32>,   // rgba (straight alpha)
    @location(2) radius: f32,
}

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_pos:  vec2<f32>,  // pixel position within the rect
    @location(1) half_size:  vec2<f32>,  // half of rect size
    @location(2) color:      vec4<f32>,
    @location(3) radius:     f32,
}

@group(0) @binding(0) var<uniform> u: RectUniforms;

const QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: RectInstance) -> VertexOut {
    let q = QUAD[vi];

    // Pixel position of this vertex
    let px = inst.rect.xy + q * inst.rect.zw;

    // NDC conversion: top-left = (-1, 1), bottom-right = (1, -1)
    let ndc = px * vec2(2.0, -2.0) / u.viewport_size + vec2(-1.0, 1.0);

    let half = inst.rect.zw * 0.5;

    var out: VertexOut;
    out.clip_pos = vec4(ndc, 0.0, 1.0);
    out.local_pos = q * inst.rect.zw;
    out.half_size = half;
    out.color     = inst.color;
    out.radius    = inst.radius;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // SDF for a rounded rectangle (centered at half_size).
    let centered = in.local_pos - in.half_size;
    let q_sdf    = abs(centered) - in.half_size + vec2(in.radius);
    let dist     = length(max(q_sdf, vec2(0.0)))
                 + min(max(q_sdf.x, q_sdf.y), 0.0)
                 - in.radius;

    let alpha = in.color.a * (1.0 - smoothstep(-0.5, 0.5, dist));
    if alpha < 0.001 { discard; }

    // Premultiplied alpha output
    return vec4(in.color.rgb * alpha, alpha);
}
"#;

/// Per-instance data for a rounded rectangle draw call.
/// Stride = 48 bytes. `_pad` is NOT bound to a shader location.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RoundedRectInstance {
    /// x, y, width, height in physical pixels.
    pub rect: [f32; 4],
    /// RGBA colour (straight alpha).
    pub color: [f32; 4],
    /// Corner radius in physical pixels.
    pub radius: f32,
    pub _pad: [f32; 3],
}

/// Compiled wgpu pipeline and uniform resources for rounded rect rendering.
pub struct RoundedRectPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectUniforms {
    viewport_size: [f32; 2],
    _pad: [f32; 2],
}

impl RoundedRectPipeline {
    /// Create the pipeline. Does NOT take a queue — call `update_viewport` to set viewport size.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rounded rect shader"),
            source: wgpu::ShaderSource::Wgsl(ROUNDED_RECT_SHADER.into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect uniform bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect pipeline layout"),
            bind_group_layouts: &[Some(&uniform_bgl)],
            immediate_size: 0,
        });

        // Vertex buffer layout: per-instance (step_mode = Instance).
        // The `_pad` field is NOT exposed to the shader — we only declare 3 attributes.
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<RoundedRectInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // location(0): rect   — vec4<f32> at offset 0
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                // location(1): color  — vec4<f32> at offset 16
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 1,
                },
                // location(2): radius — f32 at offset 32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 32,
                    shader_location: 2,
                },
            ],
        };

        // Premultiplied-alpha blend: src=One, dst=OneMinusSrcAlpha
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rounded rect pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Zero-initialised uniform buffer — caller writes viewport via update_viewport.
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect uniforms"),
            size: mem::size_of::<RectUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rect uniform bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self { pipeline, uniform_buffer, uniform_bind_group }
    }

    /// Write the viewport size into the uniform buffer. Call on init and on resize.
    pub fn update_viewport(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let uniforms = RectUniforms {
            viewport_size: [width as f32, height as f32],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }
}
