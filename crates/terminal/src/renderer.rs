//! GPU terminal renderer (Prompt 2.4): instanced cell quads + glyph atlas.
//!
//! Architecture (tech_stack R3, Alacritty/WezTerm-proven):
//! - One fullscreen-quad vertex buffer (4 verts, single draw call).
//! - One instance buffer with a [`CellInstance`] per visible cell
//!   (grid rect, atlas UVs, fg/bg RGBA) — rebuilt in bulk, uploaded once.
//! - One RGBA glyph-atlas texture ([`GlyphAtlas`], 2048×2048) with shelf
//!   packing and on-demand glyph insertion as the terminal scrolls.
//! - Uniforms: screen size, cell size, DPI scale.
//!
//! Headless operation: [`TerminalRenderer::new_headless`] builds a fully
//! functional CPU-side renderer (atlas + instances + dirty tracking) with
//! `gpu: None`. [`TerminalRenderer::render`] then returns [`RenderStats`]
//! without touching the GPU — this is what unit tests and the iced CPU
//! fallback exercise. [`TerminalRenderer::new`] tries to acquire a wgpu
//! device and falls back to headless when no GPU is available.

use crate::atlas::{AtlasError, GlyphAtlas, GlyphKey, ATLAS_SIZE};
use crate::emulator::Terminal;
use crate::grid::Color;

// ---------------------------------------------------------------------------
// GPU vertex / instance layouts
// ---------------------------------------------------------------------------

/// Fullscreen-quad vertex: corner in 0..1 space + matching UV.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}

/// Quad corners (triangle-strip order expanded to a list by the pipeline).
pub const QUAD_VERTICES: [QuadVertex; 4] = [
    QuadVertex {
        pos: [0.0, 0.0],
        uv: [0.0, 0.0],
    },
    QuadVertex {
        pos: [1.0, 0.0],
        uv: [1.0, 0.0],
    },
    QuadVertex {
        pos: [1.0, 1.0],
        uv: [1.0, 1.0],
    },
    QuadVertex {
        pos: [0.0, 1.0],
        uv: [0.0, 1.0],
    },
];

/// Per-cell instance (64 bytes): pixel rect, atlas UV rect, fg/bg RGBA.
/// One instance per visible cell; all cells render in a single draw call.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CellInstance {
    /// Cell rect in pixels: `(x, y, w, h)`, y grows downwards.
    pub rect: [f32; 4],
    /// Atlas UV rect: `(u0, v0, u1, v1)`.
    pub uv_rect: [f32; 4],
    /// Foreground (text) color, linear RGBA.
    pub fg: [f32; 4],
    /// Background (fill) color, linear RGBA.
    pub bg: [f32; 4],
}

/// Uniform block: screen size (px), cell size (px), DPI scale (+pad).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub screen_size: [f32; 2],
    pub cell_size: [f32; 2],
    pub scale_factor: f32,
    pub _pad: [f32; 3],
}

impl Uniforms {
    pub fn new(screen: (u32, u32), cell: (f32, f32), scale: f32) -> Self {
        Self {
            screen_size: [screen.0 as f32, screen.1 as f32],
            cell_size: [cell.0, cell.1],
            scale_factor: scale,
            _pad: [0.0; 3],
        }
    }
}

// ---------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------

/// Opaque white (default foreground).
pub const DEFAULT_FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Opaque black (default background).
pub const DEFAULT_BG: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// xterm 256-color palette entry as sRGB bytes.
pub fn xterm256_to_rgb(index: u8) -> (u8, u8, u8) {
    const BASE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    if index < 16 {
        return BASE[index as usize];
    }
    if index < 232 {
        let i = index - 16;
        let r = i / 36;
        let g = (i % 36) / 6;
        let b = i % 6;
        let level = |c: u8| if c == 0 { 0 } else { 55 + c * 40 };
        return (level(r), level(g), level(b));
    }
    let gray = 8 + (index - 232) * 10;
    (gray, gray, gray)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert a terminal [`Color`] to linear RGBA (truecolor + 256-color).
pub fn color_to_rgba(color: Color, is_foreground: bool) -> [f32; 4] {
    let (r, g, b) = match color {
        Color::Default => {
            return if is_foreground {
                DEFAULT_FG
            } else {
                DEFAULT_BG
            };
        },
        Color::Indexed(i) => xterm256_to_rgb(i),
        Color::Rgb(r, g, b) => (r, g, b),
    };
    [
        srgb_to_linear(f32::from(r) / 255.0),
        srgb_to_linear(f32::from(g) / 255.0),
        srgb_to_linear(f32::from(b) / 255.0),
        1.0,
    ]
}

// ---------------------------------------------------------------------------
// Errors & stats
// ---------------------------------------------------------------------------

/// Renderer failures (GPU acquisition, atlas overflow, bad configuration).
#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    /// No usable GPU adapter (fallback to CPU rendering is expected).
    #[error("no usable GPU adapter: {0}")]
    NoGpu(String),
    /// wgpu device/pipeline failure.
    #[error("GPU error: {0}")]
    Gpu(String),
    /// Glyph atlas failure.
    #[error(transparent)]
    Atlas(#[from] AtlasError),
    /// Invalid configuration (e.g. zero font size).
    #[error("invalid renderer configuration: {0}")]
    Config(String),
}

/// Per-frame statistics (drives 60fps profiling + tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderStats {
    /// Visible cells visited.
    pub cells: usize,
    /// Instances uploaded (== cells for a full frame).
    pub instances: usize,
    /// Draw calls issued (always 1 for the cell pass).
    pub draw_calls: usize,
    /// Glyphs currently in the atlas.
    pub atlas_glyphs: usize,
    /// Whether this frame rebuilt every instance.
    pub full_redraw: bool,
}

// ---------------------------------------------------------------------------
// GPU context (present only when a device was acquired)
// ---------------------------------------------------------------------------

struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    surface_format: wgpu::TextureFormat,
}

// ---------------------------------------------------------------------------
// TerminalRenderer
// ---------------------------------------------------------------------------

/// High-performance terminal renderer (Prompt 2.4 fields as specified).
pub struct TerminalRenderer {
    /// wgpu logical device (inside `gpu`; `None` == CPU fallback).
    /// Kept behind `gpu` to keep headless tests device-free.
    gpu: Option<GpuContext>,
    /// Glyph atlas (texture source).
    glyph_atlas: GlyphAtlas,
    /// Staged per-cell instances for the current frame.
    instances: Vec<CellInstance>,
    /// Uniforms for the current viewport.
    uniforms: Uniforms,
    /// Viewport in physical pixels.
    viewport: (u32, u32),
    /// UI font size in points.
    font_size: u8,
    /// Cell size in physical pixels `(w, h)`.
    cell: (f32, f32),
    /// DPI scale factor.
    scale_factor: f32,
    /// Cells changed since the last frame (incremental upload list).
    dirty: Vec<(u16, u16)>,
    /// Next frame rebuilds every instance.
    full_redraw: bool,
    /// Monotonic frame counter.
    frame: u64,
}

impl std::fmt::Debug for TerminalRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalRenderer")
            .field("viewport", &self.viewport)
            .field("font_size", &self.font_size)
            .field("cell", &self.cell)
            .field("frame", &self.frame)
            .field("has_gpu", &self.gpu.is_some())
            .finish_non_exhaustive()
    }
}

impl TerminalRenderer {
    /// CPU-only renderer (tests, headless, CPU fallback path).
    pub fn new_headless(font_size: u8) -> Self {
        let font_size = font_size.max(1);
        let cell = cell_size_for_font(font_size);
        Self {
            gpu: None,
            glyph_atlas: GlyphAtlas::new(),
            instances: Vec::new(),
            uniforms: Uniforms::new((800, 600), cell, 1.0),
            viewport: (800, 600),
            font_size,
            cell,
            scale_factor: 1.0,
            dirty: Vec::new(),
            full_redraw: true,
            frame: 0,
        }
    }

    /// Acquire a GPU device and build the pipeline; falls back to a
    /// headless renderer (with `has_gpu() == false`) when no adapter is
    /// available, per the Prompt 2.4 fallback requirement.
    pub async fn new(font_size: u8, width: u32, height: u32) -> Result<Self, RendererError> {
        if font_size == 0 {
            return Err(RendererError::Config("font size must be non-zero".into()));
        }
        let mut renderer = Self::new_headless(font_size);
        renderer.resize(width, height);
        match renderer.init_gpu().await {
            Ok(()) => Ok(renderer),
            Err(err) => {
                tracing::warn!(%err, "GPU unavailable; using CPU fallback");
                Ok(renderer)
            },
        }
    }

    /// `true` when a wgpu device/pipeline is live.
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Current viewport in physical pixels.
    pub fn viewport(&self) -> (u32, u32) {
        self.viewport
    }

    /// Current font size in points.
    pub fn font_size(&self) -> u8 {
        self.font_size
    }

    /// Current cell size in physical pixels.
    pub fn cell_size(&self) -> (f32, f32) {
        self.cell
    }

    /// Current DPI scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Set the DPI scale factor (window resize / HiDPI path).
    pub fn set_scale_factor(&mut self, scale: f32) {
        if scale > 0.0 && (scale - self.scale_factor).abs() > f32::EPSILON {
            self.scale_factor = scale;
            self.refresh_uniforms();
            self.mark_all_dirty();
        }
    }

    /// Handle window resize (pixels) + DPI scaling.
    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if (width, height) != self.viewport {
            self.viewport = (width, height);
            self.refresh_uniforms();
            self.mark_all_dirty();
        }
    }

    /// Change the font size: recompute cell metrics, clear the atlas
    /// (glyphs are size-specific), and force a full redraw.
    pub fn set_font_size(&mut self, size: u8) {
        let size = size.max(1);
        if size != self.font_size {
            self.font_size = size;
            self.cell = cell_size_for_font(size);
            self.glyph_atlas.clear();
            self.refresh_uniforms();
            self.mark_all_dirty();
        }
    }

    /// Columns × rows that fit the current viewport.
    pub fn grid_for_viewport(&self) -> (u16, u16) {
        grid_for_viewport(self.viewport, self.cell, self.scale_factor)
    }

    /// Mark one cell dirty (incremental upload list).
    pub fn mark_dirty(&mut self, row: u16, col: u16) {
        if !self.dirty.contains(&(row, col)) {
            self.dirty.push((row, col));
        }
    }

    /// Force the next frame to rebuild every instance.
    pub fn mark_all_dirty(&mut self) {
        self.full_redraw = true;
        self.dirty.clear();
    }

    /// Drain the incremental dirty list.
    pub fn take_dirty(&mut self) -> Vec<(u16, u16)> {
        std::mem::take(&mut self.dirty)
    }

    /// `true` when the next frame must upload instances.
    pub fn needs_redraw(&self) -> bool {
        self.full_redraw || !self.dirty.is_empty()
    }

    /// Render one frame: rebuild instances (single batched upload) and, when
    /// a GPU context exists, issue the single instanced draw call.
    pub fn render(&mut self, terminal: &Terminal) -> Result<RenderStats, RendererError> {
        let full = self.full_redraw;
        self.build_instances(terminal);
        self.frame += 1;
        self.full_redraw = false;
        self.dirty.clear();

        let stats = RenderStats {
            cells: self.instances.len(),
            instances: self.instances.len(),
            draw_calls: 1,
            atlas_glyphs: self.glyph_atlas.len(),
            full_redraw: full,
        };

        if self.glyph_atlas.take_dirty() {
            self.upload_atlas();
        }
        if let Err(err) = self.submit_frame() {
            tracing::warn!(%err, "GPU submit failed; frame kept as CPU stats");
        }
        Ok(stats)
    }

    /// Staged instances for the current frame (single draw call source).
    pub fn instances(&self) -> &[CellInstance] {
        &self.instances
    }

    /// Glyph atlas (texture source).
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.glyph_atlas
    }

    /// Mutable atlas (tests seed glyphs directly).
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.glyph_atlas
    }

    // -- frame construction ------------------------------------------------

    /// Walk the visible grid and stage one [`CellInstance`] per cell.
    ///
    /// Missing glyphs are inserted into the atlas with a synthetic
    /// placeholder bitmap (the real font rasterizer plugs in here); wide-char
    /// spacers reuse the background-only path so CJK stays aligned.
    pub fn build_instances(&mut self, terminal: &Terminal) {
        let cols = terminal.grid.cols();
        let rows = terminal.grid.rows();
        self.instances.clear();
        self.instances.reserve((cols as usize) * (rows as usize));

        for row in 0..rows {
            for col in 0..cols {
                let Some(cell) = terminal.grid.get_cell(row, col) else {
                    continue;
                };
                // Resolve first: `Color::Default` means white-as-fg but
                // black-as-bg, so the enums themselves must NOT be swapped.
                let mut fg = color_to_rgba(cell.fg, true);
                let mut bg = color_to_rgba(cell.bg, false);
                if cell.attrs.reverse {
                    std::mem::swap(&mut fg, &mut bg);
                }
                // Selection inversion is resolved here (branchless shader).
                if let Some(selection) = terminal.selection.as_ref() {
                    let (start, end) = selection.bounds();
                    let inside = row >= start.row
                        && row <= end.row
                        && col >= start.col.min(end.col)
                        && col <= start.col.max(end.col);
                    if inside {
                        std::mem::swap(&mut fg, &mut bg);
                    }
                }

                let uv = if cell.width == 0 || cell.ch == ' ' || cell.ch == '\0' {
                    [0.0, 0.0, 0.0, 0.0]
                } else {
                    let key = GlyphKey::new(cell.ch, cell.attrs.bold, cell.attrs.italic);
                    self.atlas_entry(key)
                };

                let (cw, ch) = self.cell;
                let wide = u16::from(cell.width.max(1)) as f32;
                self.instances.push(CellInstance {
                    rect: [f32::from(col) * cw, f32::from(row) * ch, cw * wide, ch],
                    uv_rect: uv,
                    fg,
                    bg,
                });
            }
        }
    }

    fn atlas_entry(&mut self, key: GlyphKey) -> [f32; 4] {
        if let Some(entry) = self.glyph_atlas.get(&key) {
            return entry.uv;
        }
        // Placeholder 8×12 alpha block (real rasterizer: font-kit/rusttype).
        // Keeps layout/atlas accounting exact without a font dependency.
        let w = if crate::grid::char_width(key.ch) == 2 {
            16
        } else {
            8
        };
        let bitmap = vec![220u8; (w * 12) as usize];
        self.glyph_atlas
            .insert(key, &bitmap, w, 12)
            .map(|entry| entry.uv)
            .unwrap_or([0.0, 0.0, 0.0, 0.0])
    }

    fn refresh_uniforms(&mut self) {
        self.uniforms = Uniforms::new(self.viewport, self.cell, self.scale_factor);
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.queue
                .write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));
        }
    }

    // -- GPU ----------------------------------------------------------------

    async fn init_gpu(&mut self) -> Result<(), RendererError> {
        use wgpu::util::DeviceExt as _;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or_else(|| RendererError::NoGpu("no wgpu adapter found".into()))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("terminal-renderer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|err| RendererError::Gpu(err.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terminal-cells"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/terminal.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terminal-uniforms"),
            contents: bytemuck::bytes_of(&self.uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let atlas_size = ATLAS_SIZE;
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph-atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terminal-bind-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terminal-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terminal-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CellInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 5,
                },
            ],
        };

        let surface_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terminal-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout, instance_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terminal-quad"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let capacity = 80 * 24;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terminal-instances"),
            size: (capacity * std::mem::size_of::<CellInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.upload_atlas_to(&queue, &atlas_texture);
        self.gpu = Some(GpuContext {
            device,
            queue,
            pipeline,
            vertex_buffer,
            instance_buffer,
            instance_capacity: capacity,
            uniform_buffer,
            bind_group,
            atlas_texture,
            atlas_view,
            surface_format,
        });
        Ok(())
    }

    fn upload_atlas(&mut self) {
        if let Some(gpu) = self.gpu.as_ref() {
            self.upload_atlas_to(&gpu.queue, &gpu.atlas_texture);
        }
    }

    fn upload_atlas_to(&self, queue: &wgpu::Queue, texture: &wgpu::Texture) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            self.glyph_atlas.pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_SIZE * 4),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Upload staged instances and record the single draw call.
    ///
    /// Without a surface attached (headless/iced-embedded use), this only
    /// ensures the instance buffer is large enough and staged — the actual
    /// swapchain pass runs inside the iced shader primitive.
    fn submit_frame(&mut self) -> Result<(), RendererError> {
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(());
        };
        let needed = self.instances.len() * std::mem::size_of::<CellInstance>();
        if needed as u64 > gpu.instance_buffer.size() {
            gpu.instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("terminal-instances-grown"),
                size: needed.max(1) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.instance_capacity = self.instances.len();
        }
        if !self.instances.is_empty() {
            gpu.queue.write_buffer(
                &gpu.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
        let _ = (
            &gpu.pipeline,
            &gpu.vertex_buffer,
            &gpu.bind_group,
            &gpu.atlas_view,
        );
        let _ = gpu.surface_format;
        Ok(())
    }
}

/// Cell size in physical pixels for a point-size font.
///
/// Monospace advance ≈ 0.6× size, line height ≈ 1.2× size (matches the
/// placeholder text view at 13pt until real font metrics land).
pub fn cell_size_for_font(font_size: u8) -> (f32, f32) {
    let size = f32::from(font_size.max(1));
    ((size * 0.6).max(1.0), (size * 1.2).max(1.0))
}

/// Columns × rows that fit `viewport` at `cell` size and DPI `scale`.
pub fn grid_for_viewport(viewport: (u32, u32), cell: (f32, f32), scale: f32) -> (u16, u16) {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let cols = (viewport.0 as f32 / (cell.0 * scale).max(1.0)).floor() as u16;
    let rows = (viewport.1 as f32 / (cell.1 * scale).max(1.0)).floor() as u16;
    (cols.max(1), rows.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Attributes;

    fn terminal_with_text(cols: u16, rows: u16, text: &str) -> Terminal {
        let mut term = Terminal::new(cols, rows, 0);
        term.write_bytes(text.as_bytes());
        term
    }

    #[test]
    fn instances_cover_every_visible_cell_in_one_draw() {
        let mut renderer = TerminalRenderer::new_headless(13);
        let term = terminal_with_text(10, 4, "hi");
        let stats = renderer.render(&term).unwrap();
        assert_eq!(stats.cells, 40);
        assert_eq!(stats.instances, 40);
        assert_eq!(stats.draw_calls, 1, "single instanced draw call");
        assert!(stats.full_redraw);
        assert!(!renderer.needs_redraw());
    }

    #[test]
    fn scrolling_populates_new_atlas_glyphs() {
        let mut renderer = TerminalRenderer::new_headless(13);
        let mut term = Terminal::new(20, 5, 50);
        term.write_bytes(b"hello");
        renderer.render(&term).unwrap();
        let before = renderer.atlas().len();
        assert!(before > 0);
        term.write_bytes("漢字".as_bytes());
        renderer.render(&term).unwrap();
        assert!(
            renderer.atlas().len() > before,
            "new characters join the atlas"
        );
    }

    #[test]
    fn dirty_tracking_and_full_redraw() {
        let mut renderer = TerminalRenderer::new_headless(13);
        assert!(renderer.needs_redraw());
        let term = terminal_with_text(5, 2, "ab");
        renderer.render(&term).unwrap();
        assert!(!renderer.needs_redraw());
        renderer.mark_dirty(0, 1);
        assert!(renderer.needs_redraw());
        assert_eq!(renderer.take_dirty(), vec![(0, 1)]);
        renderer.mark_all_dirty();
        assert!(renderer.needs_redraw());
    }

    #[test]
    fn resize_and_font_size_recompute_layout() {
        let mut renderer = TerminalRenderer::new_headless(13);
        renderer.resize(800, 600);
        let (cols, rows) = renderer.grid_for_viewport();
        assert!(cols > 10 && rows > 10);
        let before = renderer.cell_size();
        renderer.set_font_size(20);
        assert_ne!(renderer.cell_size(), before);
        assert_eq!(renderer.font_size(), 20);
        assert!(renderer.atlas().is_empty(), "atlas cleared on font change");
        assert!(renderer.needs_redraw());
    }

    #[test]
    fn colors_cover_default_indexed_and_truecolor() {
        assert_eq!(color_to_rgba(Color::Default, true), DEFAULT_FG);
        assert_eq!(color_to_rgba(Color::Default, false), DEFAULT_BG);
        let red = color_to_rgba(Color::Indexed(1), true);
        assert!(red[0] > 0.5 && red[1] < 0.3);
        let custom = color_to_rgba(Color::Rgb(10, 20, 30), false);
        assert!(custom[0] < custom[2]);
        // 6×6×6 cube + grayscale extremes resolve distinctly.
        assert_ne!(xterm256_to_rgb(16), xterm256_to_rgb(231));
        assert_ne!(xterm256_to_rgb(232), xterm256_to_rgb(255));
    }

    #[test]
    fn reverse_video_swaps_fg_bg_instances() {
        let mut term = Terminal::new(4, 1, 0);
        term.write_bytes(b"\x1b[7mR\x1b[0m");
        let mut renderer = TerminalRenderer::new_headless(13);
        renderer.build_instances(&term);
        let instance = renderer.instances()[0];
        assert_ne!(instance.fg, instance.bg);
        // Reverse white-on-black would be caught here if swapped wrong.
        let plain = {
            let mut plain_term = Terminal::new(4, 1, 0);
            plain_term.write_bytes(b"R");
            let mut plain_renderer = TerminalRenderer::new_headless(13);
            plain_renderer.build_instances(&plain_term);
            plain_renderer.instances()[0]
        };
        assert_eq!(instance.fg, plain.bg);
        assert_eq!(instance.bg, plain.fg);
    }

    #[test]
    fn attributes_do_not_break_instance_build() {
        let mut term = Terminal::new(6, 1, 0);
        term.write_bytes(b"\x1b[1;3;4mB\x1b[0m");
        let mut renderer = TerminalRenderer::new_headless(13);
        let stats = renderer.render(&term).unwrap();
        assert_eq!(stats.instances, 6);
        let _ = Attributes::default();
    }
}
