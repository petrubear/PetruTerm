use crate::renderer::cell::CellVertex;

/// WGSL shader source for terminal cell rendering.
const CELL_SHADER: &str = r#"
struct CellUniforms {
    cell_size:     vec2<f32>,
    viewport_size: vec2<f32>,
    padding:       vec2<f32>,
    _pad:          vec2<f32>,
}

struct InstanceIn {
    @location(0) grid_pos:     vec2<f32>,
    @location(1) atlas_uv:     vec4<f32>,
    @location(2) fg:           vec4<f32>,
    @location(3) bg:           vec4<f32>,
    @location(4) glyph_offset: vec2<f32>,
    @location(5) glyph_size:   vec2<f32>,
    @location(6) flags:        u32,
}

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv:             vec2<f32>,
    @location(1) fg:             vec4<f32>,
    @location(2) bg:             vec4<f32>,
    @location(3) is_bg:          f32,
}

@group(0) @binding(0) var<uniform> uniforms: CellUniforms;
@group(1) @binding(0) var t_atlas:   texture_2d<f32>;
@group(1) @binding(1) var s_atlas:   sampler;

// Fullscreen quad vertices (two triangles covering the cell)
const QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, instance: InstanceIn) -> VertexOut {
    let q = QUAD[vi];

    // Cell pixel origin (top-left)
    let cell_origin = uniforms.padding + instance.grid_pos * uniforms.cell_size;

    // Background quad: covers the entire cell
    let bg_pixel = cell_origin + q * uniforms.cell_size;

    // Glyph quad: covers only the glyph bitmap within the cell
    let glyph_pixel = cell_origin + instance.glyph_offset + q * instance.glyph_size;

    // Convert pixel coords to NDC [-1, 1]
    let to_ndc = vec2(2.0, -2.0) / uniforms.viewport_size;
    let bg_ndc  = bg_pixel   * to_ndc + vec2(-1.0,  1.0);
    let gly_ndc = glyph_pixel * to_ndc + vec2(-1.0,  1.0);

    var out: VertexOut;

    // We encode both passes in one draw: vertex_index < 6 → bg, >= 6 → glyph
    // (Actual impl uses two draw calls or a flag uniform; simplified here)
    out.clip_pos = vec4(gly_ndc, 0.0, 1.0);
    out.uv  = mix(instance.atlas_uv.xy, instance.atlas_uv.zw, q);
    out.fg  = instance.fg;
    out.bg  = instance.bg;
    out.is_bg = 0.0;

    return out;
}

// sRGB ↔ linear helpers for gamma-correct blending.
// Our surface is Bgra8Unorm so colors are stored as sRGB bytes.
// Blending must happen in linear light to match WezTerm / correct antialiasing.
fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 { return c / 12.92; }
    return pow((c + 0.055) / 1.055, 2.4);
}
fn lin_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { return c * 12.92; }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_atlas, s_atlas, in.uv).r;
    // Premultiplied alpha blend over the bg pass (blend state: One / OneMinusSrcAlpha).
    // fg is already sRGB-encoded (same space as the bg pass output), so premultiply
    // directly. Alpha-0 pixels output vec4(0) — fully transparent — ensuring
    // powerline glyph edges never overwrite adjacent cells' background colours.
    return vec4(in.fg.rgb * alpha, alpha);
}

// Background-only pass (draws flat bg color for the whole cell).
// When FLAG_CURSOR (0x8) is set, uses glyph_offset/glyph_size to draw a
// partial-cell rect for underline/beam cursors; full cell for block cursor.
@vertex
fn vs_bg(@builtin(vertex_index) vi: u32, instance: InstanceIn) -> VertexOut {
    let q = QUAD[vi];
    let cell_origin = uniforms.padding + instance.grid_pos * uniforms.cell_size;

    var rect_size:   vec2<f32>;
    var rect_offset: vec2<f32>;
    if (instance.flags & 0x8u) != 0u {
        // Cursor instance: use glyph_size as rect dimensions, glyph_offset as
        // position within the cell (e.g. bottom 2px for underline cursor).
        rect_size   = instance.glyph_size;
        rect_offset = instance.glyph_offset;
    } else {
        rect_size   = uniforms.cell_size;
        rect_offset = vec2(0.0);
    }

    let bg_pixel = cell_origin + rect_offset + q * rect_size;
    let to_ndc   = vec2(2.0, -2.0) / uniforms.viewport_size;
    let bg_ndc   = bg_pixel * to_ndc + vec2(-1.0, 1.0);

    var out: VertexOut;
    out.clip_pos = vec4(bg_ndc, 0.0, 1.0);
    out.uv   = vec2(0.0);
    out.fg   = instance.fg;
    out.bg   = instance.bg;
    out.is_bg = 1.0;
    return out;
}

@fragment
fn fs_bg(in: VertexOut) -> @location(0) vec4<f32> {
    return in.bg;
}
"#;

/// Compiled wgpu render pipeline for terminal cell rendering.
pub struct CellPipeline {
    pub bg_pipeline:   wgpu::RenderPipeline,
    pub cell_pipeline: wgpu::RenderPipeline,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub atlas_bind_group_layout:   wgpu::BindGroupLayout,
}

impl CellPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cell shader"),
            source: wgpu::ShaderSource::Wgsl(CELL_SHADER.into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell uniform bgl"),
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

        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
            label: Some("cell pipeline layout"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&atlas_bgl)],
            immediate_size: 0,
        });

        // Premultiplied alpha blend for the glyph pass: shader outputs (rgb*a, a).
        // Using One/OneMinusSrcAlpha so alpha=0 pixels are fully transparent and
        // never overwrite the bg pass's colours (fixes powerline fringing).
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };

        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bg pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_bg"),
                buffers: &[CellVertex::vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_bg"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
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

        let cell_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[CellVertex::vertex_buffer_layout()],
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

        Self {
            bg_pipeline,
            cell_pipeline,
            uniform_bind_group_layout: uniform_bgl,
            atlas_bind_group_layout: atlas_bgl,
        }
    }
}
