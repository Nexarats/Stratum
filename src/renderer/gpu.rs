//! GPU renderer — wgpu pipeline for rendering the terminal grid.
//!
//! Creates a window via winit, initializes wgpu, and renders the
//! terminal grid using textured quads (one per cell background,
//! one per glyph). The glyph atlas texture is uploaded once and
//! updated when new glyphs are rasterized.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::glyph_atlas::GlyphAtlas;
use crate::config::theme::{self, Theme};
use crate::screen::{Cell, Color, ScreenGrid, Selection};

/// Vertex data for a single corner of a quad.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    /// Screen position in clip space (-1 to 1).
    pub position: [f32; 2],
    /// UV coordinates into the glyph atlas.
    pub uv: [f32; 2],
    /// Foreground color (RGBA).
    pub fg_color: [f32; 4],
    /// Background color (RGBA).
    pub bg_color: [f32; 4],
    /// 1.0 = glyph quad, 0.0 = background quad.
    pub is_glyph: f32,
    /// Padding to align to 16 bytes.
    pub _padding: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2,  // position
        1 => Float32x2,  // uv
        2 => Float32x4,  // fg_color
        3 => Float32x4,  // bg_color
        4 => Float32,    // is_glyph
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// The GPU renderer state.
pub struct GpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture,
    pub glyph_atlas: GlyphAtlas,
    pub size: PhysicalSize<u32>,
    max_vertices: usize,
    max_indices: usize,
    /// Active color theme.
    pub theme: &'static Theme,
}

impl GpuRenderer {
    /// Initialize the GPU renderer with the given window.
    pub fn new(window: Arc<Window>, font_size: f32) -> Result<Self> {
        let size = window.inner_size();

        // Create wgpu instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create surface
        let surface = instance
            .create_surface(window.clone())
            .context("Failed to create wgpu surface")?;

        // Request adapter
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .context("Failed to find GPU adapter")?;

        tracing::info!("GPU adapter: {}", adapter.get_info().name);

        // Request device and queue
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Stratum GPU"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .context("Failed to create GPU device")?;

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, // VSync
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Initialize glyph atlas
        let mut glyph_atlas = GlyphAtlas::new(font_size);
        glyph_atlas.preload_ascii();

        // Create atlas texture on GPU
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width: glyph_atlas.atlas_width,
                height: glyph_atlas.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload initial atlas data
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            glyph_atlas.atlas_data(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(glyph_atlas.atlas_width),
                rows_per_image: Some(glyph_atlas.atlas_height),
            },
            wgpu::Extent3d {
                width: glyph_atlas.atlas_width,
                height: glyph_atlas.atlas_height,
                depth_or_array_layers: 1,
            },
        );
        glyph_atlas.mark_clean();

        // Create texture view and sampler
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glyph Atlas Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glyph Atlas Bind Group"),
            layout: &bind_group_layout,
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

        // Load shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terminal Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terminal.wgsl").into()),
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terminal Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Render pipeline
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terminal Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Pre-allocate vertex and index buffers for a 200x60 grid (generous)
        let max_cells = 200 * 60;
        // Each cell = 2 quads (bg + glyph) × 4 vertices = 8 vertices per cell
        let max_vertices = max_cells * 8;
        // Each quad = 6 indices (2 triangles), 2 quads per cell = 12 indices
        let max_indices = max_cells * 12;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: (max_vertices * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: (max_indices * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            bind_group,
            atlas_texture,
            glyph_atlas,
            size,
            max_vertices,
            max_indices,
            theme: &theme::STRATUM_DARK,
        })
    }

    /// Handle window resize.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Calculate terminal grid dimensions from window size.
    pub fn grid_dimensions(&self) -> (usize, usize) {
        let cols = (self.size.width as f32 / self.glyph_atlas.cell_width).floor() as usize;
        let rows = (self.size.height as f32 / self.glyph_atlas.cell_height).floor() as usize;
        (cols.max(1), rows.max(1))
    }

    /// Set the active theme by name.
    pub fn set_theme(&mut self, name: &str) {
        self.theme = theme::get_theme(name);
    }

    /// Render the terminal screen grid with overlays and selection highlighting.
    pub fn render(&mut self, screen: &ScreenGrid, selection: &Selection, overlays: &[crate::renderer::overlay::OverlayElement], scroll_offset: usize) -> Result<()> {
        // Re-upload atlas if new glyphs were rasterized
        if self.glyph_atlas.dirty {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                self.glyph_atlas.atlas_data(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.glyph_atlas.atlas_width),
                    rows_per_image: Some(self.glyph_atlas.atlas_height),
                },
                wgpu::Extent3d {
                    width: self.glyph_atlas.atlas_width,
                    height: self.glyph_atlas.atlas_height,
                    depth_or_array_layers: 1,
                },
            );
            self.glyph_atlas.mark_clean();
        }

        // Build vertex and index data
        let mut vertices: Vec<Vertex> = Vec::with_capacity(screen.width() * screen.height() * 8);
        let mut indices: Vec<u32> = Vec::with_capacity(screen.width() * screen.height() * 12);

        let screen_w = self.size.width as f32;
        let screen_h = self.size.height as f32;
        let cell_w = self.glyph_atlas.cell_width;
        let cell_h = self.glyph_atlas.cell_height;
        let atlas_w = self.glyph_atlas.atlas_width as f32;
        let atlas_h = self.glyph_atlas.atlas_height as f32;

        for row in 0..screen.height() {
            for col in 0..screen.width() {
                let cell = screen.get_cell_scrolled(col, row, scroll_offset);

                // Pixel coordinates of cell
                let px = col as f32 * cell_w;
                let py = row as f32 * cell_h;

                // Convert to clip space (-1 to 1)
                let x0 = (px / screen_w) * 2.0 - 1.0;
                let y0 = 1.0 - (py / screen_h) * 2.0; // Flip Y
                let x1 = ((px + cell_w) / screen_w) * 2.0 - 1.0;
                let y1 = 1.0 - ((py + cell_h) / screen_h) * 2.0;

                let mut bg = self.theme.resolve_bg(cell.bg);
                let mut fg = self.theme.resolve_fg(cell.fg);

                // Selection highlight: blend selection color over background
                if selection.contains(col, row) {
                    let sel_bg = self.theme.selection_bg;
                    // Alpha-blend selection over background
                    let alpha = sel_bg[3];
                    bg[0] = bg[0] * (1.0 - alpha) + sel_bg[0] * alpha;
                    bg[1] = bg[1] * (1.0 - alpha) + sel_bg[1] * alpha;
                    bg[2] = bg[2] * (1.0 - alpha) + sel_bg[2] * alpha;
                    bg[3] = 1.0;
                    if let Some(sel_fg) = self.theme.selection_fg {
                        fg = sel_fg;
                    }
                }

                let base_idx = vertices.len() as u32;

                // --- Background quad (4 vertices, 6 indices) ---
                vertices.push(Vertex {
                    position: [x0, y0],
                    uv: [0.0, 0.0],
                    fg_color: fg,
                    bg_color: bg,
                    is_glyph: 0.0,
                    _padding: [0.0; 3],
                });
                vertices.push(Vertex {
                    position: [x1, y0],
                    uv: [0.0, 0.0],
                    fg_color: fg,
                    bg_color: bg,
                    is_glyph: 0.0,
                    _padding: [0.0; 3],
                });
                vertices.push(Vertex {
                    position: [x1, y1],
                    uv: [0.0, 0.0],
                    fg_color: fg,
                    bg_color: bg,
                    is_glyph: 0.0,
                    _padding: [0.0; 3],
                });
                vertices.push(Vertex {
                    position: [x0, y1],
                    uv: [0.0, 0.0],
                    fg_color: fg,
                    bg_color: bg,
                    is_glyph: 0.0,
                    _padding: [0.0; 3],
                });

                indices.extend_from_slice(&[
                    base_idx,
                    base_idx + 1,
                    base_idx + 2,
                    base_idx,
                    base_idx + 2,
                    base_idx + 3,
                ]);

                // --- Glyph quad (only if character is not space) ---
                if cell.ch != ' ' {
                    let glyph = self.glyph_atlas.get_glyph(cell.ch);

                    if glyph.width > 0 && glyph.height > 0 {
                        // UV coordinates in atlas
                        let u0 = glyph.atlas_x as f32 / atlas_w;
                        let v0 = glyph.atlas_y as f32 / atlas_h;
                        let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_w;
                        let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_h;

                        // Glyph position within the cell
                        // x_offset = horizontal bearing from fontdue
                        // y_offset = pre-computed distance from top of cell to top of glyph
                        let gx0 = px + glyph.x_offset;
                        let gy0 = py + glyph.y_offset;
                        let gx1 = gx0 + glyph.width as f32;
                        let gy1 = gy0 + glyph.height as f32;

                        // Clip space
                        let gx0_clip = (gx0 / screen_w) * 2.0 - 1.0;
                        let gy0_clip = 1.0 - (gy0 / screen_h) * 2.0;
                        let gx1_clip = (gx1 / screen_w) * 2.0 - 1.0;
                        let gy1_clip = 1.0 - (gy1 / screen_h) * 2.0;

                        let glyph_base = vertices.len() as u32;

                        vertices.push(Vertex {
                            position: [gx0_clip, gy0_clip],
                            uv: [u0, v0],
                            fg_color: fg,
                            bg_color: bg,
                            is_glyph: 1.0,
                            _padding: [0.0; 3],
                        });
                        vertices.push(Vertex {
                            position: [gx1_clip, gy0_clip],
                            uv: [u1, v0],
                            fg_color: fg,
                            bg_color: bg,
                            is_glyph: 1.0,
                            _padding: [0.0; 3],
                        });
                        vertices.push(Vertex {
                            position: [gx1_clip, gy1_clip],
                            uv: [u1, v1],
                            fg_color: fg,
                            bg_color: bg,
                            is_glyph: 1.0,
                            _padding: [0.0; 3],
                        });
                        vertices.push(Vertex {
                            position: [gx0_clip, gy1_clip],
                            uv: [u0, v1],
                            fg_color: fg,
                            bg_color: bg,
                            is_glyph: 1.0,
                            _padding: [0.0; 3],
                        });

                        indices.extend_from_slice(&[
                            glyph_base,
                            glyph_base + 1,
                            glyph_base + 2,
                            glyph_base,
                            glyph_base + 2,
                            glyph_base + 3,
                        ]);
                    }
                }
            }
        }
        // --- Cursor ---
        {
            let (cur_col, cur_row) = screen.cursor_position();
            let viewport_row = cur_row + scroll_offset;
            if cur_col < screen.width() && viewport_row < screen.height() {
                let px = cur_col as f32 * cell_w;
                let py = viewport_row as f32 * cell_h;

                let x0 = (px / screen_w) * 2.0 - 1.0;
                let y0 = 1.0 - (py / screen_h) * 2.0;
                let x1 = ((px + cell_w) / screen_w) * 2.0 - 1.0;
                let y1 = 1.0 - ((py + cell_h) / screen_h) * 2.0;

                // Block cursor: use theme cursor color with slight transparency
                let cursor_color = [
                    self.theme.cursor[0],
                    self.theme.cursor[1],
                    self.theme.cursor[2],
                    0.92,
                ];

                let base = vertices.len() as u32;
                vertices.extend_from_slice(&[
                    Vertex { position: [x0, y0], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    Vertex { position: [x1, y0], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    Vertex { position: [x1, y1], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    Vertex { position: [x0, y1], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                ]);
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                // Re-draw the glyph under cursor in dark color so it's visible
                let cell = screen.get_cell(cur_col, cur_row);
                if cell.ch != ' ' {
                    let glyph = self.glyph_atlas.get_glyph(cell.ch);
                    if glyph.width > 0 && glyph.height > 0 {
                        let u0 = glyph.atlas_x as f32 / atlas_w;
                        let v0 = glyph.atlas_y as f32 / atlas_h;
                        let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_w;
                        let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_h;

                        let gx0 = px + glyph.x_offset;
                        let gy0 = py + glyph.y_offset;
                        let gx1 = gx0 + glyph.width as f32;
                        let gy1 = gy0 + glyph.height as f32;

                        let gx0c = (gx0 / screen_w) * 2.0 - 1.0;
                        let gy0c = 1.0 - (gy0 / screen_h) * 2.0;
                        let gx1c = (gx1 / screen_w) * 2.0 - 1.0;
                        let gy1c = 1.0 - (gy1 / screen_h) * 2.0;

                        let dark_fg = [0.05, 0.05, 0.08, 1.0]; // dark text on cursor
                        let gb = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [gx0c, gy0c], uv: [u0, v0], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                            Vertex { position: [gx1c, gy0c], uv: [u1, v0], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                            Vertex { position: [gx1c, gy1c], uv: [u1, v1], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                            Vertex { position: [gx0c, gy1c], uv: [u0, v1], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[gb, gb + 1, gb + 2, gb, gb + 2, gb + 3]);
                    }
                }
            }
        }

        // Append overlay vertices into the same batch
        self.append_overlay_vertices(&mut vertices, &mut indices, overlays);

        // Upload buffers
        if vertices.len() > self.max_vertices || indices.len() > self.max_indices {
            tracing::warn!("Buffer overflow — grid too large for pre-allocated buffers");
            return Ok(());
        }

        self.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.queue
            .write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));

        // Get surface texture
        let output = self
            .surface
            .get_current_texture()
            .context("Failed to get surface texture")?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Terminal Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Render multiple panes into their respective screen regions, with a tab bar.
    pub fn render_multi(
        &mut self,
        panes: &[(crate::layout::panes::PaneRect, &ScreenGrid, bool)], // (rect, screen, is_active)
        tab_titles: &[(&str, bool)], // (title, is_active)
        selection: &Selection,
        overlays: &[crate::renderer::overlay::OverlayElement],
        scroll_offset: usize,
    ) -> Result<()> {
        // Re-upload atlas if needed
        if self.glyph_atlas.dirty {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                self.glyph_atlas.atlas_data(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.glyph_atlas.atlas_width),
                    rows_per_image: Some(self.glyph_atlas.atlas_height),
                },
                wgpu::Extent3d {
                    width: self.glyph_atlas.atlas_width,
                    height: self.glyph_atlas.atlas_height,
                    depth_or_array_layers: 1,
                },
            );
            self.glyph_atlas.mark_clean();
        }

        let mut vertices: Vec<Vertex> = Vec::with_capacity(64_000);
        let mut indices: Vec<u32> = Vec::with_capacity(96_000);

        let screen_w = self.size.width as f32;
        let screen_h = self.size.height as f32;
        let cell_w = self.glyph_atlas.cell_width;
        let cell_h = self.glyph_atlas.cell_height;
        let atlas_w = self.glyph_atlas.atlas_width as f32;
        let atlas_h = self.glyph_atlas.atlas_height as f32;

        // --- Tab bar (at top, height = cell_h) ---
        let tab_bar_h = cell_h + 4.0;
        if tab_titles.len() > 1 {
            let mut tab_x = 0.0f32;
            for (title, is_active) in tab_titles {
                let tab_w = (title.len() as f32 + 4.0) * cell_w.min(10.0);
                let bg = if *is_active {
                    [0.20, 0.22, 0.30, 1.0] // active tab bg
                } else {
                    [0.08, 0.08, 0.12, 1.0] // inactive tab bg
                };
                let fg = if *is_active {
                    [0.90, 0.92, 0.98, 1.0]
                } else {
                    [0.50, 0.52, 0.58, 1.0]
                };

                // Tab background quad
                let x0 = (tab_x / screen_w) * 2.0 - 1.0;
                let y0 = 1.0;
                let x1 = ((tab_x + tab_w) / screen_w) * 2.0 - 1.0;
                let y1 = 1.0 - (tab_bar_h / screen_h) * 2.0;
                let base = vertices.len() as u32;
                vertices.extend_from_slice(&[
                    Vertex { position: [x0, y0], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    Vertex { position: [x1, y0], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    Vertex { position: [x1, y1], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    Vertex { position: [x0, y1], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                ]);
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                // Tab title glyphs
                let text_x_start = tab_x + cell_w;
                let text_y = 2.0;
                for (ci, ch) in title.chars().enumerate() {
                    if ch == ' ' { continue; }
                    let glyph = self.glyph_atlas.get_glyph(ch);
                    if glyph.width == 0 || glyph.height == 0 { continue; }

                    let u0 = glyph.atlas_x as f32 / atlas_w;
                    let v0 = glyph.atlas_y as f32 / atlas_h;
                    let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_w;
                    let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_h;

                    let gx0 = text_x_start + ci as f32 * cell_w + glyph.x_offset;
                    let gy0 = text_y + glyph.y_offset;
                    let gx1 = gx0 + glyph.width as f32;
                    let gy1 = gy0 + glyph.height as f32;

                    let gx0c = (gx0 / screen_w) * 2.0 - 1.0;
                    let gy0c = 1.0 - (gy0 / screen_h) * 2.0;
                    let gx1c = (gx1 / screen_w) * 2.0 - 1.0;
                    let gy1c = 1.0 - (gy1 / screen_h) * 2.0;

                    let gb = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [gx0c, gy0c], uv: [u0, v0], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                        Vertex { position: [gx1c, gy0c], uv: [u1, v0], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                        Vertex { position: [gx1c, gy1c], uv: [u1, v1], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                        Vertex { position: [gx0c, gy1c], uv: [u0, v1], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[gb, gb + 1, gb + 2, gb, gb + 2, gb + 3]);
                }
                tab_x += tab_w + 2.0; // 2px gap between tabs
            }
        }

        let content_y_offset = if tab_titles.len() > 1 { tab_bar_h } else { 0.0 };

        // --- Render each pane ---
        for (rect, screen, is_active) in panes {
            let pane_x = rect.x;
            let pane_y = rect.y + content_y_offset;
            let pane_w = rect.width;
            let pane_h = rect.height - content_y_offset / panes.len().max(1) as f32;
            for row in 0..screen.height() {
                for col in 0..screen.width() {
                    let cell = if *is_active {
                        screen.get_cell_scrolled(col, row, scroll_offset)
                    } else {
                        screen.get_cell(col, row)
                    };

                    let px = pane_x + col as f32 * cell_w;
                    let py = pane_y + row as f32 * cell_h;

                    // Clip to pane bounds
                    if px + cell_w > pane_x + pane_w || py + cell_h > pane_y + pane_h {
                        continue;
                    }

                    let x0 = (px / screen_w) * 2.0 - 1.0;
                    let y0 = 1.0 - (py / screen_h) * 2.0;
                    let x1 = ((px + cell_w) / screen_w) * 2.0 - 1.0;
                    let y1 = 1.0 - ((py + cell_h) / screen_h) * 2.0;

                    let mut bg = self.theme.resolve_bg(cell.bg);
                    let mut fg = self.theme.resolve_fg(cell.fg);

                    // Selection highlight (only for active pane)
                    if *is_active && selection.contains(col, row) {
                        let sel_bg = self.theme.selection_bg;
                        let alpha = sel_bg[3];
                        bg[0] = bg[0] * (1.0 - alpha) + sel_bg[0] * alpha;
                        bg[1] = bg[1] * (1.0 - alpha) + sel_bg[1] * alpha;
                        bg[2] = bg[2] * (1.0 - alpha) + sel_bg[2] * alpha;
                        bg[3] = 1.0;
                        if let Some(sel_fg) = self.theme.selection_fg {
                            fg = sel_fg;
                        }
                    }

                    let base_idx = vertices.len() as u32;

                    // Background quad
                    vertices.extend_from_slice(&[
                        Vertex { position: [x0, y0], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x1, y0], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x1, y1], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x0, y1], uv: [0.0, 0.0], fg_color: fg, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);

                    // Glyph quad
                    if cell.ch != ' ' {
                        let glyph = self.glyph_atlas.get_glyph(cell.ch);
                        if glyph.width > 0 && glyph.height > 0 {
                            let u0 = glyph.atlas_x as f32 / atlas_w;
                            let v0 = glyph.atlas_y as f32 / atlas_h;
                            let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_w;
                            let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_h;

                            let gx0 = px + glyph.x_offset;
                            let gy0 = py + glyph.y_offset;
                            let gx1 = gx0 + glyph.width as f32;
                            let gy1 = gy0 + glyph.height as f32;

                            let gx0_clip = (gx0 / screen_w) * 2.0 - 1.0;
                            let gy0_clip = 1.0 - (gy0 / screen_h) * 2.0;
                            let gx1_clip = (gx1 / screen_w) * 2.0 - 1.0;
                            let gy1_clip = 1.0 - (gy1 / screen_h) * 2.0;

                            let gb = vertices.len() as u32;
                            vertices.extend_from_slice(&[
                                Vertex { position: [gx0_clip, gy0_clip], uv: [u0, v0], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                                Vertex { position: [gx1_clip, gy0_clip], uv: [u1, v0], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                                Vertex { position: [gx1_clip, gy1_clip], uv: [u1, v1], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                                Vertex { position: [gx0_clip, gy1_clip], uv: [u0, v1], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                            ]);
                            indices.extend_from_slice(&[gb, gb + 1, gb + 2, gb, gb + 2, gb + 3]);
                        }
                    }
                }
            }

            // --- Cursor (only for active pane) ---
            if *is_active {
                let (cur_col, cur_row) = screen.cursor_position();
                let viewport_row = cur_row + scroll_offset;
                if cur_col < screen.width() && viewport_row < screen.height() {
                    let cpx = pane_x + cur_col as f32 * cell_w;
                    let cpy = pane_y + viewport_row as f32 * cell_h;

                    if cpx + cell_w <= pane_x + pane_w && cpy + cell_h <= pane_y + pane_h {
                        let cx0 = (cpx / screen_w) * 2.0 - 1.0;
                        let cy0 = 1.0 - (cpy / screen_h) * 2.0;
                        let cx1 = ((cpx + cell_w) / screen_w) * 2.0 - 1.0;
                        let cy1 = 1.0 - ((cpy + cell_h) / screen_h) * 2.0;

                        let cursor_color = [self.theme.cursor[0], self.theme.cursor[1], self.theme.cursor[2], 0.92];
                        let cb = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [cx0, cy0], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [cx1, cy0], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [cx1, cy1], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [cx0, cy1], uv: [0.0, 0.0], fg_color: cursor_color, bg_color: cursor_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[cb, cb + 1, cb + 2, cb, cb + 2, cb + 3]);

                        // Inverse glyph under cursor
                        let cell = screen.get_cell(cur_col, cur_row);
                        if cell.ch != ' ' {
                            let glyph = self.glyph_atlas.get_glyph(cell.ch);
                            if glyph.width > 0 && glyph.height > 0 {
                                let u0 = glyph.atlas_x as f32 / atlas_w;
                                let v0 = glyph.atlas_y as f32 / atlas_h;
                                let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_w;
                                let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_h;
                                let gx0 = cpx + glyph.x_offset;
                                let gy0 = cpy + glyph.y_offset;
                                let gx0c = (gx0 / screen_w) * 2.0 - 1.0;
                                let gy0c = 1.0 - (gy0 / screen_h) * 2.0;
                                let gx1c = ((gx0 + glyph.width as f32) / screen_w) * 2.0 - 1.0;
                                let gy1c = 1.0 - ((gy0 + glyph.height as f32) / screen_h) * 2.0;
                                let dark_fg = [0.05, 0.05, 0.08, 1.0];
                                let cgb = vertices.len() as u32;
                                vertices.extend_from_slice(&[
                                    Vertex { position: [gx0c, gy0c], uv: [u0, v0], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                                    Vertex { position: [gx1c, gy0c], uv: [u1, v0], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                                    Vertex { position: [gx1c, gy1c], uv: [u1, v1], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                                    Vertex { position: [gx0c, gy1c], uv: [u0, v1], fg_color: dark_fg, bg_color: cursor_color, is_glyph: 1.0, _padding: [0.0; 3] },
                                ]);
                                indices.extend_from_slice(&[cgb, cgb + 1, cgb + 2, cgb, cgb + 2, cgb + 3]);
                            }
                        }
                    }
                }
            }

            // --- Pane border (1px lines for splits) ---
            if panes.len() > 1 {
                let border_color = if *is_active {
                    [0.35, 0.55, 0.95, 1.0] // blue border for active
                } else {
                    [0.25, 0.25, 0.30, 1.0] // dim border
                };
                let border_w = 2.0;

                // Right edge
                if pane_x + pane_w < screen_w - 1.0 {
                    let bx0 = ((pane_x + pane_w - border_w) / screen_w) * 2.0 - 1.0;
                    let by0 = 1.0 - (pane_y / screen_h) * 2.0;
                    let bx1 = ((pane_x + pane_w) / screen_w) * 2.0 - 1.0;
                    let by1 = 1.0 - ((pane_y + pane_h) / screen_h) * 2.0;
                    let bb = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by1], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, by1], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[bb, bb + 1, bb + 2, bb, bb + 2, bb + 3]);
                }

                // Bottom edge
                if pane_y + pane_h < screen_h - 1.0 {
                    let bx0 = (pane_x / screen_w) * 2.0 - 1.0;
                    let by0 = 1.0 - ((pane_y + pane_h - border_w) / screen_h) * 2.0;
                    let bx1 = ((pane_x + pane_w) / screen_w) * 2.0 - 1.0;
                    let by1 = 1.0 - ((pane_y + pane_h) / screen_h) * 2.0;
                    let bb = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by1], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, by1], uv: [0.0, 0.0], fg_color: border_color, bg_color: border_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[bb, bb + 1, bb + 2, bb, bb + 2, bb + 3]);
                }
            }
        }

        // Append overlay vertices into the same batch
        self.append_overlay_vertices(&mut vertices, &mut indices, overlays);

        // Upload and draw
        if vertices.len() > self.max_vertices || indices.len() > self.max_indices {
            tracing::warn!("Buffer overflow — too many panes");
            return Ok(());
        }

        self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));

        let output = self.surface.get_current_texture().context("Failed to get surface texture")?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Multi-Pane Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Multi-Pane Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.08, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Append overlay element vertices into existing buffers (same render pass).
    fn append_overlay_vertices(
        &mut self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        elements: &[crate::renderer::overlay::OverlayElement],
    ) {
        if elements.is_empty() {
            return;
        }

        let screen_w = self.size.width as f32;
        let screen_h = self.size.height as f32;
        let cell_w = self.glyph_atlas.cell_width;
        let cell_h = self.glyph_atlas.cell_height;
        let atlas_w = self.glyph_atlas.atlas_width as f32;
        let atlas_h = self.glyph_atlas.atlas_height as f32;

        for element in elements {
            match element {
                crate::renderer::overlay::OverlayElement::StatusBar(bar) => {
                    let bar_h = cell_h + 4.0;
                    let bar_y = screen_h - bar_h;

                    let x0 = -1.0;
                    let y0 = 1.0 - (bar_y / screen_h) * 2.0;
                    let x1 = 1.0;
                    let y1 = -1.0;

                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [x0, y0], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x1, y0], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x1, y1], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x0, y1], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    self.render_text_at(vertices, indices, &bar.left, 4.0, bar_y + 2.0, bar.fg_color, bar.bg_color, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                    let right_x = screen_w - bar.right.len() as f32 * cell_w - 4.0;
                    self.render_text_at(vertices, indices, &bar.right, right_x, bar_y + 2.0, bar.fg_color, bar.bg_color, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                    if !bar.center.is_empty() {
                        let center_x = (screen_w - bar.center.len() as f32 * cell_w) / 2.0;
                        self.render_text_at(vertices, indices, &bar.center, center_x, bar_y + 2.0, bar.fg_color, bar.bg_color, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                    }
                }

                crate::renderer::overlay::OverlayElement::InlineDoc(doc) => {
                    if let crate::renderer::overlay::OverlayPosition::BelowCursor { col, row } = &doc.position {
                        let popup_x = (*col as f32 * cell_w).min(screen_w - 350.0).max(0.0);
                        let popup_y = (*row as f32 + 1.5) * cell_h;

                        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
                        lines.push((format!(" {} ", doc.command), [0.95, 0.80, 0.30, 1.0]));
                        if !doc.synopsis.is_empty() {
                            lines.push((format!(" {}", doc.synopsis), [0.70, 0.72, 0.78, 1.0]));
                        }
                        for flag in doc.flags.iter().take(5) {
                            lines.push((format!("  {} — {}", flag.flag, flag.description), [0.55, 0.80, 0.55, 1.0]));
                        }
                        if !doc.completions.is_empty() {
                            lines.push((" Completions:".to_string(), [0.50, 0.70, 1.0, 1.0]));
                            for c in doc.completions.iter().take(3) {
                                lines.push((format!("  > {}", c), [0.50, 0.70, 1.0, 1.0]));
                            }
                        }
                        if !doc.history.is_empty() {
                            lines.push((" History:".to_string(), [0.65, 0.55, 0.85, 1.0]));
                            for h in doc.history.iter().take(3) {
                                lines.push((format!("  ^ {}", h), [0.65, 0.55, 0.85, 1.0]));
                            }
                        }

                        let popup_w = 350.0f32;
                        let popup_h = (lines.len() as f32 + 0.5) * cell_h;
                        let bg = [0.12, 0.12, 0.18, 0.95];
                        let border_c = [0.30, 0.35, 0.50, 1.0];

                        let px0 = (popup_x / screen_w) * 2.0 - 1.0;
                        let py0 = 1.0 - (popup_y / screen_h) * 2.0;
                        let px1 = ((popup_x + popup_w) / screen_w) * 2.0 - 1.0;
                        let py1 = 1.0 - ((popup_y + popup_h) / screen_h) * 2.0;

                        let base = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [px0, py0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [px1, py0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [px1, py1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [px0, py1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                        for (i, (text, color)) in lines.iter().enumerate() {
                            let ly = popup_y + (i as f32 + 0.25) * cell_h;
                            self.render_text_at(vertices, indices, text, popup_x + 4.0, ly, *color, bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                        }
                    }
                }

                crate::renderer::overlay::OverlayElement::Toast(toast) => {
                    let toast_w = (toast.message.len() as f32 + 4.0) * cell_w.min(9.0);
                    let toast_h = cell_h + 6.0;
                    let toast_x = screen_w - toast_w - 10.0;
                    let toast_y = 10.0;
                    let bg_c = [0.15, 0.15, 0.22, 0.92];

                    let tx0 = (toast_x / screen_w) * 2.0 - 1.0;
                    let ty0 = 1.0 - (toast_y / screen_h) * 2.0;
                    let tx1 = ((toast_x + toast_w) / screen_w) * 2.0 - 1.0;
                    let ty1 = 1.0 - ((toast_y + toast_h) / screen_h) * 2.0;

                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [tx0, ty0], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [tx1, ty0], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [tx1, ty1], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [tx0, ty1], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    self.render_text_at(vertices, indices, &format!("  {}  ", toast.message), toast_x, toast_y + 3.0, toast.color, bg_c, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                }

                crate::renderer::overlay::OverlayElement::StructuredTable(table) => {
                    if table.columns.is_empty() || table.rows.is_empty() {
                        continue;
                    }

                    let padding = 8.0_f32;
                    let col_gap = cell_w * 2.0;

                    // Calculate table dimensions
                    let total_w: f32 = table.col_widths.iter().map(|w| *w as f32 * cell_w + col_gap).sum::<f32>() + padding * 2.0;
                    let total_w = total_w.min(screen_w - 20.0);
                    let row_h = cell_h + 4.0;
                    let header_h = row_h + 2.0;
                    let total_h = header_h + (table.rows.len() as f32) * row_h + padding;

                    // Position table below cursor position
                    let table_x = 10.0_f32;
                    let table_y = (table.start_row as f32) * cell_h + cell_h;
                    let table_y = table_y.min(screen_h - total_h - 30.0).max(0.0);

                    // Table background
                    let bg = [0.10, 0.10, 0.16, 0.95];
                    let header_bg = [0.18, 0.20, 0.30, 1.0];
                    let border_c = [0.30, 0.35, 0.50, 0.8];
                    let header_fg = [0.70, 0.85, 1.0, 1.0];
                    let cell_fg = [0.78, 0.80, 0.85, 1.0];
                    let alt_row_bg = [0.12, 0.12, 0.19, 0.95];

                    // Draw main background
                    let bx0 = (table_x / screen_w) * 2.0 - 1.0;
                    let by0 = 1.0 - (table_y / screen_h) * 2.0;
                    let bx1 = ((table_x + total_w) / screen_w) * 2.0 - 1.0;
                    let by1 = 1.0 - ((table_y + total_h) / screen_h) * 2.0;

                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, by1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    // Draw header background
                    let hy1 = 1.0 - ((table_y + header_h) / screen_h) * 2.0;
                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, hy1], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, hy1], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    // Draw header text
                    let mut col_x = table_x + padding;
                    for (i, col_name) in table.columns.iter().enumerate() {
                        let w = table.col_widths.get(i).copied().unwrap_or(8);
                        let display = if col_name.len() > w { &col_name[..w] } else { col_name };
                        self.render_text_at(vertices, indices, display, col_x, table_y + 3.0, header_fg, header_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                        col_x += w as f32 * cell_w + col_gap;
                    }

                    // Draw alternating row backgrounds and cell text
                    for (row_idx, row) in table.rows.iter().enumerate() {
                        let ry = table_y + header_h + (row_idx as f32) * row_h;
                        let row_bg = if row_idx % 2 == 0 { bg } else { alt_row_bg };

                        // Row background
                        let ry0 = 1.0 - (ry / screen_h) * 2.0;
                        let ry1 = 1.0 - ((ry + row_h) / screen_h) * 2.0;
                        let base = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [bx0, ry0], uv: [0.0, 0.0], fg_color: cell_fg, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx1, ry0], uv: [0.0, 0.0], fg_color: cell_fg, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx1, ry1], uv: [0.0, 0.0], fg_color: cell_fg, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx0, ry1], uv: [0.0, 0.0], fg_color: cell_fg, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                        // Cell text
                        let mut col_x = table_x + padding;
                        for (i, cell) in row.iter().enumerate() {
                            let w = table.col_widths.get(i).copied().unwrap_or(8);
                            let display: String = if cell.len() > w {
                                format!("{}…", &cell[..w.saturating_sub(1)])
                            } else {
                                cell.clone()
                            };
                            self.render_text_at(vertices, indices, &display, col_x, ry + 2.0, cell_fg, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                            col_x += w as f32 * cell_w + col_gap;
                        }
                    }

                    // Footer: row count
                    let footer_y = table_y + header_h + (table.rows.len() as f32) * row_h + 2.0;
                    let footer_text = format!(" {} rows ", table.rows.len());
                    let footer_fg = [0.50, 0.55, 0.65, 1.0];
                    self.render_text_at(vertices, indices, &footer_text, table_x + padding, footer_y, footer_fg, bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                }

                crate::renderer::overlay::OverlayElement::SuggestionCard(card) => {
                    if card.items.is_empty() {
                        continue;
                    }

                    let max_vis = card.max_visible.min(card.items.len());
                    let row_h = cell_h + 4.0;
                    let header_h = cell_h + 6.0;
                    let footer_h = cell_h + 2.0;
                    let card_w = 420.0_f32;
                    let card_h = header_h + (max_vis as f32) * row_h + footer_h;

                    // Position below cursor
                    let (cx, cy) = match &card.position {
                        crate::renderer::overlay::OverlayPosition::BelowCursor { col, row } => {
                            ((*col as f32 * cell_w).min(screen_w - card_w - 8.0).max(4.0),
                             (*row as f32 + 1.5) * cell_h)
                        }
                        _ => (10.0, screen_h * 0.4),
                    };
                    let card_x = cx;
                    let card_y = if cy + card_h > screen_h - cell_h * 2.0 {
                        (cy - card_h - cell_h).max(4.0)
                    } else {
                        cy
                    };

                    // Card background (glassmorphism)
                    let bg = [0.08, 0.08, 0.14, 0.96];
                    let border_c = [0.25, 0.35, 0.60, 0.8];
                    let bx0 = (card_x / screen_w) * 2.0 - 1.0;
                    let by0 = 1.0 - (card_y / screen_h) * 2.0;
                    let bx1 = ((card_x + card_w) / screen_w) * 2.0 - 1.0;
                    let by1 = 1.0 - ((card_y + card_h) / screen_h) * 2.0;
                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, by1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    // Header bar
                    let header_bg = [0.14, 0.16, 0.24, 1.0];
                    let header_fg = [0.70, 0.85, 1.0, 1.0];
                    let hy1 = 1.0 - ((card_y + header_h) / screen_h) * 2.0;
                    let hb = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, hy1], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, hy1], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[hb, hb + 1, hb + 2, hb, hb + 2, hb + 3]);

                    let header_text = format!(" {} — {} items", card.command, card.items.len());
                    self.render_text_at(vertices, indices, &header_text, card_x + 6.0, card_y + 3.0, header_fg, header_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                    // Items
                    let start = card.scroll_offset;
                    let end = (start + max_vis).min(card.items.len());
                    for (vi, idx) in (start..end).enumerate() {
                        let item = &card.items[idx];
                        let iy = card_y + header_h + (vi as f32) * row_h;
                        let is_selected = idx as i32 == card.selected_index;

                        let row_bg = if is_selected {
                            [0.22, 0.28, 0.45, 1.0] // bright selection
                        } else if vi % 2 == 0 {
                            bg
                        } else {
                            [0.10, 0.10, 0.17, 0.96]
                        };

                        // Row background
                        let ry0 = 1.0 - (iy / screen_h) * 2.0;
                        let ry1 = 1.0 - ((iy + row_h) / screen_h) * 2.0;
                        let rb = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [bx0, ry0], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx1, ry0], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx1, ry1], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx0, ry1], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[rb, rb + 1, rb + 2, rb, rb + 2, rb + 3]);

                        // Icon
                        let icon_str = format!(" {} ", item.icon);
                        self.render_text_at(vertices, indices, &icon_str, card_x + 4.0, iy + 2.0, item.color, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                        // Label
                        let label_x = card_x + cell_w * 4.0;
                        let label_fg = if is_selected {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [0.88, 0.90, 0.95, 1.0]
                        };
                        self.render_text_at(vertices, indices, &item.label, label_x, iy + 2.0, label_fg, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                        // Detail (right-aligned, dimmer)
                        if let Some(detail) = &item.detail {
                            let max_detail = 25;
                            let truncated = if detail.len() > max_detail {
                                format!("{}...", &detail[..max_detail])
                            } else {
                                detail.clone()
                            };
                            let detail_x = card_x + card_w - (truncated.len() as f32 + 1.0) * cell_w;
                            let detail_fg = [0.50, 0.52, 0.60, 0.8];
                            self.render_text_at(vertices, indices, &truncated, detail_x, iy + 2.0, detail_fg, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                        }
                    }

                    // Footer
                    let footer_y = card_y + header_h + (max_vis as f32) * row_h;
                    let footer_fg = [0.45, 0.48, 0.58, 1.0];
                    let footer_text = format!(" Tab to accept  |  {}/{}", card.selected_index + 1, card.items.len());
                    self.render_text_at(vertices, indices, &footer_text, card_x + 6.0, footer_y + 1.0, footer_fg, bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                }

                _ => {}
            }
        }
    }

    /// Render overlay elements (status bar, inline docs, toasts) as a second pass.
    /// NOTE: Prefer using the integrated overlay rendering via render()/render_multi().
    pub fn render_overlay_pass(&mut self, elements: &[crate::renderer::overlay::OverlayElement]) -> Result<()> {
        if elements.is_empty() {
            return Ok(());
        }

        let mut vertices: Vec<Vertex> = Vec::with_capacity(8_000);
        let mut indices: Vec<u32> = Vec::with_capacity(12_000);

        let screen_w = self.size.width as f32;
        let screen_h = self.size.height as f32;
        let cell_w = self.glyph_atlas.cell_width;
        let cell_h = self.glyph_atlas.cell_height;
        let atlas_w = self.glyph_atlas.atlas_width as f32;
        let atlas_h = self.glyph_atlas.atlas_height as f32;

        for element in elements {
            match element {
                crate::renderer::overlay::OverlayElement::StatusBar(bar) => {
                    // Draw status bar background at the bottom
                    let bar_h = cell_h + 4.0;
                    let bar_y = screen_h - bar_h;

                    let x0 = -1.0;
                    let y0 = 1.0 - (bar_y / screen_h) * 2.0;
                    let x1 = 1.0;
                    let y1 = -1.0;

                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [x0, y0], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x1, y0], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x1, y1], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [x0, y1], uv: [0.0, 0.0], fg_color: bar.fg_color, bg_color: bar.bg_color, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    // Render left text
                    self.render_text_at(&mut vertices, &mut indices, &bar.left, 4.0, bar_y + 2.0, bar.fg_color, bar.bg_color, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                    // Render right text
                    let right_x = screen_w - bar.right.len() as f32 * cell_w - 4.0;
                    self.render_text_at(&mut vertices, &mut indices, &bar.right, right_x, bar_y + 2.0, bar.fg_color, bar.bg_color, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                    // Render center text
                    if !bar.center.is_empty() {
                        let center_x = (screen_w - bar.center.len() as f32 * cell_w) / 2.0;
                        self.render_text_at(&mut vertices, &mut indices, &bar.center, center_x, bar_y + 2.0, bar.fg_color, bar.bg_color, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                    }
                }

                crate::renderer::overlay::OverlayElement::InlineDoc(doc) => {
                    // Draw inline doc popup below cursor
                    if let crate::renderer::overlay::OverlayPosition::BelowCursor { col, row } = &doc.position {
                        let popup_x = (*col as f32 * cell_w).min(screen_w - 350.0).max(0.0);
                        let popup_y = (*row as f32 + 1.5) * cell_h;

                        // Build content lines
                        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
                        lines.push((format!(" {} ", doc.command), [0.95, 0.80, 0.30, 1.0])); // yellow header
                        if !doc.synopsis.is_empty() {
                            lines.push((format!(" {}", doc.synopsis), [0.70, 0.72, 0.78, 1.0]));
                        }
                        for flag in doc.flags.iter().take(5) {
                            lines.push((format!("  {} — {}", flag.flag, flag.description), [0.55, 0.80, 0.55, 1.0]));
                        }
                        if !doc.completions.is_empty() {
                            lines.push((" Completions:".to_string(), [0.50, 0.70, 1.0, 1.0]));
                            for c in doc.completions.iter().take(3) {
                                lines.push((format!("  → {}", c), [0.50, 0.70, 1.0, 1.0]));
                            }
                        }
                        if !doc.history.is_empty() {
                            lines.push((" History:".to_string(), [0.65, 0.55, 0.85, 1.0]));
                            for h in doc.history.iter().take(3) {
                                lines.push((format!("  ↑ {}", h), [0.65, 0.55, 0.85, 1.0]));
                            }
                        }

                        let popup_w = 350.0f32;
                        let popup_h = (lines.len() as f32 + 0.5) * cell_h;
                        let bg = [0.12, 0.12, 0.18, 0.95];
                        let border_c = [0.30, 0.35, 0.50, 1.0];

                        // Background
                        let px0 = (popup_x / screen_w) * 2.0 - 1.0;
                        let py0 = 1.0 - (popup_y / screen_h) * 2.0;
                        let px1 = ((popup_x + popup_w) / screen_w) * 2.0 - 1.0;
                        let py1 = 1.0 - ((popup_y + popup_h) / screen_h) * 2.0;

                        let base = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [px0, py0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [px1, py0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [px1, py1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [px0, py1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                        // Render text lines
                        for (i, (text, color)) in lines.iter().enumerate() {
                            let ly = popup_y + (i as f32 + 0.25) * cell_h;
                            self.render_text_at(&mut vertices, &mut indices, text, popup_x + 4.0, ly, *color, bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                        }
                    }
                }

                crate::renderer::overlay::OverlayElement::Toast(toast) => {
                    // Draw toast top-right
                    let toast_w = (toast.message.len() as f32 + 4.0) * cell_w.min(9.0);
                    let toast_h = cell_h + 6.0;
                    let toast_x = screen_w - toast_w - 10.0;
                    let toast_y = 10.0;

                    let bg_c = [0.15, 0.15, 0.22, 0.92];

                    let tx0 = (toast_x / screen_w) * 2.0 - 1.0;
                    let ty0 = 1.0 - (toast_y / screen_h) * 2.0;
                    let tx1 = ((toast_x + toast_w) / screen_w) * 2.0 - 1.0;
                    let ty1 = 1.0 - ((toast_y + toast_h) / screen_h) * 2.0;

                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [tx0, ty0], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [tx1, ty0], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [tx1, ty1], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [tx0, ty1], uv: [0.0, 0.0], fg_color: toast.color, bg_color: bg_c, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    self.render_text_at(&mut vertices, &mut indices, &format!("  {}  ", toast.message), toast_x, toast_y + 3.0, toast.color, bg_c, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                }

                crate::renderer::overlay::OverlayElement::SuggestionCard(card) => {
                    if card.items.is_empty() {
                        continue;
                    }

                    let max_vis = card.max_visible.min(card.items.len());
                    let row_h = cell_h + 4.0;
                    let header_h = cell_h + 6.0;
                    let footer_h = cell_h + 2.0;
                    let card_w = 420.0_f32;
                    let card_h = header_h + (max_vis as f32) * row_h + footer_h;

                    let (cx, cy) = match &card.position {
                        crate::renderer::overlay::OverlayPosition::BelowCursor { col, row } => {
                            ((*col as f32 * cell_w).min(screen_w - card_w - 8.0).max(4.0),
                             (*row as f32 + 1.5) * cell_h)
                        }
                        _ => (10.0, screen_h * 0.4),
                    };
                    let card_x = cx;
                    let card_y = if cy + card_h > screen_h - cell_h * 2.0 {
                        (cy - card_h - cell_h).max(4.0)
                    } else {
                        cy
                    };

                    let bg = [0.08, 0.08, 0.14, 0.96];
                    let border_c = [0.25, 0.35, 0.60, 0.8];
                    let bx0 = (card_x / screen_w) * 2.0 - 1.0;
                    let by0 = 1.0 - (card_y / screen_h) * 2.0;
                    let bx1 = ((card_x + card_w) / screen_w) * 2.0 - 1.0;
                    let by1 = 1.0 - ((card_y + card_h) / screen_h) * 2.0;
                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, by1], uv: [0.0, 0.0], fg_color: border_c, bg_color: bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    let header_bg = [0.14, 0.16, 0.24, 1.0];
                    let header_fg = [0.70, 0.85, 1.0, 1.0];
                    let hy1 = 1.0 - ((card_y + header_h) / screen_h) * 2.0;
                    let hb = vertices.len() as u32;
                    vertices.extend_from_slice(&[
                        Vertex { position: [bx0, by0], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, by0], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx1, hy1], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        Vertex { position: [bx0, hy1], uv: [0.0, 0.0], fg_color: header_fg, bg_color: header_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                    ]);
                    indices.extend_from_slice(&[hb, hb + 1, hb + 2, hb, hb + 2, hb + 3]);

                    let header_text = format!(" {} — {} items", card.command, card.items.len());
                    self.render_text_at(&mut vertices, &mut indices, &header_text, card_x + 6.0, card_y + 3.0, header_fg, header_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                    let start = card.scroll_offset;
                    let end = (start + max_vis).min(card.items.len());
                    for (vi, idx) in (start..end).enumerate() {
                        let item = &card.items[idx];
                        let iy = card_y + header_h + (vi as f32) * row_h;
                        let is_selected = idx as i32 == card.selected_index;

                        let row_bg = if is_selected {
                            [0.22, 0.28, 0.45, 1.0]
                        } else if vi % 2 == 0 {
                            bg
                        } else {
                            [0.10, 0.10, 0.17, 0.96]
                        };

                        let ry0 = 1.0 - (iy / screen_h) * 2.0;
                        let ry1 = 1.0 - ((iy + row_h) / screen_h) * 2.0;
                        let rb = vertices.len() as u32;
                        vertices.extend_from_slice(&[
                            Vertex { position: [bx0, ry0], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx1, ry0], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx1, ry1], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                            Vertex { position: [bx0, ry1], uv: [0.0, 0.0], fg_color: item.color, bg_color: row_bg, is_glyph: 0.0, _padding: [0.0; 3] },
                        ]);
                        indices.extend_from_slice(&[rb, rb + 1, rb + 2, rb, rb + 2, rb + 3]);

                        let icon_str = format!(" {} ", item.icon);
                        self.render_text_at(&mut vertices, &mut indices, &icon_str, card_x + 4.0, iy + 2.0, item.color, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                        let label_x = card_x + cell_w * 4.0;
                        let label_fg = if is_selected {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [0.88, 0.90, 0.95, 1.0]
                        };
                        self.render_text_at(&mut vertices, &mut indices, &item.label, label_x, iy + 2.0, label_fg, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);

                        if let Some(detail) = &item.detail {
                            let max_detail = 25;
                            let truncated = if detail.len() > max_detail {
                                format!("{}...", &detail[..max_detail])
                            } else {
                                detail.clone()
                            };
                            let detail_x = card_x + card_w - (truncated.len() as f32 + 1.0) * cell_w;
                            let detail_fg = [0.50, 0.52, 0.60, 0.8];
                            self.render_text_at(&mut vertices, &mut indices, &truncated, detail_x, iy + 2.0, detail_fg, row_bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                        }
                    }

                    let footer_y = card_y + header_h + (max_vis as f32) * row_h;
                    let footer_fg = [0.45, 0.48, 0.58, 1.0];
                    let footer_text = format!(" Tab to accept  |  {}/{}", card.selected_index + 1, card.items.len());
                    self.render_text_at(&mut vertices, &mut indices, &footer_text, card_x + 6.0, footer_y + 1.0, footer_fg, bg, screen_w, screen_h, atlas_w, atlas_h, cell_w);
                }

                _ => {} // ConsequenceWarning, MutationPreview handled similarly
            }
        }

        if vertices.is_empty() {
            return Ok(());
        }

        if vertices.len() > self.max_vertices || indices.len() > self.max_indices {
            return Ok(());
        }

        self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));

        let output = self.surface.get_current_texture().context("Failed to get surface texture for overlay")?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Overlay Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // DON'T clear — draw on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Helper: render a text string at pixel position, appending to vertex/index buffers.
    fn render_text_at(
        &mut self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        text: &str,
        start_x: f32,
        start_y: f32,
        fg: [f32; 4],
        bg: [f32; 4],
        screen_w: f32,
        screen_h: f32,
        atlas_w: f32,
        atlas_h: f32,
        cell_w: f32,
    ) {
        for (i, ch) in text.chars().enumerate() {
            if ch == ' ' { continue; }
            let glyph = self.glyph_atlas.get_glyph(ch);
            if glyph.width == 0 || glyph.height == 0 { continue; }

            let u0 = glyph.atlas_x as f32 / atlas_w;
            let v0 = glyph.atlas_y as f32 / atlas_h;
            let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_w;
            let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_h;

            let gx0 = start_x + i as f32 * cell_w + glyph.x_offset;
            let gy0 = start_y + glyph.y_offset;
            let gx1 = gx0 + glyph.width as f32;
            let gy1 = gy0 + glyph.height as f32;

            let gx0c = (gx0 / screen_w) * 2.0 - 1.0;
            let gy0c = 1.0 - (gy0 / screen_h) * 2.0;
            let gx1c = (gx1 / screen_w) * 2.0 - 1.0;
            let gy1c = 1.0 - (gy1 / screen_h) * 2.0;

            let gb = vertices.len() as u32;
            vertices.extend_from_slice(&[
                Vertex { position: [gx0c, gy0c], uv: [u0, v0], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                Vertex { position: [gx1c, gy0c], uv: [u1, v0], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                Vertex { position: [gx1c, gy1c], uv: [u1, v1], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
                Vertex { position: [gx0c, gy1c], uv: [u0, v1], fg_color: fg, bg_color: bg, is_glyph: 1.0, _padding: [0.0; 3] },
            ]);
            indices.extend_from_slice(&[gb, gb + 1, gb + 2, gb, gb + 2, gb + 3]);
        }
    }
}

/// Convert a foreground Color to RGBA.
/// Default foreground = light text on dark background.
fn fg_color_to_rgba(color: Color) -> [f32; 4] {
    match color {
        Color::Default => [0.80, 0.82, 0.88, 1.0], // Soft white text
        other => shared_color_to_rgba(other),
    }
}

/// Convert a background Color to RGBA.
/// Default background = dark terminal background.
fn bg_color_to_rgba(color: Color) -> [f32; 4] {
    match color {
        Color::Default => [0.10, 0.10, 0.14, 1.0], // Dark background (#1a1a24)
        other => shared_color_to_rgba(other),
    }
}

/// Shared color lookup for non-Default colors.
fn shared_color_to_rgba(color: Color) -> [f32; 4] {
    match color {
        Color::Default => [0.85, 0.85, 0.85, 1.0],
        Color::Black => [0.0, 0.0, 0.0, 1.0],
        Color::Red => [0.8, 0.2, 0.2, 1.0],
        Color::Green => [0.2, 0.8, 0.3, 1.0],
        Color::Yellow => [0.8, 0.75, 0.2, 1.0],
        Color::Blue => [0.3, 0.5, 0.9, 1.0],
        Color::Magenta => [0.8, 0.3, 0.8, 1.0],
        Color::Cyan => [0.3, 0.8, 0.8, 1.0],
        Color::White => [0.9, 0.9, 0.9, 1.0],
        Color::BrightBlack => [0.45, 0.45, 0.45, 1.0],
        Color::BrightRed => [1.0, 0.35, 0.35, 1.0],
        Color::BrightGreen => [0.35, 1.0, 0.45, 1.0],
        Color::BrightYellow => [1.0, 1.0, 0.35, 1.0],
        Color::BrightBlue => [0.5, 0.7, 1.0, 1.0],
        Color::BrightMagenta => [1.0, 0.5, 1.0, 1.0],
        Color::BrightCyan => [0.5, 1.0, 1.0, 1.0],
        Color::BrightWhite => [1.0, 1.0, 1.0, 1.0],
        Color::Rgb(r, g, b) => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0],
        Color::Indexed(i) => index_to_rgba(i),
    }
}

/// Convert 256-color index to RGBA.
fn index_to_rgba(idx: u8) -> [f32; 4] {
    match idx {
        // Standard colors (0-7)
        0 => [0.0, 0.0, 0.0, 1.0],
        1 => [0.8, 0.2, 0.2, 1.0],
        2 => [0.2, 0.8, 0.3, 1.0],
        3 => [0.8, 0.75, 0.2, 1.0],
        4 => [0.3, 0.5, 0.9, 1.0],
        5 => [0.8, 0.3, 0.8, 1.0],
        6 => [0.3, 0.8, 0.8, 1.0],
        7 => [0.9, 0.9, 0.9, 1.0],
        // Bright colors (8-15)
        8 => [0.45, 0.45, 0.45, 1.0],
        9 => [1.0, 0.35, 0.35, 1.0],
        10 => [0.35, 1.0, 0.45, 1.0],
        11 => [1.0, 1.0, 0.35, 1.0],
        12 => [0.5, 0.7, 1.0, 1.0],
        13 => [1.0, 0.5, 1.0, 1.0],
        14 => [0.5, 1.0, 1.0, 1.0],
        15 => [1.0, 1.0, 1.0, 1.0],
        // 216-color cube (16-231)
        16..=231 => {
            let n = idx - 16;
            let r = (n / 36) as f32;
            let g = ((n % 36) / 6) as f32;
            let b = (n % 6) as f32;
            [
                if r > 0.0 { (r * 40.0 + 55.0) / 255.0 } else { 0.0 },
                if g > 0.0 { (g * 40.0 + 55.0) / 255.0 } else { 0.0 },
                if b > 0.0 { (b * 40.0 + 55.0) / 255.0 } else { 0.0 },
                1.0,
            ]
        }
        // Greyscale (232-255)
        232..=255 => {
            let v = (8 + 10 * (idx - 232) as u32) as f32 / 255.0;
            [v, v, v, 1.0]
        }
    }
}
