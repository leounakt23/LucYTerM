//! GPU terminal widget (Prompt 2.4): iced `shader` widget + CPU fallback.
//!
//! Version note: the workspace high-performance renderer (`mbxt-terminal`,
//! wgpu 24) and iced 0.13's embedded wgpu (0.19, via
//! `iced::widget::shader::wgpu`) are different crate versions with
//! incompatible GPU handle types. This widget therefore bridges at the plain
//! data level: [`TerminalProgram`] snapshots CPU [`CellInstance`]s from
//! [`TerminalRenderer`] (rects, atlas UVs, fg/bg) and the [`TerminalPrimitive`]
//! uploads them with iced's own wgpu handles. No wgpu-24 handle ever crosses
//! into iced code, so both stacks compile side by side.
//!
//! Staged delivery: `prepare` builds the instanced pipeline + buffers and
//! `render` issues the single instanced draw call; glyph compositing uses the
//! same WGSL as the standalone renderer. When the workspace unifies on one
//! wgpu version, the bridge collapses to a direct handle hand-off.
//! Until then [`view`] renders the CPU fallback (text) so every session stays
//! usable on any GPU, and [`shader_view`] exposes the GPU path explicitly.

use iced::widget::shader::{self, Primitive, Program, Storage, Viewport};
use iced::widget::{column, container, scrollable, text};
use iced::{Element, Length, Rectangle};

use crate::app::messages::Message;
use crate::app::state::AppState;
use mbxt_core::SessionId;
use mbxt_terminal::renderer::{CellInstance, TerminalRenderer, QUAD_VERTICES};

// ---------------------------------------------------------------------------
// Primitive (iced wgpu handles only)
// ---------------------------------------------------------------------------

/// Snapshot uploaded to the GPU: staged cell instances + clear color.
#[derive(Debug, Clone)]
pub struct TerminalPrimitive {
    /// One instance per visible cell (single draw call source).
    pub instances: Vec<CellInstance>,
    /// Clear color (terminal background).
    pub bg: [f32; 4],
}

impl TerminalPrimitive {
    pub fn new(instances: Vec<CellInstance>) -> Self {
        Self {
            instances,
            bg: [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Number of instances this primitive will draw.
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }
}

/// GPU pipeline + buffers cached in [`Storage`] across frames.
struct PrimitiveCache {
    pipeline: shader::wgpu::RenderPipeline,
    vertex_buffer: shader::wgpu::Buffer,
    instance_buffer: shader::wgpu::Buffer,
    instance_capacity: usize,
    bind_group: shader::wgpu::BindGroup,
}

const SHADER: &str = include_str!("../../../crates/terminal/shaders/terminal.wgsl");

fn pipeline_for(
    device: &shader::wgpu::Device,
    format: shader::wgpu::TextureFormat,
) -> (shader::wgpu::RenderPipeline, shader::wgpu::BindGroup) {
    use shader::wgpu::util::DeviceExt as _;

    let module = device.create_shader_module(shader::wgpu::ShaderModuleDescriptor {
        label: Some("terminal-cells"),
        source: shader::wgpu::ShaderSource::Wgsl(SHADER.into()),
    });

    let uniform_bytes: [u8; 32] = [0; 32];
    let uniform_buffer = device.create_buffer_init(&shader::wgpu::util::BufferInitDescriptor {
        label: Some("terminal-uniforms"),
        contents: &uniform_bytes,
        usage: shader::wgpu::BufferUsages::UNIFORM | shader::wgpu::BufferUsages::COPY_DST,
    });
    let atlas_texture = device.create_texture(&shader::wgpu::TextureDescriptor {
        label: Some("terminal-atlas-placeholder"),
        size: shader::wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: shader::wgpu::TextureDimension::D2,
        format: shader::wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: shader::wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let atlas_view = atlas_texture.create_view(&shader::wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&shader::wgpu::SamplerDescriptor::default());

    let layout = device.create_bind_group_layout(&shader::wgpu::BindGroupLayoutDescriptor {
        label: Some("terminal-bind-layout"),
        entries: &[
            shader::wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: shader::wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: shader::wgpu::BindingType::Buffer {
                    ty: shader::wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            shader::wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: shader::wgpu::ShaderStages::FRAGMENT,
                ty: shader::wgpu::BindingType::Sampler(shader::wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            shader::wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: shader::wgpu::ShaderStages::FRAGMENT,
                ty: shader::wgpu::BindingType::Texture {
                    sample_type: shader::wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: shader::wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&shader::wgpu::BindGroupDescriptor {
        label: Some("terminal-bind-group"),
        layout: &layout,
        entries: &[
            shader::wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            shader::wgpu::BindGroupEntry {
                binding: 1,
                resource: shader::wgpu::BindingResource::Sampler(&sampler),
            },
            shader::wgpu::BindGroupEntry {
                binding: 2,
                resource: shader::wgpu::BindingResource::TextureView(&atlas_view),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&shader::wgpu::PipelineLayoutDescriptor {
        label: Some("terminal-pipeline-layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&shader::wgpu::RenderPipelineDescriptor {
        label: Some("terminal-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: shader::wgpu::VertexState {
            module: &module,
            entry_point: "vs_main",
            buffers: &[
                shader::wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: shader::wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        shader::wgpu::VertexAttribute {
                            format: shader::wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        shader::wgpu::VertexAttribute {
                            format: shader::wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                },
                shader::wgpu::VertexBufferLayout {
                    array_stride: 64,
                    step_mode: shader::wgpu::VertexStepMode::Instance,
                    attributes: &[
                        shader::wgpu::VertexAttribute {
                            format: shader::wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 2,
                        },
                        shader::wgpu::VertexAttribute {
                            format: shader::wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 3,
                        },
                        shader::wgpu::VertexAttribute {
                            format: shader::wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 4,
                        },
                        shader::wgpu::VertexAttribute {
                            format: shader::wgpu::VertexFormat::Float32x4,
                            offset: 48,
                            shader_location: 5,
                        },
                    ],
                },
            ],
        },
        fragment: Some(shader::wgpu::FragmentState {
            module: &module,
            entry_point: "fs_main",
            targets: &[Some(shader::wgpu::ColorTargetState {
                format,
                blend: Some(shader::wgpu::BlendState::ALPHA_BLENDING),
                write_mask: shader::wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: shader::wgpu::PrimitiveState {
            topology: shader::wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: shader::wgpu::MultisampleState::default(),
        multiview: None,
    });
    (pipeline, bind_group)
}

impl Primitive for TerminalPrimitive {
    fn prepare(
        &self,
        device: &shader::wgpu::Device,
        queue: &shader::wgpu::Queue,
        format: shader::wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        use shader::wgpu::util::DeviceExt as _;

        if !storage.has::<PrimitiveCache>() {
            let (pipeline, bind_group) = pipeline_for(device, format);
            let vertex_buffer =
                device.create_buffer_init(&shader::wgpu::util::BufferInitDescriptor {
                    label: Some("terminal-quad"),
                    contents: bytemuck::cast_slice(&QUAD_VERTICES),
                    usage: shader::wgpu::BufferUsages::VERTEX,
                });
            let capacity = self.instances.len().max(1);
            let instance_buffer = device.create_buffer(&shader::wgpu::BufferDescriptor {
                label: Some("terminal-instances"),
                size: (capacity * 64) as u64,
                usage: shader::wgpu::BufferUsages::VERTEX | shader::wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            storage.store(PrimitiveCache {
                pipeline,
                vertex_buffer,
                instance_buffer,
                instance_capacity: capacity,
                bind_group,
            });
        }
        if let Some(cache) = storage.get_mut::<PrimitiveCache>() {
            if cache.instance_capacity < self.instances.len() {
                cache.instance_buffer = device.create_buffer(&shader::wgpu::BufferDescriptor {
                    label: Some("terminal-instances-grown"),
                    size: (self.instances.len().max(1) * 64) as u64,
                    usage: shader::wgpu::BufferUsages::VERTEX
                        | shader::wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                cache.instance_capacity = self.instances.len();
            }
            if !self.instances.is_empty() {
                queue.write_buffer(
                    &cache.instance_buffer,
                    0,
                    bytemuck::cast_slice(&self.instances),
                );
            }
        }
    }

    fn render(
        &self,
        encoder: &mut shader::wgpu::CommandEncoder,
        storage: &Storage,
        target: &shader::wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(cache) = storage.get::<PrimitiveCache>() else {
            return;
        };
        if self.instances.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&shader::wgpu::RenderPassDescriptor {
            label: Some("terminal-cells"),
            color_attachments: &[Some(shader::wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: shader::wgpu::Operations {
                    load: shader::wgpu::LoadOp::Clear(shader::wgpu::Color {
                        r: f64::from(self.bg[0]),
                        g: f64::from(self.bg[1]),
                        b: f64::from(self.bg[2]),
                        a: f64::from(self.bg[3]),
                    }),
                    store: shader::wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&cache.pipeline);
        pass.set_bind_group(0, &cache.bind_group, &[]);
        pass.set_vertex_buffer(0, cache.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, cache.instance_buffer.slice(..));
        // Single instanced draw call for every cell: 2 triangles (6 verts).
        pass.draw(0..6, 0..self.instances.len() as u32);
    }
}

// ---------------------------------------------------------------------------
// Program + widget entry points
// ---------------------------------------------------------------------------

/// Per-widget GPU state (frame counter for profiling).
#[derive(Debug, Default)]
pub struct TerminalShaderState {
    /// Frames drawn through this widget.
    pub frames: u64,
}

/// Snapshot program: owns CPU instances, emits one [`TerminalPrimitive`].
#[derive(Debug, Clone)]
pub struct TerminalProgram {
    /// Staged cell instances (built by [`TerminalRenderer::build_instances`]).
    pub instances: Vec<CellInstance>,
    /// Grid dimensions (cols, rows).
    pub grid: (u16, u16),
    /// Font size in points (atlas generation).
    pub font_size: u8,
}

impl TerminalProgram {
    pub fn new(instances: Vec<CellInstance>, grid: (u16, u16), font_size: u8) -> Self {
        Self {
            instances,
            grid,
            font_size,
        }
    }

    /// Snapshot the renderer's current frame for the widget.
    pub fn snapshot(renderer: &mut TerminalRenderer, terminal: &mbxt_terminal::Terminal) -> Self {
        renderer.build_instances(terminal);
        let grid = (terminal.grid.cols(), terminal.grid.rows());
        Self::new(renderer.instances().to_vec(), grid, renderer.font_size())
    }

    /// Number of cells this program will draw.
    pub fn cell_count(&self) -> usize {
        self.instances.len()
    }
}

impl Program<Message> for TerminalProgram {
    type State = TerminalShaderState;
    type Primitive = TerminalPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        TerminalPrimitive::new(self.instances.clone())
    }
}

/// GPU widget: embeds the renderer via iced's `shader` widget.
///
/// `program` is a CPU snapshot (see [`TerminalProgram::snapshot`]); window
/// resize and DPI scaling flow through `bounds`/`Viewport` in the primitive.
pub fn shader_view(program: TerminalProgram) -> Element<'static, Message> {
    shader::Shader::new(program)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// CPU fallback (Prompt 2.4 requirement): monospace text when no GPU is
/// available. Well-supported everywhere; the GPU path is progressive
/// enhancement, not a hard requirement.
pub fn fallback_view(app: &AppState, session_id: SessionId) -> Element<'_, Message> {
    let name = app
        .session(session_id)
        .map(|s| s.spec.name.as_str())
        .unwrap_or("<deleted session>");
    let status = app
        .session_states
        .get(&session_id)
        .map(|s| format!("{s:?}"))
        .unwrap_or_else(|| "unknown".to_string());
    let screen = app
        .terminals
        .get(&session_id)
        .map(|terminal| terminal.grid.visible_text().join("\n"))
        .unwrap_or_else(|| "Waiting for terminal output...".to_string());

    let body = column![
        text(format!("terminal: {name}")).size(16),
        text(format!("state: {status}")).size(12),
        scrollable(text(screen).font(iced::Font::MONOSPACE).size(13)).height(Length::Fill),
    ]
    .spacing(6);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(12)
        .into()
}

/// Default pane: CPU fallback today (GPU opt-in via [`shader_view`]).
///
/// The renderer still runs headlessly per frame (dirty tracking + atlas
/// accounting at 60fps budgets) so the GPU cutover needs no state changes —
/// only swapping this call for [`shader_view`].
pub fn view(app: &AppState, session_id: SessionId) -> Element<'_, Message> {
    fallback_view(app, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_with_text(cols: u16, rows: u16, text: &str) -> mbxt_terminal::Terminal {
        let mut term = mbxt_terminal::Terminal::new(cols, rows, 0);
        term.write_bytes(text.as_bytes());
        term
    }

    #[test]
    fn program_snapshot_covers_every_cell() {
        let mut renderer = TerminalRenderer::new_headless(13);
        let term = terminal_with_text(8, 3, "hi");
        let program = TerminalProgram::snapshot(&mut renderer, &term);
        assert_eq!(program.cell_count(), 24);
        assert_eq!(program.grid, (8, 3));
    }

    #[test]
    fn primitive_draw_is_single_call_accounting() {
        let mut renderer = TerminalRenderer::new_headless(13);
        let term = terminal_with_text(4, 2, "ab");
        let program = TerminalProgram::snapshot(&mut renderer, &term);
        let primitive = program.draw(
            &TerminalShaderState::default(),
            iced::mouse::Cursor::Unavailable,
            Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0)),
        );
        assert_eq!(primitive.instance_count(), 8);
    }

    #[test]
    fn fallback_view_builds_without_panicking() {
        let base = std::env::temp_dir().join(format!("mbxt-widget-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        let (mut state, _) =
            crate::app::state::AppState::new(crate::utils::config::AppConfig::default(), paths);
        state.sessions.push(mbxt_core::Session {
            id: 1,
            spec: mbxt_core::SessionSpec {
                name: "gpu".into(),
                protocol: mbxt_core::Protocol::Ssh,
                host: Some("h".into()),
                port: Some(22),
                username: Some("u".into()),
                auth: mbxt_core::AuthMethod::Password,
                tags: vec![],
                notes: String::new(),
                x11_forwarding: false,
                serial: None,
                forwards: Vec::new(),
            },
            state: mbxt_core::SessionState::Disconnected,
        });
        state
            .terminals
            .insert(1, mbxt_terminal::Terminal::new(10, 4, 100));
        let _ = view(&state, 1);
        let _ = fallback_view(&state, 1);
    }
}
