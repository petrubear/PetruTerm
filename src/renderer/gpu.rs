use anyhow::{Context, Result};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use winit::window::Window;

use crate::config::Config;
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::cell::{CellUniforms, CellVertex};
use crate::renderer::lcd_atlas::LcdGlyphAtlas;
use crate::renderer::pipeline::{CellPipeline, CellPipelineBgAware, CellPipelineLcd};

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
    bg_aware_pipeline: CellPipelineBgAware,
    pub atlas: GlyphAtlas,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    atlas_bind_group: wgpu::BindGroup,
    bg_aware_uniform_bind_group: wgpu::BindGroup,
    bg_aware_atlas_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    cell_count: usize,

    // LCD subpixel AA resources
    lcd_pipeline: Option<CellPipelineLcd>,
    lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    lcd_atlas_bind_group: wgpu::BindGroup,
    lcd_instance_buffer: wgpu::Buffer,
    lcd_instance_count: usize,
    lcd_ready: bool,
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
        let bg_aware_pipeline = CellPipelineBgAware::new(&device, format);
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

        // Bind groups for bg-aware pipeline (same resource, different layout instances)
        let bg_aware_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cell uniform bg (bg-aware)"),
            layout: &bg_aware_pipeline.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let bg_aware_atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bg (bg-aware)"),
            layout: &bg_aware_pipeline.atlas_bind_group_layout,
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
        });

        // Instance buffer (GPU-side, write each frame)
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<CellVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // LCD subpixel AA resources (created only if LCD AA is enabled in config)
        let lcd_pipeline = if config.font.lcd_antialiasing {
            let pipeline = CellPipelineLcd::new(&device, format);
            log::info!("LCD subpixel AA pipeline created");
            Some(pipeline)
        } else {
            None
        };

        let lcd_atlas = if config.font.lcd_antialiasing {
            Some(Rc::new(RefCell::new(LcdGlyphAtlas::new(&device))))
        } else {
            None
        };

        // LCD atlas bind group (created now, will be recreated when set_lcd_atlas is called)
        let lcd_atlas_bind_group: wgpu::BindGroup;

        // Placeholder bind group initially
        let dummy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LCD atlas bg (dummy)"),
            entries: &[],
        });
        lcd_atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("LCD atlas bg (placeholder)"),
            layout: &dummy_layout,
            entries: &[],
        });

        // LCD instance buffer
        let lcd_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LCD cell instances"),
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
            bg_aware_pipeline,
            atlas,
            uniform_buffer,
            uniform_bind_group,
            atlas_bind_group,
            bg_aware_uniform_bind_group,
            bg_aware_atlas_bind_group,
            instance_buffer,
            cell_count: 0,
            lcd_pipeline,
            lcd_atlas,
            lcd_atlas_bind_group,
            lcd_instance_buffer,
            lcd_instance_count: 0,
            lcd_ready: false,
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

    /// Upload cell instances for this frame. Supports partial updates via offset.
    pub fn upload_instances(&mut self, instances: &[CellVertex], offset: usize) {
        let count = instances.len();
        if count > 0 && offset + count <= MAX_INSTANCES {
            self.queue.write_buffer(
                &self.instance_buffer,
                (offset * std::mem::size_of::<CellVertex>()) as u64,
                bytemuck::cast_slice(instances),
            );
        }
        // cell_count is the total count to draw, set by the caller.
    }

    pub fn set_cell_count(&mut self, count: usize) {
        self.cell_count = count.min(MAX_INSTANCES);
    }

    /// Render a single frame: combined pass for bg and glyphs.
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
                label: Some("terminal pass"),
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
                // Backgrounds first
                pass.set_pipeline(&self.pipeline.bg_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..self.cell_count as u32);

                // Glyphs on top (same pass, no reload)
                pass.set_pipeline(&self.pipeline.cell_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..self.cell_count as u32);
            }

            // LCD pass (if any)
            if self.lcd_instance_count > 0 {
                if let Some(ref lcd_pipeline) = self.lcd_pipeline {
                    pass.set_pipeline(&lcd_pipeline.lcd_pipeline);
                    pass.set_bind_group(0, &self.bg_aware_uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.bg_aware_atlas_bind_group, &[]);
                    pass.set_bind_group(2, &self.lcd_atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.lcd_instance_buffer.slice(..));
                    pass.draw(0..6, 0..self.lcd_instance_count as u32);
                }
            }
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

    /// Returns mutable access to the LCD atlas and an immutable reference to the queue.
    /// Only available when LCD AA is enabled.
    pub fn lcd_atlas_and_queue(&mut self) -> Option<(std::cell::RefMut<'_, LcdGlyphAtlas>, &wgpu::Queue)> {
        if let Some(atlas) = &self.lcd_atlas {
            Some((atlas.borrow_mut(), &self.queue))
        } else {
            None
        }
    }

    /// Upload LCD cell instances for this frame. Must be called before `render()`.
    pub fn upload_lcd_instances(&mut self, instances: &[CellVertex]) {
        let count = instances.len().min(MAX_INSTANCES);
        if count > 0 {
            self.queue.write_buffer(
                &self.lcd_instance_buffer,
                0,
                bytemuck::cast_slice(&instances[..count]),
            );
        }
        self.lcd_instance_count = count;
    }

    /// Returns true if LCD subpixel AA is enabled.
    pub fn has_lcd(&self) -> bool {
        self.lcd_pipeline.is_some()
    }

    /// Take the LCD atlas out of the renderer, transferring it to TextShaper.
    /// Called during initialization to share the atlas with the rasterizer.
    pub fn take_lcd_atlas(&mut self) -> Option<Rc<RefCell<LcdGlyphAtlas>>> {
        self.lcd_atlas.take()
    }

    /// Returns a clone of the LCD atlas Rc for sharing with TextShaper.
    pub fn get_lcd_atlas(&self) -> Option<Rc<RefCell<LcdGlyphAtlas>>> {
        self.lcd_atlas.as_ref().map(|rc| Rc::clone(rc))
    }

    pub fn device(&self) -> wgpu::Device {
        self.device.clone()
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

    /// Set the LCD atlas and create the bind group for the LCD render pass.
    /// Called once after TextShaper is created, to share the same atlas between
    /// the rasterizer (in TextShaper) and the renderer (here).
    pub fn set_lcd_atlas(&mut self, atlas: Rc<RefCell<LcdGlyphAtlas>>) {
        let pipeline = match &self.lcd_pipeline {
            Some(p) => p,
            None => return,
        };

        {
            let atlas_ref = atlas.borrow();
            let atlas_view = atlas_ref.texture_view();
            let atlas_sampler = atlas_ref.sampler();

            self.lcd_atlas_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("LCD atlas bg"),
                layout: &pipeline.lcd_atlas_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                    },
                ],
            });
        }

        self.lcd_atlas = Some(atlas);
        self.lcd_ready = true;
    }

    /// Returns true if LCD subpixel AA is ready (atlas has been set).
    pub fn is_lcd_ready(&self) -> bool {
        self.lcd_ready
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
