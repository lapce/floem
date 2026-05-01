use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use bytemuck::{Pod, Zeroable};
use floem::{
    Application, ColorEffect, ColorEffectId, CompositorSurface, GpuResources,
    RenderableCompositorSurface, RenderableCompositorSurfaceConfig, ShaderEffectId, SourceEffect,
    action::{capture_metal, inspect},
    group_ref,
    imaging::{Brush, ClipRef, Filter, ImageBrush},
    kurbo::{Affine, Circle, Point, Rect, Size, Stroke},
    peniko::Color,
    prelude::*,
    style::Position,
    text::{Alignment, Attrs, AttrsList, FontWeight, TextLayout},
    window::{WindowConfig, WindowId},
};
use wgpu::util::DeviceExt;

const CUBE_SIZE: u32 = 640;

fn app_view(window_id: WindowId) -> impl IntoView {
    let (surface, producer_surface) = CompositorSurface::new_renderable(
        window_id,
        Size::new(f64::from(CUBE_SIZE), f64::from(CUBE_SIZE)),
        RenderableCompositorSurfaceConfig::default(),
    );
    let producer_surface = Arc::new(Mutex::new(Some(producer_surface)));
    let paint_surface = surface.clone();

    (
        "External Surface as Image Brush".style(|s| {
            s.font_size(30.0)
                .font_weight(FontWeight::BOLD)
                .color(Color::from_rgb8(246, 241, 226))
        }),
        "The cube is rendered by an external producer into a Subduction-owned wgpu texture. Floem can publish that frame as a compositor layer, or sample the same submitted frame as an imaging external image. This example uses the cube texture as an image brush for the large glyph fill, then flattens the surrounding clip, blur, and checkerboard color effect into one ordered render pass when direct layer promotion is not legal. Press F11 for the inspector or F12 to capture the next Metal frame."
            .style(|s| {
                s.font_size(14.0)
                    .line_height(1.35)
                    .text_wrap()
                    .text_align(Alignment::Center)
                    .max_width(760.0)
                    .color(Color::from_rgb8(155, 169, 177))
            }),
        cube_panel(paint_surface),
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
                if let Err(err) = start_cube_target(producer_surface, gpu_resources.clone()) {
                    eprintln!("compositor-surface-cube: failed to start renderable target: {err}");
                }
            },
        )
        .on_event_stop(listener::KeyUp, |_, KeyboardEvent { key, .. }| {
            match *key {
                Key::Named(NamedKey::F11) => inspect(),
                Key::Named(NamedKey::F12) => capture_metal(),
                _ => {}
            }
        })
}

fn cube_panel(surface: CompositorSurface) -> impl IntoView {
    cube_canvas(surface).style(|s| {
        s.width(780.0)
            .height(520.0)
            .margin_top(22.0)
            .position(Position::Relative)
            .border_radius(32.0)
            .border(1.0)
            .border_color(Color::from_rgba8(241, 219, 167, 55))
    })
}

fn cube_canvas(surface: CompositorSurface) -> impl IntoView {
    canvas(move |cx, size| {
        let canvas = Rect::ZERO.with_size(size);
        let cube_rect = centered_rect(size, Size::new(440.0, 330.0));
        cx.painter
            .fill(canvas.to_rounded_rect(32.0), Color::from_rgb8(19, 28, 30))
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

        let cube_image = surface.image(
            (cube_rect.width() * cx.effective_scale).ceil() as u32,
            (cube_rect.height() * cx.effective_scale).ceil() as u32,
        );

        let brush = Brush::Image(ImageBrush::from(cube_image));
        let label_rect = Rect::new(
            cube_rect.x0 - 18.0,
            cube_rect.y0 + 138.0,
            cube_rect.x1 + 18.0,
            cube_rect.y0 + 192.0,
        );
        let label_text_rect = Rect::new(
            label_rect.x0 + 32.0,
            label_rect.y0 + 6.0,
            label_rect.x1 - 132.0,
            label_rect.y1 - 6.0,
        );
        let source_panel = SourceEffect::wgsl(
            ShaderEffectId(2),
            r#"
let cell = floor(position / vec2<f32>(22.0, 22.0));
let checker = (cell.x + cell.y) - 2.0 * floor((cell.x + cell.y) * 0.5);
let stripe = 0.5 + 0.5 * sin((position.x + position.y) * 0.035);
let base = mix(vec3<f32>(0.08, 0.20, 0.22), vec3<f32>(0.13, 0.34, 0.36), checker);
return vec4<f32>(base + stripe * 0.035, 0.58);
"#,
        )
        .with_label("cube source shader panel")
        .with_color_space(subduction::wgpu::SurfaceColorSpace::ExtendedLinearSrgb);
        cx.painter.sink_mut().source_effect_rect(
            Rect::new(
                cube_rect.x0 - 92.0,
                cube_rect.y0 + 58.0,
                cube_rect.x1 + 92.0,
                cube_rect.y1 - 52.0,
            ),
            source_panel,
        );

        let checkerboard_effect = ColorEffect::wgsl(
            ColorEffectId(1),
            r#"
let sampled = textureSample(input_texture, input_sampler, uv);
let cell = floor(position / vec2<f32>(28.0, 28.0));
let checker = (cell.x + cell.y) - 2.0 * floor((cell.x + cell.y) * 0.5);
let tint_a = vec3<f32>(1.12, 0.94, 0.72);
let tint_b = vec3<f32>(0.72, 1.03, 1.12);
let tint = mix(tint_a, tint_b, checker);
return vec4<f32>(sampled.rgb * tint, sampled.a);
"#,
        )
        .with_label("cube checkerboard color effect")
        .with_color_space(subduction::wgpu::SurfaceColorSpace::ExtendedLinearSrgb);
        let filters = [checkerboard_effect.into(), Filter::blur(5.).into()];
        cx.painter.with_group(
            group_ref().with_filters(&filters).with_clip(ClipRef::Fill {
                transform: Affine::IDENTITY,
                shape: floem::imaging::GeometryRef::RoundedRect(
                    Rect::new(72.0, 76.0, size.width - 72.0, size.height - 76.0)
                        .to_rounded_rect(30.0),
                ),
                fill_rule: floem::peniko::Fill::NonZero,
            }),
            |p| {
                p.fill(
                    Rect::new(72.0, 76.0, size.width - 72.0, size.height - 76.0)
                        .to_rounded_rect(30.0),
                    Color::from_rgb8(25, 47, 50),
                )
                .draw();

                p.fill(
                    label_rect.to_rounded_rect(20.0),
                    Color::from_rgba8(247, 226, 164, 230),
                )
                .draw();
            },
        );

        let mut text = TextLayout::new_with_text(
            "TEXTURE BRUSH",
            AttrsList::new(Attrs::new().font_size(34.0).weight(FontWeight::BOLD)),
            Some(Alignment::Center),
        );
        text.set_size(
            label_text_rect.width() as f32,
            label_text_rect.height() as f32,
        );
        let text_size = text.size();
        let origin = Point::new(
            label_text_rect.x0,
            label_text_rect.y0 + ((label_text_rect.height() - text_size.height) * 0.5).max(0.0),
        );
        text.draw_with_painter_brush(
            cx.painter.as_imaging_dyn(),
            origin,
            floem::kurbo::Vec2::ZERO,
            cx.effective_scale,
            &brush,
            Some(Affine::translate((
                cube_rect.x0 - origin.x,
                cube_rect.y0 - origin.y,
            ))),
        );

        // cx.painter
        //     .fill(
        //         Rect::new(
        //             label_rect.x0 + 32.0,
        //             label_rect.y0 + 17.0,
        //             label_rect.x1 - 102.0,
        //             label_rect.y0 + 36.0,
        //         )
        //         .to_rounded_rect(9.0),
        //         Color::from_rgba8(35, 45, 42, 210),
        //     )
        //     .draw();
        // cx.painter
        //     .fill(
        //         Rect::new(
        //             label_rect.x1 - 116.0,
        //             label_rect.y0 + 14.0,
        //             label_rect.x1 - 46.0,
        //             label_rect.y0 + 40.0,
        //         )
        //         .to_rounded_rect(13.0),
        //         Color::from_rgba8(18, 139, 128, 230),
        //     )
        //     .draw();

        // cx.painter
        //     .stroke(
        //         cube_rect.inflate(14.0, 14.0).to_rounded_rect(26.0),
        //         &Stroke::new(4.0),
        //         Color::from_rgba8(250, 238, 205, 225),
        //     )
        //     .draw();

        cx.painter.fill(canvas, &brush).draw();
    })
}

fn centered_rect(container: Size, size: Size) -> Rect {
    let origin = Point::new(
        (container.width - size.width).max(0.0) * 0.5,
        (container.height - size.height).max(0.0) * 0.5 + 16.0,
    );
    Rect::from_origin_size(origin, size)
}

fn start_cube_target(
    surface: RenderableCompositorSurface,
    gpu_resources: GpuResources,
) -> Result<(), String> {
    let renderer = Arc::new(Mutex::new(CubeRenderer::new(
        gpu_resources.clone(),
        CUBE_SIZE,
        CUBE_SIZE,
        wgpu::TextureFormat::Bgra8Unorm,
    )?));
    let animation_origin = Arc::new(Mutex::new(None::<Instant>));
    let renderer_for_callback = renderer.clone();
    let origin_for_callback = animation_origin.clone();
    surface.set_frame_callback(move |mut cx| {
        let completion_tx = cx.completion_sender();
        let lease = match cx.acquire_target() {
            Ok(lease) => lease,
            Err(subduction::wgpu::SurfaceFrameError::NoTargetAvailable) => {
                return Ok(subduction::wgpu::SurfaceFrameDecision::Skip(
                    subduction::wgpu::SurfaceSkipReason::ProducerBusy,
                ));
            }
            Err(err) => return Err(err),
        };
        let mut origin = origin_for_callback.lock().unwrap();
        let origin = *origin.get_or_insert_with(Instant::now);
        let seconds = origin.elapsed().as_secs_f32();
        match renderer_for_callback.lock().unwrap().render(seconds, lease) {
            Ok(completion) => {
                let _ = completion_tx.send(completion);
            }
            Err(err) => {
                eprintln!("compositor-surface-cube: {err}");
            }
        }
        Ok(subduction::wgpu::SurfaceFrameDecision::Deferred)
    });
    Ok(())
}

struct CubeRenderer {
    gpu_resources: GpuResources,
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
        gpu_resources: GpuResources,
        width: u32,
        height: u32,
        target_format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let device = &gpu_resources.device;
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
            gpu_resources,
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

    fn render(
        &mut self,
        seconds: f32,
        frame: subduction::wgpu::SurfaceFrameLease,
    ) -> Result<subduction::wgpu::SurfaceFrameCompletion, String> {
        if self.width != frame.size.width || self.height != frame.size.height {
            self.width = frame.size.width.max(1);
            self.height = frame.size.height.max(1);
            self.depth = create_depth_texture(&self.gpu_resources.device, self.width, self.height);
        }
        let aspect = self.width as f32 / self.height as f32;
        let model = mul(rotation_y(seconds * 0.9), rotation_x(seconds * 0.55));
        let view = translation(0.0, 0.0, -4.8);
        let projection = perspective(45_f32.to_radians(), aspect, 0.1, 100.0);
        let mvp = mul(projection, mul(view, model));
        self.gpu_resources
            .queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));

        let mut encoder =
            self.gpu_resources
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cube encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cube render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
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

        self.gpu_resources.queue.submit(Some(encoder.finish()));
        let _ = self.gpu_resources.device.poll(wgpu::PollType::Poll);
        Ok(frame.submit())
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
        .create_view(&wgpu::TextureViewDescriptor {
            label: Some("cube depth texture view"),
            ..Default::default()
        })
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
