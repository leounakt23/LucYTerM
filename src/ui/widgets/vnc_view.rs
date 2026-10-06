//! VNC viewer widget (Prompt 4.3): wgpu texture display + toolbar.
//!
//! Frames stream from [`VncManager`] snapshots into an iced `shader`
//! primitive (same wgpu-0.19 bridge as the terminal widget — no wgpu-24
//! handle crosses into iced). Pointer/keyboard events map to RFB input in
//! [`Program::update`]; the toolbar covers scaling, fullscreen, CAD,
//! clipboard sync, and disconnect.
//!
//! Performance: one texture upload per generation change, one draw call per
//! frame, repaints driven by the 15 fps frame-tick subscription (only while
//! a viewer tab is visible).

use std::sync::Arc;

use iced::widget::shader::{self, Primitive, Program, Storage, Viewport};
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Rectangle};

use crate::app::messages::{Message, VncMsg};
use crate::app::state::AppState;
use crate::connection::vnc::input::{button as vnc_button, keysym, ScalingMode};
use crate::connection::vnc::{FrameSnapshot, VncManager};

// ---------------------------------------------------------------------------
// Primitive (iced wgpu handles only)
// ---------------------------------------------------------------------------

/// Snapshot handed to the GPU: one shared frame, uploaded on generation
/// change (single draw call).
#[derive(Debug, Clone)]
pub struct VncPrimitive {
    pub frame: Option<Arc<FrameSnapshot>>,
}

impl VncPrimitive {
    pub fn empty() -> Self {
        Self { frame: None }
    }
}

/// Cached GPU objects for one widget instance.
struct PrimitiveCache {
    pipeline: shader::wgpu::RenderPipeline,
    bind_group: shader::wgpu::BindGroup,
    texture: shader::wgpu::Texture,
    view: shader::wgpu::TextureView,
    sampler: shader::wgpu::Sampler,
    size: (u32, u32),
    generation: u64,
}

const SHADER: &str = r#"
@group(0) @binding(0) var frame_sampler: sampler;
@group(0) @binding(1) var frame_texture: texture_2d<f32>;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Out {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: Out;
    out.position = vec4<f32>(corners[index], 0.0, 1.0);
    out.uv = vec2<f32>((corners[index].x + 1.0) * 0.5, (1.0 - corners[index].y) * 0.5);
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(frame_texture, frame_sampler, in.uv);
}
"#;

fn pipeline_for(
    device: &shader::wgpu::Device,
    format: shader::wgpu::TextureFormat,
) -> (shader::wgpu::RenderPipeline, shader::wgpu::BindGroupLayout) {
    let module = device.create_shader_module(shader::wgpu::ShaderModuleDescriptor {
        label: Some("vnc-frame"),
        source: shader::wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_bind_group_layout(&shader::wgpu::BindGroupLayoutDescriptor {
        label: Some("vnc-bind-layout"),
        entries: &[
            shader::wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: shader::wgpu::ShaderStages::FRAGMENT,
                ty: shader::wgpu::BindingType::Sampler(shader::wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            shader::wgpu::BindGroupLayoutEntry {
                binding: 1,
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
    let pipeline_layout = device.create_pipeline_layout(&shader::wgpu::PipelineLayoutDescriptor {
        label: Some("vnc-pipeline-layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&shader::wgpu::RenderPipelineDescriptor {
        label: Some("vnc-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: shader::wgpu::VertexState {
            module: &module,
            entry_point: "vs",
            buffers: &[],
        },
        fragment: Some(shader::wgpu::FragmentState {
            module: &module,
            entry_point: "fs",
            targets: &[Some(shader::wgpu::ColorTargetState {
                format,
                blend: Some(shader::wgpu::BlendState::REPLACE),
                write_mask: shader::wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: shader::wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: shader::wgpu::MultisampleState::default(),
        multiview: None,
    });
    (pipeline, layout)
}

impl Primitive for VncPrimitive {
    fn prepare(
        &self,
        device: &shader::wgpu::Device,
        queue: &shader::wgpu::Queue,
        format: shader::wgpu::TextureFormat,
        storage: &mut Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        if frame.width == 0 || frame.height == 0 || frame.rgba.is_empty() {
            return;
        }
        let stale = storage
            .get::<PrimitiveCache>()
            .map(|cache| {
                cache.generation != frame.generation
                    || cache.size != (u32::from(frame.width), u32::from(frame.height))
            })
            .unwrap_or(true);
        if stale {
            let (pipeline, layout) = pipeline_for(device, format);
            let texture = device.create_texture(&shader::wgpu::TextureDescriptor {
                label: Some("vnc-frame"),
                size: shader::wgpu::Extent3d {
                    width: u32::from(frame.width),
                    height: u32::from(frame.height),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: shader::wgpu::TextureDimension::D2,
                format: shader::wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: shader::wgpu::TextureUsages::TEXTURE_BINDING
                    | shader::wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&shader::wgpu::TextureViewDescriptor::default());
            let sampler = device.create_sampler(&shader::wgpu::SamplerDescriptor::default());
            let bind_group = device.create_bind_group(&shader::wgpu::BindGroupDescriptor {
                label: Some("vnc-bind-group"),
                layout: &layout,
                entries: &[
                    shader::wgpu::BindGroupEntry {
                        binding: 0,
                        resource: shader::wgpu::BindingResource::Sampler(&sampler),
                    },
                    shader::wgpu::BindGroupEntry {
                        binding: 1,
                        resource: shader::wgpu::BindingResource::TextureView(&view),
                    },
                ],
            });
            storage.store(PrimitiveCache {
                pipeline,
                bind_group,
                texture,
                view,
                sampler,
                size: (u32::from(frame.width), u32::from(frame.height)),
                generation: frame.generation,
            });
        }
        if let Some(cache) = storage.get::<PrimitiveCache>() {
            queue.write_texture(
                shader::wgpu::ImageCopyTexture {
                    texture: &cache.texture,
                    mip_level: 0,
                    origin: shader::wgpu::Origin3d::ZERO,
                    aspect: shader::wgpu::TextureAspect::All,
                },
                &frame.rgba,
                shader::wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * u32::from(frame.width)),
                    rows_per_image: Some(u32::from(frame.height)),
                },
                shader::wgpu::Extent3d {
                    width: u32::from(frame.width),
                    height: u32::from(frame.height),
                    depth_or_array_layers: 1,
                },
            );
            let _ = (&cache.view, &cache.sampler);
        }
    }

    fn render(
        &self,
        encoder: &mut shader::wgpu::CommandEncoder,
        storage: &Storage,
        target: &shader::wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if self.frame.is_none() {
            return;
        }
        let Some(cache) = storage.get::<PrimitiveCache>() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&shader::wgpu::RenderPassDescriptor {
            label: Some("vnc-frame"),
            color_attachments: &[Some(shader::wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: shader::wgpu::Operations {
                    load: shader::wgpu::LoadOp::Clear(shader::wgpu::Color::BLACK),
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
        // Single draw call: fullscreen triangle.
        pass.draw(0..3, 0..1);
    }
}

// ---------------------------------------------------------------------------
// Program (input mapping + snapshot)
// ---------------------------------------------------------------------------

/// Per-widget input state (button mask survives across motion events).
#[derive(Debug, Default)]
pub struct VncShaderState {
    pub buttons: u8,
}

/// Viewer UI state (ephemeral; lives in `AppState::vnc_viewers`).
#[derive(Debug, Clone)]
pub struct VncViewerUi {
    pub scaling: ScalingMode,
    pub fullscreen: bool,
    pub connected: bool,
    pub error: Option<String>,
}

impl Default for VncViewerUi {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::Fit,
            fullscreen: false,
            connected: false,
            error: None,
        }
    }
}

/// Snapshot program: owns the latest frame, emits RFB input messages.
#[derive(Debug, Clone)]
pub struct VncProgram {
    pub session: mbxt_core::SessionId,
    pub frame: Option<Arc<FrameSnapshot>>,
    pub remote: (u16, u16),
    pub scaling: ScalingMode,
}

impl VncProgram {
    /// Snapshot the manager's latest frame for `session`.
    pub fn snapshot(
        session: mbxt_core::SessionId,
        frame: Option<Arc<FrameSnapshot>>,
        remote: (u16, u16),
        scaling: ScalingMode,
    ) -> Self {
        Self {
            session,
            frame,
            remote,
            scaling,
        }
    }
}

impl Program<Message> for VncProgram {
    type State = VncShaderState;
    type Primitive = VncPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        VncPrimitive {
            frame: self.frame.clone(),
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: shader::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
        _shell: &mut iced::advanced::Shell<'_, Message>,
    ) -> (iced::event::Status, Option<Message>) {
        use iced::event::Status;
        match event {
            shader::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return (Status::Ignored, None);
                };
                let (x, y) = crate::connection::vnc::scale_point(
                    (position.x, position.y),
                    (bounds.width, bounds.height),
                    self.remote,
                    self.scaling,
                );
                (
                    Status::Captured,
                    Some(Message::Vnc(VncMsg::PointerMoved(
                        self.session,
                        state.buttons,
                        (x, y),
                    ))),
                )
            },
            shader::Event::Mouse(iced::mouse::Event::ButtonPressed(button)) => {
                if let Some(bit) = mouse_button_bit(button) {
                    state.buttons |= bit;
                    let position = cursor
                        .position_in(bounds)
                        .map(|p| {
                            crate::connection::vnc::scale_point(
                                (p.x, p.y),
                                (bounds.width, bounds.height),
                                self.remote,
                                self.scaling,
                            )
                        })
                        .unwrap_or((0, 0));
                    (
                        Status::Captured,
                        Some(Message::Vnc(VncMsg::PointerButton(
                            self.session,
                            state.buttons,
                            position,
                        ))),
                    )
                } else {
                    (Status::Ignored, None)
                }
            },
            shader::Event::Mouse(iced::mouse::Event::ButtonReleased(button)) => {
                if let Some(bit) = mouse_button_bit(button) {
                    state.buttons &= !bit;
                    let position = cursor
                        .position_in(bounds)
                        .map(|p| {
                            crate::connection::vnc::scale_point(
                                (p.x, p.y),
                                (bounds.width, bounds.height),
                                self.remote,
                                self.scaling,
                            )
                        })
                        .unwrap_or((0, 0));
                    (
                        Status::Captured,
                        Some(Message::Vnc(VncMsg::PointerButton(
                            self.session,
                            state.buttons,
                            position,
                        ))),
                    )
                } else {
                    (Status::Ignored, None)
                }
            },
            shader::Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                let up = match delta {
                    iced::mouse::ScrollDelta::Lines { y, .. }
                    | iced::mouse::ScrollDelta::Pixels { y, .. } => y > 0.0,
                };
                let position = cursor
                    .position_in(bounds)
                    .map(|p| {
                        crate::connection::vnc::scale_point(
                            (p.x, p.y),
                            (bounds.width, bounds.height),
                            self.remote,
                            self.scaling,
                        )
                    })
                    .unwrap_or((0, 0));
                (
                    Status::Captured,
                    Some(Message::Vnc(VncMsg::PointerWheel(
                        self.session,
                        position,
                        up,
                    ))),
                )
            },
            shader::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, text, .. }) => {
                if let Some(message) = key_pressed_to_message(self.session, &key, text.as_deref()) {
                    (Status::Captured, Some(message))
                } else {
                    (Status::Ignored, None)
                }
            },
            shader::Event::Keyboard(iced::keyboard::Event::KeyReleased { key, .. }) => {
                if let Some(keysym) = named_keysym(&key) {
                    (
                        Status::Captured,
                        Some(Message::Vnc(VncMsg::KeyEvent(self.session, keysym, false))),
                    )
                } else {
                    (Status::Ignored, None)
                }
            },
            _ => (Status::Ignored, None),
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        iced::mouse::Interaction::Crosshair
    }
}

fn mouse_button_bit(button: iced::mouse::Button) -> Option<u8> {
    match button {
        iced::mouse::Button::Left => Some(vnc_button::LEFT),
        iced::mouse::Button::Middle => Some(vnc_button::MIDDLE),
        iced::mouse::Button::Right => Some(vnc_button::RIGHT),
        _ => None,
    }
}

fn named_keysym(key: &iced::keyboard::Key) -> Option<u32> {
    use iced::keyboard::key::Named;
    use iced::keyboard::Key;
    match key {
        Key::Named(Named::Backspace) => Some(keysym::BACKSPACE),
        Key::Named(Named::Tab) => Some(keysym::TAB),
        Key::Named(Named::Enter) => Some(keysym::RETURN),
        Key::Named(Named::Escape) => Some(keysym::ESCAPE),
        Key::Named(Named::Insert) => Some(keysym::INSERT),
        Key::Named(Named::Delete) => Some(keysym::DELETE),
        Key::Named(Named::Home) => Some(keysym::HOME),
        Key::Named(Named::End) => Some(keysym::END),
        Key::Named(Named::PageUp) => Some(keysym::PAGE_UP),
        Key::Named(Named::PageDown) => Some(keysym::PAGE_DOWN),
        Key::Named(Named::ArrowLeft) => Some(keysym::LEFT),
        Key::Named(Named::ArrowUp) => Some(keysym::UP),
        Key::Named(Named::ArrowRight) => Some(keysym::RIGHT),
        Key::Named(Named::ArrowDown) => Some(keysym::DOWN),
        Key::Named(Named::Shift) => Some(keysym::SHIFT_LEFT),
        Key::Named(Named::Control) => Some(keysym::CONTROL_LEFT),
        Key::Named(Named::Alt) => Some(keysym::ALT_LEFT),
        Key::Named(Named::Super) => Some(keysym::SUPER_LEFT),
        Key::Named(Named::Space) => Some(0x20),
        Key::Named(Named::F1) => crate::connection::vnc::input::keysym_f(1),
        Key::Named(Named::F2) => crate::connection::vnc::input::keysym_f(2),
        Key::Named(Named::F3) => crate::connection::vnc::input::keysym_f(3),
        Key::Named(Named::F4) => crate::connection::vnc::input::keysym_f(4),
        Key::Named(Named::F5) => crate::connection::vnc::input::keysym_f(5),
        Key::Named(Named::F6) => crate::connection::vnc::input::keysym_f(6),
        Key::Named(Named::F7) => crate::connection::vnc::input::keysym_f(7),
        Key::Named(Named::F8) => crate::connection::vnc::input::keysym_f(8),
        Key::Named(Named::F9) => crate::connection::vnc::input::keysym_f(9),
        Key::Named(Named::F10) => crate::connection::vnc::input::keysym_f(10),
        Key::Named(Named::F11) => crate::connection::vnc::input::keysym_f(11),
        Key::Named(Named::F12) => crate::connection::vnc::input::keysym_f(12),
        _ => None,
    }
}

fn key_pressed_to_message(
    session: mbxt_core::SessionId,
    key: &iced::keyboard::Key,
    text: Option<&str>,
) -> Option<Message> {
    use iced::keyboard::Key;
    // Printable text wins (layout-correct); otherwise the named table.
    if let Some(text) = text {
        if let Some(c) = text.chars().next() {
            if let Some(keysym) = crate::connection::vnc::input::keysym_for_char(c) {
                return Some(Message::Vnc(VncMsg::KeyTap(session, keysym)));
            }
        }
    }
    match key {
        Key::Character(c) => {
            let c = c.as_str().chars().next()?;
            let keysym = crate::connection::vnc::input::keysym_for_char(c)?;
            Some(Message::Vnc(VncMsg::KeyTap(session, keysym)))
        },
        named => {
            let keysym = named_keysym(named)?;
            Some(Message::Vnc(VncMsg::KeyEvent(session, keysym, true)))
        },
    }
}

// ---------------------------------------------------------------------------
// Pane view (display + toolbar)
// ---------------------------------------------------------------------------

/// VNC tab pane: toolbar + live display (or connecting/error/empty states).
pub fn view(app: &AppState, session: mbxt_core::SessionId) -> Element<'_, Message> {
    let viewer = app.vnc_viewers.get(&session);
    let mut body = column![toolbar(app, session)].spacing(6);

    match viewer {
        None => {
            body = body.push(
                column![
                    text("Remote desktop").size(16),
                    button(text("Connect viewer").size(12))
                        .on_press(Message::Vnc(VncMsg::OpenViewer(session)))
                        .padding([2, 8]),
                ]
                .spacing(6),
            );
        },
        Some(viewer) if !viewer.connected => {
            let status = viewer.error.as_deref().unwrap_or("connecting…");
            body = body.push(text(status).size(13));
        },
        Some(viewer) => {
            let scaling = viewer.scaling;
            let frame = VncManager::shared().frame_snapshot(session);
            let remote = VncManager::shared()
                .desktop_size(session)
                .unwrap_or((1024, 768));
            let program = VncProgram::snapshot(session, frame, remote, scaling);
            let display: Element<'_, Message> = match scaling {
                ScalingMode::Fit => shader::Shader::new(program)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
                ScalingMode::OneToOne => container(
                    shader::Shader::new(program)
                        .width(Length::Fixed(f32::from(remote.0)))
                        .height(Length::Fixed(f32::from(remote.1))),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            };
            body = body.push(display);
        },
    }

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(12)
        .into()
}

/// Toolbar: scaling, fullscreen, Ctrl+Alt+Del, clipboard sync, disconnect.
pub fn toolbar(app: &AppState, session: mbxt_core::SessionId) -> Element<'_, Message> {
    let scaling_label = match app
        .vnc_viewers
        .get(&session)
        .map(|viewer| viewer.scaling)
        .unwrap_or(ScalingMode::Fit)
    {
        ScalingMode::Fit => "Scale: fit",
        ScalingMode::OneToOne => "Scale: 1:1",
    };
    let fullscreen_label = match app
        .vnc_viewers
        .get(&session)
        .map(|viewer| viewer.fullscreen)
        .unwrap_or(false)
    {
        true => "Exit fullscreen",
        false => "Fullscreen",
    };
    let remote_cut = VncManager::shared()
        .last_cut_text(session)
        .map(|text| format!("remote clipboard: {text}"))
        .unwrap_or_else(|| "remote clipboard: —".to_string());

    row![
        button(text(scaling_label).size(11))
            .on_press(Message::Vnc(VncMsg::SetScaling(
                session,
                match app
                    .vnc_viewers
                    .get(&session)
                    .map(|viewer| viewer.scaling)
                    .unwrap_or(ScalingMode::Fit)
                {
                    ScalingMode::Fit => ScalingMode::OneToOne,
                    ScalingMode::OneToOne => ScalingMode::Fit,
                }
            )))
            .padding([2, 8]),
        button(text(fullscreen_label).size(11))
            .on_press(Message::Vnc(VncMsg::ToggleFullscreen(session)))
            .padding([2, 8]),
        button(text("Send Ctrl+Alt+Del").size(11))
            .on_press(Message::Vnc(VncMsg::SendCad(session)))
            .padding([2, 8]),
        button(text("Send clipboard").size(11))
            .on_press(Message::Vnc(VncMsg::SendClipboard(session)))
            .padding([2, 8]),
        button(text("Disconnect").size(11))
            .on_press(Message::Vnc(VncMsg::CloseViewer(session)))
            .padding([2, 8]),
        text(remote_cut).size(11).width(Length::Fill),
    ]
    .spacing(6)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AppState {
        let base = std::env::temp_dir().join(format!("mbxt-vnc-test-{}", std::process::id()));
        let paths = crate::utils::paths::AppPaths {
            config_dir: base.join("config"),
            logs_dir: base.join("logs"),
            data_dir: base.join("data"),
            runtime_dir: base.join("run"),
        };
        AppState::new(crate::utils::config::AppConfig::default(), paths).0
    }

    #[test]
    fn empty_and_toolbar_views_build() {
        let app = test_app();
        let _ = view(&app, 1);
        let _ = toolbar(&app, 1);
    }

    #[test]
    fn primitive_draw_wraps_snapshot() {
        let snapshot = Arc::new(FrameSnapshot {
            generation: 3,
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        });
        let program =
            VncProgram::snapshot(9, Some(Arc::clone(&snapshot)), (2, 2), ScalingMode::Fit);
        let primitive = program.draw(
            &VncShaderState::default(),
            iced::mouse::Cursor::Unavailable,
            Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0)),
        );
        assert_eq!(primitive.frame.as_ref().unwrap().generation, 3);
    }

    #[test]
    fn named_and_character_keys_map() {
        use iced::keyboard::{key::Named, Key};
        assert_eq!(
            named_keysym(&Key::Named(Named::Escape)),
            Some(keysym::ESCAPE)
        );
        assert_eq!(named_keysym(&Key::Named(Named::ArrowUp)), Some(keysym::UP));
        assert!(named_keysym(&Key::Named(Named::PrintScreen)).is_none());
        let message = key_pressed_to_message(5, &Key::Character("a".into()), Some("a")).unwrap();
        assert!(matches!(message, Message::Vnc(VncMsg::KeyTap(5, 0x61))));
        assert!(mouse_button_bit(iced::mouse::Button::Left).is_some());
        assert!(mouse_button_bit(iced::mouse::Button::Back).is_none());
    }
}
