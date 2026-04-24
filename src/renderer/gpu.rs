#![allow(dead_code)]
use anyhow::{Context, Result};
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;
use winit::window::Window;

use crate::config::Config;
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::cell::{CellUniforms, CellVertex};
use crate::renderer::lcd_atlas::LcdGlyphAtlas;
use crate::renderer::pipeline::{CellPipeline, CellPipelineBgAware, CellPipelineLcd};
use crate::renderer::rounded_rect::{RoundedRectInstance, RoundedRectPipeline};

/// Maximum number of cell instances per frame (cols × rows + overdraw headroom).
const MAX_INSTANCES: usize = 32_768;

/// Maximum number of rounded rect instances per frame (tab bar pills + overdraw).
const MAX_RECT_INSTANCES: usize = 1024;

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
    /// Present modes supported by the surface/adapter (captured at init).
    available_present_modes: Vec<wgpu::PresentMode>,

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
    /// Index into the instance buffer where overlay instances start (palette, context menu).
    /// Instances before this index are rendered first (bg+glyph); overlays rendered after.
    overlay_start: usize,

    // LCD subpixel AA resources
    lcd_pipeline: Option<CellPipelineLcd>,
    lcd_atlas: Option<Rc<RefCell<LcdGlyphAtlas>>>,
    lcd_atlas_bind_group: wgpu::BindGroup,
    lcd_instance_buffer: wgpu::Buffer,
    lcd_instance_count: usize,
    lcd_ready: bool,

    // Rounded rect pipeline (TD-013: pill tab bar)
    rect_pipeline: RoundedRectPipeline,
    rect_instance_buffer: wgpu::Buffer,
    rect_instance_count: usize,
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
                    window
                        .display_handle()
                        .context("No display handle")?
                        .as_raw(),
                ),
                raw_window_handle: window.window_handle().context("No window handle")?.as_raw(),
            })?
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: match config.gpu_preference {
                    crate::config::schema::GpuPreference::HighPerformance => {
                        wgpu::PowerPreference::HighPerformance
                    }
                    crate::config::schema::GpuPreference::None => wgpu::PowerPreference::None,
                    crate::config::schema::GpuPreference::LowPower => {
                        wgpu::PowerPreference::LowPower
                    }
                },
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

        // Prefer Mailbox (low-latency, no tearing); fall back to FifoRelaxed, then Fifo.
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else if caps.present_modes.contains(&wgpu::PresentMode::FifoRelaxed) {
            wgpu::PresentMode::FifoRelaxed
        } else {
            wgpu::PresentMode::Fifo
        };
        log::info!("Surface present mode: {:?}", present_mode);

        let available_present_modes = caps.present_modes.clone();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: inner.width.max(1),
            height: inner.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
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

        // Rounded rect pipeline (TD-013)
        let rect_pipeline = RoundedRectPipeline::new(&device, format);
        rect_pipeline.update_viewport(&queue, inner.width, inner.height);

        let rect_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect instances"),
            size: (MAX_RECT_INSTANCES * mem::size_of::<RoundedRectInstance>()) as u64,
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
            available_present_modes,
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
            overlay_start: 0,
            lcd_pipeline,
            lcd_atlas,
            lcd_atlas_bind_group,
            lcd_instance_buffer,
            lcd_instance_count: 0,
            lcd_ready: false,
            rect_pipeline,
            rect_instance_buffer,
            rect_instance_count: 0,
        })
    }

    /// Switch the surface present mode at runtime.
    /// Use `wgpu::PresentMode::Fifo` (vsync) to reduce GPU wakeup frequency on battery,
    /// or `wgpu::PresentMode::Mailbox` for lowest latency when on power.
    /// Has immediate effect — no restart required.
    pub fn set_present_mode(&mut self, mode: wgpu::PresentMode) {
        if self.surface_config.present_mode == mode {
            return;
        }
        self.surface_config.present_mode = mode;
        self.surface.configure(&self.device, &self.surface_config);
        log::info!("Present mode switched to {:?}", mode);
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
        self.queue
            .write_buffer(&self.uniform_buffer, 8, bytemuck::cast_slice(&vp));

        self.rect_pipeline
            .update_viewport(&self.queue, width, height);

        log::debug!("Renderer resized to {width}x{height}");
    }

    /// Call after font measurement so the shader uses real cell metrics.
    pub fn set_cell_size(&mut self, w: f32, h: f32) {
        let cell = [w, h];
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&cell));
    }

    /// Update the padding (origin offset) uniform. The x/y values are in physical pixels.
    /// Call after set_cell_size when the tab bar height is known.
    pub fn set_padding(&mut self, x: f32, y: f32) {
        let pad = [x, y];
        // CellUniforms layout: cell_size(8) + viewport_size(8) + padding(8) + _pad(8)
        self.queue
            .write_buffer(&self.uniform_buffer, 16, bytemuck::cast_slice(&pad));
    }

    /// Upload cell instances for this frame. Supports partial updates via offset.
    pub fn upload_instances(&mut self, instances: &[CellVertex], offset: usize) {
        let count = instances.len();
        if offset + count > MAX_INSTANCES {
            log::warn!(
                "upload_instances overflow: offset={offset} count={count} max={MAX_INSTANCES}"
            );
        }
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

    /// Set the index at which overlay instances (palette, context menu) begin.
    /// These are rendered in a separate bg+glyph pass AFTER the main pass so
    /// overlay backgrounds cover terminal glyphs underneath.
    pub fn set_overlay_start(&mut self, start: usize) {
        self.overlay_start = start.min(MAX_INSTANCES);
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
                return Err(anyhow::anyhow!(
                    "wgpu validation error during surface acquire"
                ));
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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

            // Rounded rect pass (TD-013: pill tab bar) — drawn before cell backgrounds
            if self.rect_instance_count > 0 {
                pass.set_pipeline(&self.rect_pipeline.pipeline);
                pass.set_bind_group(0, &self.rect_pipeline.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, self.rect_instance_buffer.slice(..));
                pass.draw(0..6, 0..self.rect_instance_count as u32);
            }

            if self.cell_count > 0 {
                let main_end = (self.overlay_start as u32).min(self.cell_count as u32);
                let overlay_end = self.cell_count as u32;
                let overlay_start = main_end;

                // ── Main pass: terminal + static UI ──────────────────────────────
                if main_end > 0 {
                    pass.set_pipeline(&self.pipeline.bg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, 0..main_end);

                    pass.set_pipeline(&self.pipeline.cell_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, 0..main_end);
                }

                // ── LCD pass (terminal glyphs) — drawn BEFORE overlay so overlay bg covers them ──
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

                // ── Overlay pass: palette, context menu ─────────────────────────
                // Rendered AFTER main glyphs (and LCD pass) so overlay backgrounds
                // cover all terminal text underneath.
                if overlay_start < overlay_end {
                    pass.set_pipeline(&self.pipeline.bg_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, overlay_start..overlay_end);

                    pass.set_pipeline(&self.pipeline.cell_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    pass.draw(0..6, overlay_start..overlay_end);
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

    /// Returns the wgpu queue when LCD AA is enabled, without borrowing the atlas.
    /// Use this to pass the queue to the rasterizer (which accesses its own Rc<RefCell> internally).
    pub fn lcd_queue(&self) -> Option<&wgpu::Queue> {
        if self.lcd_atlas.is_some() {
            Some(&self.queue)
        } else {
            None
        }
    }

    /// Upload rounded rect instances for this frame (TD-013). Must be called before `render()`.
    pub fn upload_rect_instances(&mut self, instances: &[RoundedRectInstance]) {
        let count = instances.len().min(MAX_RECT_INSTANCES);
        if instances.len() > MAX_RECT_INSTANCES {
            log::warn!(
                "upload_rect_instances overflow: count={} max={MAX_RECT_INSTANCES}",
                instances.len()
            );
        }
        if count > 0 {
            self.queue.write_buffer(
                &self.rect_instance_buffer,
                0,
                bytemuck::cast_slice(&instances[..count]),
            );
        }
        self.rect_instance_count = count;
    }

    /// Upload LCD cell instances for this frame. Must be called before `render()`.
    pub fn upload_lcd_instances(&mut self, instances: &[CellVertex]) {
        let count = instances.len().min(MAX_INSTANCES);
        if instances.len() > MAX_INSTANCES {
            log::warn!(
                "upload_lcd_instances overflow: count={} max={MAX_INSTANCES}",
                instances.len()
            );
        }
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
        self.lcd_atlas.as_ref().map(Rc::clone)
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

    /// Returns the present modes supported by this surface/adapter.
    pub fn surface_caps_present_modes(&self) -> &[wgpu::PresentMode] {
        &self.available_present_modes
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
                        resource: wgpu::BindingResource::TextureView(atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(atlas_sampler),
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

    /// Rebuild all atlas bind groups after atlas.clear() or lcd_atlas.clear().
    ///
    /// Both GlyphAtlas::clear() and LcdGlyphAtlas::clear() recreate the wgpu
    /// Texture and TextureView. Any BindGroup created before the clear holds a
    /// reference to the old (now destroyed) view, which is a wgpu correctness
    /// violation and may silently produce garbage rendering or a driver crash.
    ///
    /// Call this immediately after any atlas clear, before the next render pass.
    pub fn rebuild_atlas_bind_groups(&mut self) {
        self.atlas_bind_group = make_atlas_bind_group(&self.device, &self.pipeline, &self.atlas);
        self.bg_aware_atlas_bind_group =
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atlas bg (bg-aware)"),
                layout: &self.bg_aware_pipeline.atlas_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(self.atlas.texture_view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(self.atlas.sampler()),
                    },
                ],
            });
        if self.lcd_ready {
            if let (Some(pipeline), Some(atlas_rc)) = (&self.lcd_pipeline, &self.lcd_atlas) {
                let atlas = atlas_rc.borrow();
                self.lcd_atlas_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("LCD atlas bg"),
                        layout: &pipeline.lcd_atlas_bind_group_layout,
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
            }
        }
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
