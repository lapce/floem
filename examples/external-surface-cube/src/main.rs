use std::{
    sync::{Arc, Mutex, mpsc::Receiver},
    thread,
    time::{Duration, Instant},
};

use bytemuck::{Pod, Zeroable};
use floem::{
    Application, ExternalSurface, ExternalSurfacePaintOptions, GpuResources, SubductionWgpuSurface,
    action::inspect,
    external_surface::{SubductionFrameTick, SubductionFrameTicker},
    kurbo::{Circle, Point, Rect, Size, Stroke},
    peniko::Color,
    prelude::*,
    text::FontWeight,
    window::{WindowConfig, WindowId},
};
use wgpu::util::DeviceExt;

const CUBE_SIZE: u32 = 640;

fn app_view(window_id: WindowId) -> impl IntoView {
    let (surface, producer_surface) = ExternalSurface::new_subduction_wgpu(
        window_id,
        Size::new(f64::from(CUBE_SIZE), f64::from(CUBE_SIZE)),
    );
    let producer_surface = Arc::new(Mutex::new(Some(producer_surface)));
    let frame_ticker = Arc::new(Mutex::new(None::<SubductionFrameTicker>));
    let paint_surface = surface.clone();

    (
        "Subduction Cube".style(|s| {
            s.font_size(30.0)
                .font_weight(FontWeight::BOLD)
                .color(Color::from_rgb8(246, 241, 226))
        }),
        "Below: Floem paint. Middle: system-composited external surface. Above: Floem paint on a compositor-owned layer."
            .style(|s| s.font_size(14.0).color(Color::from_rgb8(155, 169, 177))),
        cube_canvas(paint_surface).style(|s| {
            s.width(780.0)
                .height(520.0)
                .margin_top(22.0)
                .border_radius(32.0)
                .border(1.0)
                .border_color(Color::from_rgba8(241, 219, 167, 55))
        }),
    )
        .style(|s| {
            s.width_full()
                .height_full()
                .flex_col()
                .items_center()
                .justify_center()
                .padding(36.0)
                .background(Color::from_rgb8(11, 15, 17))
        })
        .on_event_stop(
            listener::WindowGpuResourcesReady,
            move |_cx, gpu_resources| {
                let Some(producer_surface) = producer_surface.lock().unwrap().take() else {
                    return;
                };
                let Ok((ticker, rx)) = producer_surface.start_frame_ticker() else {
                    eprintln!("external-surface-cube: failed to start subduction frame ticker");
                    return;
                };
                *frame_ticker.lock().unwrap() = Some(ticker);
                start_cube_thread(producer_surface, gpu_resources.clone(), rx);
            },
        )
        .on_event_stop(listener::KeyUp, |_, KeyboardEvent { key, .. }| {
            if *key == Key::Named(NamedKey::F11) {
                inspect();
            }
        })
}

fn cube_canvas(surface: ExternalSurface) -> impl IntoView {
    let nums = [Some(5), None];
    for num in nums.iter().filter(|n| n.is_some()) {
        dbg!(num);
    }
    canvas(move |cx, size| {
        let canvas = Rect::ZERO.with_size(size);
        let cube_rect = centered_rect(size, Size::new(440.0, 330.0));

        // Content below the external surface.
        cx.painter
            .fill(canvas.to_rounded_rect(32.0), Color::from_rgb8(19, 28, 30))
            .draw();
        cx.painter
            .fill(
                Rect::new(72.0, 76.0, size.width - 72.0, size.height - 76.0).to_rounded_rect(30.0),
                Color::from_rgb8(25, 47, 50),
            )
            .draw();
        cx.painter
            .stroke(
                cube_rect.inflate(28.0, 24.0).to_rounded_rect(28.0),
                &Stroke::new(2.0),
                Color::from_rgba8(104, 154, 148, 150),
            )
            .draw();

        for index in 0..6 {
            let x = cube_rect.x0 + 54.0 + f64::from(index) * 66.0;
            cx.painter
                .fill(
                    Circle::new(Point::new(x, cube_rect.y1 + 48.0), 6.0),
                    Color::from_rgba8(236, 196, 126, 145),
                )
                .draw();
        }

        let options = ExternalSurfacePaintOptions::default();
        cx.painter
            .sink_mut()
            .draw_external_surface(surface.id(), cube_rect, options);

        // Content above the external surface.
        cx.painter
            .fill(
                Rect::new(
                    cube_rect.x0 - 18.0,
                    cube_rect.y0 + 34.0,
                    cube_rect.x1 + 18.0,
                    cube_rect.y0 + 88.0,
                )
                .to_rounded_rect(20.0),
                Color::from_rgba8(247, 226, 164, 230),
            )
            .draw();
        cx.painter
            .fill(
                Rect::new(
                    cube_rect.x0 + 14.0,
                    cube_rect.y0 + 51.0,
                    cube_rect.x1 - 120.0,
                    cube_rect.y0 + 70.0,
                )
                .to_rounded_rect(9.0),
                Color::from_rgba8(35, 45, 42, 210),
            )
            .draw();
        cx.painter
            .fill(
                Rect::new(
                    cube_rect.x1 - 98.0,
                    cube_rect.y0 + 48.0,
                    cube_rect.x1 - 28.0,
                    cube_rect.y0 + 74.0,
                )
                .to_rounded_rect(13.0),
                Color::from_rgba8(18, 139, 128, 230),
            )
            .draw();
        cx.painter
            .stroke(
                cube_rect.inflate(14.0, 14.0).to_rounded_rect(26.0),
                &Stroke::new(4.0),
                Color::from_rgba8(250, 238, 205, 225),
            )
            .draw();
    })
}

fn centered_rect(container: Size, size: Size) -> Rect {
    let origin = Point::new(
        (container.width - size.width).max(0.0) * 0.5,
        (container.height - size.height).max(0.0) * 0.5 + 16.0,
    );
    Rect::from_origin_size(origin, size)
}

fn start_cube_thread(
    surface: SubductionWgpuSurface,
    gpu_resources: GpuResources,
    rx: Receiver<SubductionFrameTick>,
) {
    thread::spawn(move || {
        let mut renderer = match CubeRenderer::new(surface, gpu_resources, CUBE_SIZE, CUBE_SIZE) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("external-surface-cube: {err}");
                return;
            }
        };
        let mut diag = CubeRenderDiagnostics::new();
        let started = Instant::now();
        let mut surface_texture = match renderer.acquire_surface_texture(&mut diag) {
            Ok(surface_texture) => surface_texture,
            Err(err) => {
                eprintln!("external-surface-cube: {err}");
                return;
            }
        };

        while let Ok(tick) = rx.recv() {
            diag.record_recv(tick);
            let animation_time = tick.predicted_present.unwrap_or(tick.received_at);
            let seconds = animation_time
                .checked_duration_since(started)
                .unwrap_or(Duration::ZERO)
                .as_secs_f32();
            if let Err(err) = renderer.render(seconds, surface_texture, &mut diag) {
                eprintln!("external-surface-cube: {err}");
            }
            surface_texture = match renderer.acquire_surface_texture(&mut diag) {
                Ok(surface_texture) => surface_texture,
                Err(err) => {
                    eprintln!("external-surface-cube: {err}");
                    break;
                }
            };
            diag.maybe_report();
        }
    });
}

struct CubeRenderer {
    target: floem::external_surface::SubductionWgpuTarget,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl CubeRenderer {
    fn new(
        surface: SubductionWgpuSurface,
        gpu_resources: GpuResources,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let target = surface
            .create_target_with_gpu_resources(&gpu_resources, width, height)
            .map_err(|err| format!("failed to create subduction wgpu target: {err}"))?;
        let device = &target.device;
        let target_format = target.format();
        let depth = create_depth_texture(device, width, height);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube uniform buffer"),
            contents: bytemuck::bytes_of(&identity_matrix()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cube bind group layout"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cube bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube shader"),
            source: wgpu::ShaderSource::Wgsl(CUBE_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cube pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertex buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube index buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Ok(Self {
            target,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: INDICES.len() as u32,
            uniform_buffer,
            bind_group,
            depth,
            width,
            height,
        })
    }

    fn acquire_surface_texture(
        &mut self,
        diag: &mut CubeRenderDiagnostics,
    ) -> Result<wgpu::SurfaceTexture, String> {
        let acquire_start = Instant::now();
        let surface_texture = match self.target.surface.get_current_texture() {
            Ok(surface_texture) => surface_texture,
            Err(err) => {
                self.target
                    .surface
                    .configure(&self.target.device, &self.target.config);
                return Err(format!("failed to acquire cube surface texture: {err}"));
            }
        };
        diag.record_acquire(acquire_start.elapsed());
        Ok(surface_texture)
    }

    fn render(
        &mut self,
        seconds: f32,
        surface_texture: wgpu::SurfaceTexture,
        diag: &mut CubeRenderDiagnostics,
    ) -> Result<(), String> {
        let frame_start = Instant::now();
        let aspect = self.width as f32 / self.height as f32;
        let model = mul(rotation_y(seconds * 0.9), rotation_x(seconds * 0.55));
        let view = translation(0.0, 0.0, -4.8);
        let projection = perspective(45_f32.to_radians(), aspect, 0.1, 100.0);
        let mvp = mul(projection, mul(view, model));
        self.target
            .queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));

        let target_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.target
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cube encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cube render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        let submit_start = Instant::now();
        self.target.queue.submit(Some(encoder.finish()));
        diag.record_submit(submit_start.elapsed());
        let present_start = Instant::now();
        surface_texture.present();
        diag.record_present(present_start.elapsed(), frame_start.elapsed());
        Ok(())
    }
}

#[derive(Debug)]
struct CubeRenderDiagnostics {
    enabled: bool,
    last_report: Instant,
    last_recv: Option<Instant>,
    last_frame_index: Option<u64>,
    recv: u64,
    dropped_ticks: u64,
    max_recv_gap: Duration,
    max_tick_to_recv: Duration,
    max_acquire: Duration,
    max_submit: Duration,
    max_present: Duration,
    max_frame: Duration,
}

impl CubeRenderDiagnostics {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("FLOEM_CUBE_DIAG").is_some(),
            last_report: Instant::now(),
            last_recv: None,
            last_frame_index: None,
            recv: 0,
            dropped_ticks: 0,
            max_recv_gap: Duration::ZERO,
            max_tick_to_recv: Duration::ZERO,
            max_acquire: Duration::ZERO,
            max_submit: Duration::ZERO,
            max_present: Duration::ZERO,
            max_frame: Duration::ZERO,
        }
    }

    fn record_recv(&mut self, tick: SubductionFrameTick) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        if let Some(last_recv) = self.last_recv {
            self.max_recv_gap = self.max_recv_gap.max(now.duration_since(last_recv));
        }
        if let Some(last_frame_index) = self.last_frame_index {
            let gap = tick.frame_index.saturating_sub(last_frame_index);
            if gap > 1 {
                self.dropped_ticks = self.dropped_ticks.saturating_add(gap - 1);
            }
        }
        self.last_recv = Some(now);
        self.last_frame_index = Some(tick.frame_index);
        self.max_tick_to_recv = self
            .max_tick_to_recv
            .max(now.saturating_duration_since(tick.received_at));
        self.recv = self.recv.saturating_add(1);
    }

    fn record_acquire(&mut self, elapsed: Duration) {
        if self.enabled {
            self.max_acquire = self.max_acquire.max(elapsed);
        }
    }

    fn record_submit(&mut self, elapsed: Duration) {
        if self.enabled {
            self.max_submit = self.max_submit.max(elapsed);
        }
    }

    fn record_present(&mut self, present: Duration, frame: Duration) {
        if self.enabled {
            self.max_present = self.max_present.max(present);
            self.max_frame = self.max_frame.max(frame);
        }
    }

    fn maybe_report(&mut self) {
        if !self.enabled || self.last_report.elapsed() < Duration::from_secs(1) {
            return;
        }
        eprintln!(
            "cube render: recv={} dropped_ticks={} max_recv_gap={:.2}ms max_tick_to_recv={:.2}ms max_acquire={:.2}ms max_submit={:.2}ms max_present_call={:.2}ms max_frame={:.2}ms",
            self.recv,
            self.dropped_ticks,
            self.max_recv_gap.as_secs_f64() * 1000.0,
            self.max_tick_to_recv.as_secs_f64() * 1000.0,
            self.max_acquire.as_secs_f64() * 1000.0,
            self.max_submit.as_secs_f64() * 1000.0,
            self.max_present.as_secs_f64() * 1000.0,
            self.max_frame.as_secs_f64() * 1000.0,
        );
        *self = Self {
            enabled: true,
            last_report: Instant::now(),
            ..Self::new()
        };
    }
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("cube depth texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0, 1.0],
        color: [0.95, 0.32, 0.22],
    },
    Vertex {
        position: [1.0, -1.0, 1.0],
        color: [1.0, 0.72, 0.25],
    },
    Vertex {
        position: [1.0, 1.0, 1.0],
        color: [0.25, 0.75, 1.0],
    },
    Vertex {
        position: [-1.0, 1.0, 1.0],
        color: [0.37, 0.95, 0.62],
    },
    Vertex {
        position: [-1.0, -1.0, -1.0],
        color: [0.92, 0.27, 0.65],
    },
    Vertex {
        position: [1.0, -1.0, -1.0],
        color: [0.37, 0.45, 1.0],
    },
    Vertex {
        position: [1.0, 1.0, -1.0],
        color: [0.70, 0.86, 0.26],
    },
    Vertex {
        position: [-1.0, 1.0, -1.0],
        color: [0.95, 0.95, 0.98],
    },
];

const INDICES: &[u16] = &[
    0, 1, 2, 0, 2, 3, 1, 5, 6, 1, 6, 2, 5, 4, 7, 5, 7, 6, 4, 0, 3, 4, 3, 7, 3, 2, 6, 3, 6, 7, 4, 5,
    1, 4, 1, 0,
];

const CUBE_SHADER: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = uniforms.mvp * vec4<f32>(vertex.position, 1.0);
    out.color = vertex.color;
    return out;
}

@fragment
fn fs_main(vertex: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(vertex.color, 1.0);
}
"#;

type Mat4 = [[f32; 4]; 4];

fn identity_matrix() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn translation(x: f32, y: f32, z: f32) -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

fn rotation_x(radians: f32) -> Mat4 {
    let (sin, cos) = radians.sin_cos();
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, cos, sin, 0.0],
        [0.0, -sin, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn rotation_y(radians: f32) -> Mat4 {
    let (sin, cos) = radians.sin_cos();
    [
        [cos, 0.0, -sin, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [sin, 0.0, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, (near * far) / (near - far), 0.0],
    ]
}

fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = a[0][row] * b[col][0]
                + a[1][row] * b[col][1]
                + a[2][row] * b[col][2]
                + a[3][row] * b[col][3];
        }
    }
    out
}

fn main() {
    Application::new()
        .window(
            app_view,
            Some(
                WindowConfig::default()
                    .size(Size::new(900.0, 660.0))
                    .title("External Surface Cube"),
            ),
        )
        .run();
}
