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

    // Cell pixel origin - snapped to integer pixels to avoid subpixel gaps
    let cell_origin = floor(uniforms.padding + instance.grid_pos * uniforms.cell_size + vec2(0.005));

    // Glyph quad: covers only the glyph bitmap within the cell. 
    // We snap the offset too to maintain relative alignment.
    let glyph_pixel = cell_origin + floor(instance.glyph_offset + vec2(0.005)) + q * floor(instance.glyph_size + vec2(0.005));

    // Convert pixel coords to NDC [-1, 1]
    let to_ndc = vec2(2.0, -2.0) / uniforms.viewport_size;
    let gly_ndc = glyph_pixel * to_ndc + vec2(-1.0,  1.0);

    var out: VertexOut;
    out.clip_pos = vec4(gly_ndc, 0.0, 1.0);
    out.uv  = mix(instance.atlas_uv.xy, instance.atlas_uv.zw, q);
    out.fg  = instance.fg;
    out.bg  = instance.bg;
    out.is_bg = 0.0;

    return out;
}

// sRGB ↔ linear helpers for gamma-correct blending.
fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 { return c / 12.92; }
    return pow((c + 0.055) / 1.055, 2.4);
}
fn lin_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { return c * 12.92; }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// Convert a vec4 (rgba) to an array of 4 f32s — enables component-wise indexing.
fn to_array4(v: vec4<f32>) -> array<f32, 4> {
    return array<f32, 4>(v.r, v.g, v.b, v.a);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let mask = textureSample(t_atlas, s_atlas, in.uv).r;
    
    // Gamma-corrected coverage (TD-026). 
    // Powerline symbols need a slightly softer curve to avoid aliasing artifacts.
    let ca = pow(mask, 1.0 / 1.2);
    if ca < 0.001 { discard; }

    // Convert foreground to linear space for correct alpha blending
    let fg_lin = vec3(srgb_to_lin(in.fg.r), srgb_to_lin(in.fg.g), srgb_to_lin(in.fg.b));
    
    // Output PREMULTIPLIED alpha. 
    // This allows the GPU to blend perfectly over the already-drawn backgrounds.
    // We convert back to sRGB because the target surface is Bgra8Unorm (non-sRGB view).
    let rgb = vec3(lin_to_srgb(fg_lin.r), lin_to_srgb(fg_lin.g), lin_to_srgb(fg_lin.b));
    
    return vec4(rgb * ca, ca);
}

// TD-026b: Background-aware gamma-correct blend.
@fragment
fn fs_bg_aware(in: VertexOut) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_atlas, s_atlas, in.uv).r;
    let corrected_alpha = pow(alpha, 1.0 / 1.4);

    let fg = to_array4(in.fg);
    let bg = to_array4(in.bg);
    let fg_lin = vec3(srgb_to_lin(fg[0]), srgb_to_lin(fg[1]), srgb_to_lin(fg[2]));
    let bg_lin = vec3(srgb_to_lin(bg[0]), srgb_to_lin(bg[1]), srgb_to_lin(bg[2]));

    let blended_lin = mix(bg_lin, fg_lin, corrected_alpha);

    let out_rgb = vec3(lin_to_srgb(blended_lin[0]), lin_to_srgb(blended_lin[1]), lin_to_srgb(blended_lin[2]));
    return vec4(out_rgb, 1.0);
}

// TD-026c: LCD subpixel AA shader.
@group(2) @binding(0) var t_lcd_atlas: texture_2d<f32>;
@group(2) @binding(1) var s_lcd_atlas: sampler;

@fragment
fn fs_lcd(in: VertexOut) -> @location(0) vec4<f32> {
    let lcd = textureSample(t_lcd_atlas, s_lcd_atlas, in.uv);
    let coverage = pow(lcd.rgb, vec3(1.0 / 1.4));

    let fg = to_array4(in.fg);
    let bg = to_array4(in.bg);
    let fg_lin = vec3(srgb_to_lin(fg[0]), srgb_to_lin(fg[1]), srgb_to_lin(fg[2]));
    let bg_lin = vec3(srgb_to_lin(bg[0]), srgb_to_lin(bg[1]), srgb_to_lin(bg[2]));

    let blended_lin = mix(bg_lin, fg_lin, coverage);

    let out_rgb = vec3(lin_to_srgb(blended_lin[0]), lin_to_srgb(blended_lin[1]), lin_to_srgb(blended_lin[2]));
    return vec4(out_rgb, 1.0);
}

// Background-only pass (draws flat bg color for the whole cell).
// When FLAG_CURSOR (0x8) is set, uses glyph_offset/glyph_size to draw a
// partial-cell rect for underline/beam cursors; full cell for block cursor.
@vertex
fn vs_bg(@builtin(vertex_index) vi: u32, instance: InstanceIn) -> VertexOut {
    let q = QUAD[vi];
    // Snapped to integer pixels
    let cell_origin = floor(uniforms.padding + instance.grid_pos * uniforms.cell_size + vec2(0.005));

    var rect_size:   vec2<f32>;
    var rect_offset: vec2<f32>;
    if (instance.flags & 0x8u) != 0u {
        rect_size   = floor(instance.glyph_size + vec2(0.005));
        rect_offset = floor(instance.glyph_offset + vec2(0.005));
    } else {
        rect_size   = floor(uniforms.cell_size + vec2(0.005));
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
    if in.bg.a < 0.01 { discard; }
    return in.bg;
}
"#;

/// WGSL shader for LCD subpixel AA (TD-026c).
/// Includes CELL_SHADER code plus group 2 bindings for LCD atlas and fs_lcd entry point.
const LCD_SHADER: &str = r#"
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
@group(2) @binding(0) var t_lcd_atlas: texture_2d<f32>;
@group(2) @binding(1) var s_lcd_atlas: sampler;

const QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, instance: InstanceIn) -> VertexOut {
    let q = QUAD[vi];
    // Snapped to integer pixels
    let cell_origin = floor(uniforms.padding + instance.grid_pos * uniforms.cell_size + vec2(0.005));
    let glyph_pixel = cell_origin + floor(instance.glyph_offset + vec2(0.005)) + q * floor(instance.glyph_size + vec2(0.005));
    let to_ndc = vec2(2.0, -2.0) / uniforms.viewport_size;
    let bg_ndc  = (cell_origin + q * uniforms.cell_size) * to_ndc + vec2(-1.0, 1.0);
    let gly_ndc = glyph_pixel * to_ndc + vec2(-1.0,  1.0);
    var out: VertexOut;
    out.clip_pos = vec4(gly_ndc, 0.0, 1.0);
    out.uv  = mix(instance.atlas_uv.xy, instance.atlas_uv.zw, q);
    out.fg  = instance.fg;
    out.bg  = instance.bg;
    out.is_bg = 0.0;
    return out;
}

fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 { return c / 12.92; }
    return pow((c + 0.055) / 1.055, 2.4);
}
fn lin_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { return c * 12.92; }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// Convert a vec4 (rgba) to an array of 4 f32s — enables component-wise indexing.
fn to_array4(v: vec4<f32>) -> array<f32, 4> {
    return array<f32, 4>(v.r, v.g, v.b, v.a);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_atlas, s_atlas, in.uv).r;
    let ca = pow(alpha, 1.0 / 1.2);
    if ca < 0.001 { discard; }
    let fg_lin = vec3(srgb_to_lin(in.fg.r), srgb_to_lin(in.fg.g), srgb_to_lin(in.fg.b));
    let rgb = vec3(lin_to_srgb(fg_lin.r), lin_to_srgb(fg_lin.g), lin_to_srgb(fg_lin.b));
    return vec4(rgb * ca, ca);
}

@fragment
fn fs_bg_aware(in: VertexOut) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_atlas, s_atlas, in.uv).r;
    let corrected_alpha = pow(alpha, 1.0 / 1.4);
    let fg = to_array4(in.fg);
    let bg = to_array4(in.bg);
    let fg_lin = vec3(srgb_to_lin(fg[0]), srgb_to_lin(fg[1]), srgb_to_lin(fg[2]));
    let bg_lin = vec3(srgb_to_lin(bg[0]), srgb_to_lin(bg[1]), srgb_to_lin(bg[2]));
    let blended_lin = mix(bg_lin, fg_lin, corrected_alpha);
    let out_rgb = vec3(lin_to_srgb(blended_lin[0]), lin_to_srgb(blended_lin[1]), lin_to_srgb(blended_lin[2]));
    return vec4(out_rgb, 1.0);
}

// TD-026c: LCD subpixel AA — reads 3×-resolution LCD glyphs, blends per-channel in linear space.
@fragment
fn fs_lcd(in: VertexOut) -> @location(0) vec4<f32> {
    let lcd = textureSample(t_lcd_atlas, s_lcd_atlas, in.uv);
    let coverage = pow(lcd.rgb, vec3(1.0 / 1.4));
    let fg = to_array4(in.fg);
    let bg = to_array4(in.bg);
    let fg_lin = vec3(srgb_to_lin(fg[0]), srgb_to_lin(fg[1]), srgb_to_lin(fg[2]));
    let bg_lin = vec3(srgb_to_lin(bg[0]), srgb_to_lin(bg[1]), srgb_to_lin(bg[2]));
    let blended_lin = mix(bg_lin, fg_lin, coverage);
    let out_rgb = vec3(lin_to_srgb(blended_lin[0]), lin_to_srgb(blended_lin[1]), lin_to_srgb(blended_lin[2]));
    return vec4(out_rgb, 1.0);
}

@vertex
fn vs_bg(@builtin(vertex_index) vi: u32, instance: InstanceIn) -> VertexOut {
    let q = QUAD[vi];
    // Snapped to integer pixels
    let cell_origin = floor(uniforms.padding + instance.grid_pos * uniforms.cell_size + vec2(0.005));
    var rect_size:   vec2<f32>;
    var rect_offset: vec2<f32>;
    if (instance.flags & 0x8u) != 0u {
        rect_size   = floor(instance.glyph_size + vec2(0.005));
        rect_offset = floor(instance.glyph_offset + vec2(0.005));
    } else {
        rect_size   = floor(uniforms.cell_size + vec2(0.005));
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
    if in.bg.a < 0.01 { discard; }
    return in.bg;
}
"#;

/// Compiled wgpu render pipeline for terminal cell rendering.
pub struct CellPipeline {
    pub bg_pipeline: wgpu::RenderPipeline,
    pub cell_pipeline: wgpu::RenderPipeline,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub atlas_bind_group_layout: wgpu::BindGroupLayout,
}

/// Background-aware glyph pipeline variant (TD-026b).
/// Does gamma-correct blend in shader (linear space), outputs non-premultiplied color.
pub struct CellPipelineBgAware {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub atlas_bind_group_layout: wgpu::BindGroupLayout,
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

        // Blend state for the glyph pass (One / OneMinusSrcAlpha).
        // Shader outputs either vec4(0) for near-transparent pixels (pass-through)
        // or vec4(gamma_blended_rgb, 1.0) for visible pixels (full replace).
        // Both cases work correctly with this blend equation:
        //   alpha=0 → 0 + 1*dst = dst  (bg pass colour visible, no fringing)
        //   alpha=1 → rgb + 0*dst = rgb (gamma-correct blend replaces bg)
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

impl CellPipelineBgAware {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cell shader bg-aware"),
            source: wgpu::ShaderSource::Wgsl(CELL_SHADER.into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell uniform bgl (bg-aware)"),
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
            label: Some("atlas bgl (bg-aware)"),
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
            label: Some("cell pipeline layout (bg-aware)"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&atlas_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell pipeline (bg-aware)"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[CellVertex::vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_bg_aware"),
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

        Self {
            pipeline,
            uniform_bind_group_layout: uniform_bgl,
            atlas_bind_group_layout: atlas_bgl,
        }
    }
}

/// LCD subpixel AA pipeline (TD-026c).
/// Reads 3×-resolution LCD glyphs from a separate atlas and blends per-channel
/// against the cell background in linear space.
pub struct CellPipelineLcd {
    pub bg_pipeline: wgpu::RenderPipeline,
    pub lcd_pipeline: wgpu::RenderPipeline,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub atlas_bind_group_layout: wgpu::BindGroupLayout,
    pub lcd_atlas_bind_group_layout: wgpu::BindGroupLayout,
}

impl CellPipelineLcd {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("LCD shader"),
            source: wgpu::ShaderSource::Wgsl(LCD_SHADER.into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell uniform bgl (LCD)"),
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
            label: Some("atlas bgl (LCD)"),
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

        let lcd_atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LCD atlas bgl"),
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
            label: Some("cell pipeline layout (LCD)"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&atlas_bgl), Some(&lcd_atlas_bgl)],
            immediate_size: 0,
        });

        let blend_replace = wgpu::BlendState::REPLACE;

        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bg pipeline (LCD)"),
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

        let lcd_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("LCD pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[CellVertex::vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_lcd"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend_replace),
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
            lcd_pipeline,
            uniform_bind_group_layout: uniform_bgl,
            atlas_bind_group_layout: atlas_bgl,
            lcd_atlas_bind_group_layout: lcd_atlas_bgl,
        }
    }
}
