use anyhow::{Context, Result};
use std::sync::Arc;
use winit::window::Window;

use crate::config::Config;
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::cell::{CellUniforms, CellVertex};
use crate::renderer::pipeline::CellPipeline;

/// Maximum number of cell instances per frame (cols × rows + overdraw headroom).
const MAX_INSTANCES: usize = 32_768;

/// Core wgpu renderer: owns the surface, device, queue, pipeline, and glyph atlas.
pub struct GpuRenderer {
    /// Kept alive so the raw window handle the surface holds remains valid.
    _window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
    bg_color: wgpu::Color,

    // Cell rendering resources
    pipeline: CellPipeline,
    pub atlas: GlyphAtlas,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    cell_count: usize,
}

impl GpuRenderer {
    pub async fn new(window: Arc<Window>, config: &Config) -> Result<Self> {
        let inner = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        // SAFETY: `_window` (Arc<Window>) is stored alongside `surface` in this struct and
        // will outlive the surface.
        let surface: wgpu::Surface<'static> = unsafe {
            use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(
                    window.display_handle().context("No display handle")?.as_raw()
                ),
                raw_window_handle: window
                    .window_handle()
                    .context("No window handle")?
                    .as_raw(),
            })?
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("No suitable GPU adapter found.")?;

        log::info!(
            "GPU adapter: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("PetruTerm"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .context("Failed to create wgpu device")?;

        let caps = surface.get_capabilities(&adapter);
        // Prefer non-sRGB format: our colors are stored in sRGB space (direct hex),
        // so we want the surface to display them as-is without GPU gamma encoding.
        let format = caps
            .formats
            .iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: inner.width.max(1),
            height: inner.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let bg = config.colors.background_wgpu();

        // Build pipeline and atlas
        let pipeline = CellPipeline::new(&device, format);
        let atlas = GlyphAtlas::new(&device);

        // Uniform buffer (CellUniforms, updated when cell size or viewport changes)
        let pad = &config.window.padding;
        let uniforms = CellUniforms {
            cell_size: [8.0, 16.0], // placeholder; updated after font measurement
            viewport_size: [inner.width as f32, inner.height as f32],
            padding: [pad.left as f32, pad.top as f32],
            _pad: [0.0; 2],
        };
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell uniforms"),
            size: std::mem::size_of::<CellUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cell uniform bg"),
            layout: &pipeline.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Atlas bind group
        let atlas_bind_group = make_atlas_bind_group(&device, &pipeline, &atlas);

        // Instance buffer (GPU-side, write each frame)
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<CellVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            _window: window,
            surface,
            device,
            queue,
            surface_config,
            size: (inner.width, inner.height),
            bg_color: bg,
            pipeline,
            atlas,
            uniform_buffer,
            uniform_bind_group,
            atlas_bind_group,
            instance_buffer,
            cell_count: 0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size = (width, height);
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        // Update viewport_size in uniforms (partial write at offset 8)
        let vp = [width as f32, height as f32];
        self.queue.write_buffer(&self.uniform_buffer, 8, bytemuck::cast_slice(&vp));

        log::debug!("Renderer resized to {width}x{height}");
    }

    /// Call after font measurement so the shader uses real cell metrics.
    pub fn set_cell_size(&mut self, w: f32, h: f32) {
        let cell = [w, h];
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&cell));
    }

    /// Upload cell instances for this frame. Must be called before `render()`.
    pub fn upload_instances(&mut self, instances: &[CellVertex]) {
        let count = instances.len().min(MAX_INSTANCES);
        if count > 0 {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&instances[..count]),
            );
        }
        self.cell_count = count;
    }

    /// Render a single frame: bg pass then glyph pass.
    pub fn render(&mut self) -> Result<()> {
        use wgpu::CurrentSurfaceTexture;
        let output = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(t) => t,
            CurrentSurfaceTexture::Suboptimal(t) => {
                self.surface.configure(&self.device, &self.surface_config);
                t
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            CurrentSurfaceTexture::Validation => {
                return Err(anyhow::anyhow!("wgpu validation error during surface acquire"));
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear + bg pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.bg_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            if self.cell_count > 0 {
                // Background pass
                pass.set_pipeline(&self.pipeline.bg_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..self.cell_count as u32);
            }
        }

        if self.cell_count > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glyph pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline.cell_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, &self.atlas_bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..6, 0..self.cell_count as u32);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    pub fn update_bg_color(&mut self, color: wgpu::Color) {
        self.bg_color = color;
    }

    /// Returns mutable access to the atlas and an immutable reference to the queue
    /// in one call, avoiding split-borrow issues in callers.
    pub fn atlas_and_queue(&mut self) -> (&mut GlyphAtlas, &wgpu::Queue) {
        (&mut self.atlas, &self.queue)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

fn make_atlas_bind_group(
    device: &wgpu::Device,
    pipeline: &CellPipeline,
    atlas: &GlyphAtlas,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("atlas bg"),
        layout: &pipeline.atlas_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(atlas.texture_view()),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(atlas.sampler()),
            },
        ],
    })
}
