/// Cell flag bits for the `flags` field in `CellVertex`.
/// Used by the GPU shader to select rendering mode.
pub const FLAG_CURSOR: u32 = 0x08;
/// LCD subpixel AA glyph (3× horizontal resolution in atlas).
pub const FLAG_LCD: u32 = 0x10;

/// A single instanced vertex representing one terminal cell on the GPU.
///
/// Each cell is rendered as a quad (two triangles). The vertex shader expands
/// the per-instance data into screen-space positions using the cell size uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CellVertex {
    /// Grid position [col, row].
    pub grid_pos: [f32; 2],
    /// UV coordinates into the glyph atlas texture [u_min, v_min, u_max, v_max].
    pub atlas_uv: [f32; 4],
    /// Foreground color RGBA.
    pub fg: [f32; 4],
    /// Background color RGBA.
    pub bg: [f32; 4],
    /// Glyph offset within the cell [x, y] in pixels (for sub-pixel positioning).
    pub glyph_offset: [f32; 2],
    /// Glyph size [w, h] in pixels.
    pub glyph_size: [f32; 2],
    /// Cell flags bit field: 0x1 = wide char, 0x2 = underline, 0x4 = strikethrough.
    pub flags: u32,
    /// Padding to align to 16 bytes.
    pub _pad: u32,
}

impl CellVertex {
    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CellVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // grid_pos
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // atlas_uv
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // fg
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // bg
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // glyph_offset
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // glyph_size
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // flags
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

/// Uniforms passed to the cell shader: cell size and viewport dimensions.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CellUniforms {
    /// Cell size in pixels [width, height].
    pub cell_size: [f32; 2],
    /// Viewport size in pixels [width, height].
    pub viewport_size: [f32; 2],
    /// Padding origin [x, y] in pixels.
    pub padding: [f32; 2],
    pub _pad: [f32; 2],
}
