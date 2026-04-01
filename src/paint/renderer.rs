//! # Renderer
//!
//! This section is to help understand how Floem is implemented for developers of Floem.
//!
//! ## Render loop and update lifecycle
//!
//! event -> update -> layout -> paint.
//!
//! #### Event
//! After an event comes in (e.g. the user clicked the mouse, pressed a key etc), the event will be propagated from the root view to the children.
//! If the parent does not handle the event, it will automatically be sent to the child view. If the parent does handle the event the parent can decide whether the event should continue propagating so that the child can also process the event or if the propagation should stop.
//! The event propagation is stopped whenever an event listener returns `true` on the event handling.
//!
//!
//! #### Event handling -> reactive system updates
//! Event handling is a common place for reactive state changes to occur. E.g., on the counter example, when you click increment,
//! it updates the counter and because the label has an effect that is subscribed to those changes (see [`floem_reactive::create_effect`]), the label will update the text it presents.
//!
//! #### Update
//! The update of states on the Views could cause some of them to need a new layout recalculation, because the size might have changed etc.
//! The reactive system can't directly manipulate the view state of the label because the `WindowState` owns all the views. And instead, it will send the update to a message queue via [`ViewId::update_state`](crate::ViewId::update_state)
//! After the event propagation is done, Floem will process all the update messages in the queue, and it can manipulate the state of a particular view through the update method.
//!
//!
//! #### Layout
//! The layout method is called from the root view to re-layout the views that have requested a layout call.
//! The layout call is to change the layout properties at Taffy, and after the layout call is done, `compute_layout` is called to calculate the sizes and positions of each view.
//!
//! #### Paint
//! And in the end, `paint` is called to render all the views to the screen.
//!
//!
//! ## Terminology
//!
//! Useful definitions for developers of Floem
//!
//! #### Active view
//!
//! Affects pointer events. Pointer events will only be sent to the active view. The view will continue to receive pointer events even if the mouse is outside its bounds.
//! It is useful when you drag things, e.g. the scroll bar, you set the scroll bar active after pointer down, then when you drag, the `PointerMove` will always be sent to the view, even if your mouse is outside of the view.
//!
//! #### Focused view
//! Affects keyboard events. Keyboard events will only be sent to the focused view. The view will continue to receive keyboard events even if it's not the active view.
//!
//! ## Notable invariants and tolerances
//! - There can be only one root `View`
//! - Only one view can be active at a time.
//! - Only one view can be focused at a time.
//!
use std::sync::Arc;

use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::{BeginFrame, RenderCore, RenderOutput, Renderer};
use imaging::{
    BlurredRoundedRect, ClipRef, FillRef, GlyphRunRef, GroupRef, PaintSink, RetainedDrawRef,
    StrokeRef,
};
use peniko::ImageData;
use peniko::kurbo::Size;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use wgpu::util::TextureBlitter;
use winit::window::Window;

use crate::platform::{Duration, Instant};

pub type CpuTargetRenderFn = dyn for<'a> FnMut(
    BeginFrame,
    floem_renderer::CpuBufferTarget<'a>,
    &mut dyn FnMut(&mut dyn RenderCore),
) -> Result<(), String>;
pub type CpuTargetSupportFn =
    dyn Fn(floem_renderer::CpuBufferTargetInfo) -> Result<(), String> + Send + Sync;

pub type CpuRendererInstallFn =
    dyn Fn(CpuRendererInstallCx) -> Result<RendererType, String> + Send + Sync;
pub type GpuRendererInstallFn =
    dyn for<'a> Fn(GpuRendererInstallCx<'a>) -> Result<RendererType, String> + Send + Sync;

pub trait GpuCopyRenderer: Renderer<Target = wgpu::TextureView> {}
impl<T> GpuCopyRenderer for T where T: Renderer<Target = wgpu::TextureView> {}

pub trait CpuCopyRenderer: Renderer<Target = ImageData> {}
impl<T> CpuCopyRenderer for T where T: Renderer<Target = ImageData> {}

#[derive(Clone, Copy)]
pub struct CpuRendererInstallCx {
    pub size: Size,
    pub scale: f64,
    pub font_embolden: f32,
}

#[derive(Clone, Copy)]
pub struct GpuRendererInstallCx<'a> {
    pub size: Size,
    pub scale: f64,
    pub font_embolden: f32,
    pub gpu_resources: &'a GpuResources,
    pub texture_format: wgpu::TextureFormat,
}

enum RendererInstallerKind {
    Cpu(Box<CpuRendererInstallFn>),
    Gpu(Box<GpuRendererInstallFn>),
}

pub struct RendererInstaller {
    pub name: &'static str,
    kind: RendererInstallerKind,
}

impl RendererInstaller {
    pub fn cpu(
        name: &'static str,
        install: impl Fn(CpuRendererInstallCx) -> Result<RendererType, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            kind: RendererInstallerKind::Cpu(Box::new(install)),
        }
    }

    pub fn gpu(
        name: &'static str,
        install: impl for<'a> Fn(GpuRendererInstallCx<'a>) -> Result<RendererType, String>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            name,
            kind: RendererInstallerKind::Gpu(Box::new(install)),
        }
    }

    pub(crate) fn requires_gpu(&self) -> bool {
        matches!(&self.kind, RendererInstallerKind::Gpu(_))
    }
}

struct NullRenderer;

impl RenderCore for NullRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        f(self)
    }

    fn finish(&mut self) {}

    fn readback(&mut self) -> Option<RenderOutput> {
        None
    }

    fn debug_info(&self) -> String {
        "Uninitialized".to_string()
    }
}

impl Renderer for NullRenderer {
    type Target = RenderOutput;

    fn set_size(&mut self, _frame: BeginFrame) {}

    fn reset(&mut self) {}

    fn read_target(&mut self) -> Option<Self::Target> {
        None
    }
}

impl PaintSink for NullRenderer {
    fn push_clip(&mut self, _clip: ClipRef<'_>) {}

    fn pop_clip(&mut self) {}

    fn push_group(&mut self, _group: GroupRef<'_>) {}

    fn pop_group(&mut self) {}

    fn retained(&mut self, _draw: RetainedDrawRef<'_>) {}

    fn fill(&mut self, _draw: FillRef<'_>) {}

    fn stroke(&mut self, _draw: StrokeRef<'_>) {}

    fn glyph_run(
        &mut self,
        _draw: GlyphRunRef<'_>,
        _glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
    }

    fn blurred_rounded_rect(&mut self, _draw: BlurredRoundedRect) {}
}

struct CpuWindowTarget<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
    width: u32,
    height: u32,
}

struct GpuWindowTarget {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: TextureBlitter,
}

pub enum RendererType {
    TargetCpu {
        render: Box<CpuTargetRenderFn>,
        supports: Box<CpuTargetSupportFn>,
    },
    CopyGpu(Box<dyn GpuCopyRenderer>),
    CopyCpu(Box<dyn CpuCopyRenderer>),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaintTiming {
    pub presented: bool,
    pub total: Duration,
    pub resize: Duration,
    pub pre_present_notify: Duration,
    pub prepare: Duration,
    pub scene: Duration,
    pub finalize: Duration,
    pub read_output: Duration,
    pub present: PresentTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PresentTiming {
    pub total: Duration,
    pub acquire_surface: Duration,
    pub compose: Duration,
    pub submit: Duration,
    pub present_call: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureTiming {
    pub total: Duration,
    pub resize: Duration,
    pub pre_present_notify: Duration,
    pub prepare: Duration,
    pub scene: Duration,
    pub finalize: Duration,
    pub readback: Duration,
    pub convert: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureOutput {
    pub image: Option<ImageData>,
    pub timing: CaptureTiming,
}

pub(crate) trait WindowRenderer {
    fn resize(&mut self, width: u32, height: u32);
    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn RenderCore))
    -> PaintTiming;
    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput;
    fn debug_info(&mut self) -> String;
    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        None
    }
}

struct NullWindowBackend {
    renderer: NullRenderer,
}

impl WindowRenderer for NullWindowBackend {
    fn resize(&mut self, _width: u32, _height: u32) {}

    fn render(
        &mut self,
        _begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        paint(&mut self.renderer);
        self.renderer.finish();
        PaintTiming::default()
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        paint(&mut self.renderer);
        self.renderer.finish();
        CaptureOutput::default()
    }

    fn debug_info(&mut self) -> String {
        self.renderer.debug_info()
    }
}

#[allow(dead_code)]
struct CpuDirectWindowRenderer<R> {
    target: CpuWindowTarget<Arc<dyn Window>>,
    _marker: std::marker::PhantomData<R>,
}

struct GpuCopyWindowRenderer {
    renderer: Box<dyn GpuCopyRenderer>,
    target: GpuWindowTarget,
}

struct CpuCopyWindowRenderer {
    renderer: Box<dyn CpuCopyRenderer>,
    target: CpuWindowTarget<Arc<dyn Window>>,
}

struct CaptureBuffer {
    data: Vec<u8>,
    width: u32,
    height: u32,
    bytes_per_row: usize,
}

impl CaptureBuffer {
    fn with_target(
        &mut self,
        width: u32,
        height: u32,
        f: &mut dyn FnMut(floem_renderer::CpuBufferTarget<'_>),
    ) -> ImageData {
        let width = width.max(1);
        let height = height.max(1);
        let bytes_per_row = width as usize * 4;
        let len = bytes_per_row * height as usize;
        if self.data.len() != len {
            self.data.resize(len, 0);
        }
        self.width = width;
        self.height = height;
        self.bytes_per_row = bytes_per_row;
        f(floem_renderer::CpuBufferTarget {
            buffer: &mut self.data,
            width,
            height,
            bytes_per_row,
            format: floem_renderer::CpuBufferFormat::Bgra8Opaque,
        });
        ImageData {
            data: peniko::Blob::new(Arc::new(self.data.clone())),
            format: peniko::ImageFormat::Bgra8,
            width,
            height,
            alpha_type: peniko::ImageAlphaType::Alpha,
        }
    }
}

struct TargetCpuWindowRenderer {
    renderer: Box<CpuTargetRenderFn>,
    target: CpuWindowTarget<Arc<dyn Window>>,
    capture: CaptureBuffer,
    debug_name: &'static str,
}

impl GpuWindowTarget {
    fn new(
        gpu_resources: &GpuResources,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<Self, String> {
        let texture_format = choose_surface_texture_format(&surface, gpu_resources)?;

        let latency = match gpu_resources.adapter.get_info().backend {
            wgpu::Backend::Vulkan => 2,
            _ => 1,
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format: texture_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: if transparent {
                wgpu::CompositeAlphaMode::Auto
            } else {
                wgpu::CompositeAlphaMode::Opaque
            },
            view_formats: vec![],
            desired_maximum_frame_latency: latency,
        };
        surface.configure(&gpu_resources.device, &config);

        Ok(Self {
            device: gpu_resources.device.clone(),
            queue: gpu_resources.queue.clone(),
            surface,
            config,
            blitter: TextureBlitter::new(&gpu_resources.device, texture_format),
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn present(&mut self, output: &wgpu::TextureView) -> PresentTiming {
        let start = Instant::now();
        let acquire_start = start;
        let surface_texture = self
            .surface
            .get_current_texture()
            .expect("failed to acquire surface texture");
        let acquire_surface = acquire_start.elapsed();
        let compose_start = Instant::now();
        let output_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Floem Surface Blit"),
            });
        let output_texture = output.texture();
        let surface_copy_size = surface_texture.texture.size();
        if output_texture.format() == surface_texture.texture.format()
            && output_texture.size() == surface_copy_size
        {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: output_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &surface_texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                surface_copy_size,
            );
        } else {
            self.blitter
                .copy(&self.device, &mut encoder, output, &output_view);
        }
        let compose = compose_start.elapsed();
        let submit_start = Instant::now();
        self.queue.submit([encoder.finish()]);
        let submit = submit_start.elapsed();
        let present_call_start = Instant::now();
        surface_texture.present();
        let present_call = present_call_start.elapsed();
        PresentTiming {
            total: start.elapsed(),
            acquire_surface,
            compose,
            submit,
            present_call,
        }
    }

    fn gpu_surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }
}

fn choose_surface_texture_format(
    surface: &wgpu::Surface<'static>,
    gpu_resources: &GpuResources,
) -> Result<wgpu::TextureFormat, String> {
    let surface_caps = surface.get_capabilities(&gpu_resources.adapter);
    surface_caps
        .formats
        .into_iter()
        .find(|it| {
            matches!(
                it,
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
            )
        })
        .ok_or_else(|| "surface should support Rgba8Unorm or Bgra8Unorm".to_string())
}

impl<W> CpuWindowTarget<W>
where
    W: Clone + raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle,
{
    fn new(window: W, width: u32, height: u32) -> Result<Self, String> {
        let context = Context::new(window.clone()).map_err(|err| err.to_string())?;
        let mut surface = Surface::new(&context, window).map_err(|err| err.to_string())?;
        surface
            .resize(
                NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .map_err(|err| err.to_string())?;
        Ok(Self {
            context,
            surface,
            width,
            height,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.surface
            .resize(
                NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .expect("failed to resize surface");
    }

    fn present(&mut self, image: &ImageData) -> PresentTiming {
        let start = Instant::now();
        let acquire_start = start;
        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");
        let acquire_surface = acquire_start.elapsed();
        let compose_start = Instant::now();
        let width = image.width as usize;
        for y in 0..image.height as usize {
            let row_start = y * width;
            let src = &image.data.data()[row_start * 4..(row_start + width) * 4];
            let dst = &mut buffer[row_start..row_start + width];
            for (out_pixel, rgba) in dst.iter_mut().zip(src.chunks_exact(4)) {
                *out_pixel = ((rgba[0] as u32) << 16) | ((rgba[1] as u32) << 8) | (rgba[2] as u32);
            }
        }
        let compose = compose_start.elapsed();
        let present_call_start = Instant::now();
        buffer
            .present()
            .expect("failed to present the surface buffer");
        let present_call = present_call_start.elapsed();
        PresentTiming {
            total: start.elapsed(),
            acquire_surface,
            compose,
            submit: Duration::ZERO,
            present_call,
        }
    }

    fn with_cpu_target(&mut self, f: &mut dyn FnMut(floem_renderer::CpuBufferTarget<'_>)) {
        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");
        let width = self.width.max(1);
        let height = self.height.max(1);
        let bytes_per_row = width as usize * std::mem::size_of::<u32>();
        let pixel_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                buffer.as_mut_ptr().cast::<u8>(),
                buffer.len() * std::mem::size_of::<u32>(),
            )
        };
        f(floem_renderer::CpuBufferTarget {
            buffer: pixel_bytes,
            width,
            height,
            bytes_per_row,
            format: floem_renderer::CpuBufferFormat::Bgra8Opaque,
        });
        buffer
            .present()
            .expect("failed to present the surface buffer");
    }
}

#[cfg(feature = "vello-hybrid")]
impl WindowRenderer
    for CpuDirectWindowRenderer<imaging_vello_hybrid::VelloHybridTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        use floem_renderer::TargetRenderer;
        let start = Instant::now();
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                imaging_vello_hybrid::VelloHybridTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
        PaintTiming {
            presented: true,
            total: start.elapsed(),
            scene: start.elapsed(),
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        CaptureOutput::default()
    }

    fn debug_info(&mut self) -> String {
        "Vello Hybrid".to_string()
    }
}

#[cfg(feature = "vello-cpu")]
impl WindowRenderer
    for CpuDirectWindowRenderer<imaging_vello_cpu::VelloCpuTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        use floem_renderer::TargetRenderer;
        let start = Instant::now();
        self.target.with_cpu_target(&mut |target| {
            let mut renderer = imaging_vello_cpu::VelloCpuTargetRenderer::create(begin, target)
                .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
        PaintTiming {
            presented: true,
            total: start.elapsed(),
            scene: start.elapsed(),
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        CaptureOutput::default()
    }

    fn debug_info(&mut self) -> String {
        "Vello CPU".to_string()
    }
}

#[cfg(feature = "skia-cpu")]
impl WindowRenderer for CpuDirectWindowRenderer<imaging_skia::SkiaCpuTargetRenderer<'static>> {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        use floem_renderer::TargetRenderer;
        let start = Instant::now();
        self.target.with_cpu_target(&mut |target| {
            let mut renderer = imaging_skia::SkiaCpuTargetRenderer::create(begin, target)
                .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
        PaintTiming {
            presented: true,
            total: start.elapsed(),
            scene: start.elapsed(),
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        CaptureOutput::default()
    }

    fn debug_info(&mut self) -> String {
        "Skia CPU".to_string()
    }
}

#[cfg(feature = "tiny-skia")]
impl WindowRenderer
    for CpuDirectWindowRenderer<floem_tiny_skia_renderer::TinySkiaTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        use floem_renderer::TargetRenderer;
        let start = Instant::now();
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                floem_tiny_skia_renderer::TinySkiaTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
        PaintTiming {
            presented: true,
            total: start.elapsed(),
            scene: start.elapsed(),
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        CaptureOutput::default()
    }

    fn debug_info(&mut self) -> String {
        "Tiny Skia".to_string()
    }
}

impl WindowRenderer for TargetCpuWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        let start = Instant::now();
        self.target.with_cpu_target(&mut |target| {
            (self.renderer)(begin, target, paint).expect("failed to render cpu target");
        });
        PaintTiming {
            presented: true,
            total: start.elapsed(),
            scene: start.elapsed(),
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        let start = Instant::now();
        let image = Some(self.capture.with_target(
            begin.size.width as u32,
            begin.size.height as u32,
            &mut |target| {
                (self.renderer)(begin, target, paint).expect("failed to capture cpu target");
            },
        ));
        CaptureOutput {
            image,
            timing: CaptureTiming {
                total: start.elapsed(),
                scene: start.elapsed(),
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        self.debug_name.to_string()
    }
}

impl WindowRenderer for GpuCopyWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        let total_start = Instant::now();
        let prepare_start = total_start;
        self.renderer.set_size(begin);
        self.renderer.reset();
        let prepare = prepare_start.elapsed();
        let scene_start = Instant::now();
        paint(&mut *self.renderer);
        let scene = scene_start.elapsed();
        let finalize_start = Instant::now();
        self.renderer.finish();
        let finalize = finalize_start.elapsed();
        let read_output_start = Instant::now();
        let output = self.renderer.read_target();
        let read_output = read_output_start.elapsed();
        let present = output
            .as_ref()
            .map(|output| self.target.present(output))
            .unwrap_or_default();
        PaintTiming {
            presented: output.is_some(),
            total: total_start.elapsed(),
            prepare,
            scene,
            finalize,
            read_output,
            present,
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        let total_start = Instant::now();
        let prepare_start = total_start;
        self.renderer.set_size(begin);
        self.renderer.reset();
        let prepare = prepare_start.elapsed();
        let scene_start = Instant::now();
        paint(&mut *self.renderer);
        let scene = scene_start.elapsed();
        let finalize_start = Instant::now();
        self.renderer.finish();
        let finalize = finalize_start.elapsed();
        let readback_start = Instant::now();
        let output = self.renderer.readback();
        let readback = readback_start.elapsed();
        let convert_start = Instant::now();
        let image = output.and_then(|output| output.into_image_with(&self.target.device, &self.target.queue));
        let convert = convert_start.elapsed();
        CaptureOutput {
            image,
            timing: CaptureTiming {
                total: total_start.elapsed(),
                prepare,
                scene,
                finalize,
                readback,
                convert,
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        self.renderer.debug_info()
    }

    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        Some(self.target.gpu_surface())
    }
}

impl WindowRenderer for CpuCopyWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> PaintTiming {
        let total_start = Instant::now();
        let prepare_start = total_start;
        self.renderer.set_size(begin);
        self.renderer.reset();
        let prepare = prepare_start.elapsed();
        let scene_start = Instant::now();
        paint(&mut *self.renderer);
        let scene = scene_start.elapsed();
        let finalize_start = Instant::now();
        self.renderer.finish();
        let finalize = finalize_start.elapsed();
        let read_output_start = Instant::now();
        let output = self.renderer.read_target();
        let read_output = read_output_start.elapsed();
        let present = output
            .as_ref()
            .map(|output| self.target.present(output))
            .unwrap_or_default();
        PaintTiming {
            presented: output.is_some(),
            total: total_start.elapsed(),
            prepare,
            scene,
            finalize,
            read_output,
            present,
            ..Default::default()
        }
    }

    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn RenderCore),
    ) -> CaptureOutput {
        let total_start = Instant::now();
        let prepare_start = total_start;
        self.renderer.set_size(begin);
        self.renderer.reset();
        let prepare = prepare_start.elapsed();
        let scene_start = Instant::now();
        paint(&mut *self.renderer);
        let scene = scene_start.elapsed();
        let finalize_start = Instant::now();
        self.renderer.finish();
        let finalize = finalize_start.elapsed();
        let readback_start = Instant::now();
        let image = self.renderer.read_target();
        let readback = readback_start.elapsed();
        CaptureOutput {
            image,
            timing: CaptureTiming {
                total: total_start.elapsed(),
                prepare,
                scene,
                finalize,
                readback,
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        self.renderer.debug_info()
    }
}

fn env_flag_requested(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| value.as_str() == "1")
}

pub(crate) fn force_cpu_requested() -> bool {
    env_flag_requested("FLOEM_FORCE_CPU") || env_flag_requested("FLOEM_FORCE_TINY_SKIA")
}

pub(crate) type WindowBackend = Box<dyn WindowRenderer>;

pub(crate) fn uninitialized_backend() -> WindowBackend {
    Box::new(NullWindowBackend {
        renderer: NullRenderer,
    })
}

fn cpu_install_cx(size: Size, scale: f64, font_embolden: f32) -> CpuRendererInstallCx {
    CpuRendererInstallCx {
        size,
        scale,
        font_embolden,
    }
}

fn gpu_install_cx<'a>(
    size: Size,
    scale: f64,
    font_embolden: f32,
    gpu_resources: &'a GpuResources,
    texture_format: wgpu::TextureFormat,
) -> GpuRendererInstallCx<'a> {
    GpuRendererInstallCx {
        size,
        scale,
        font_embolden,
        gpu_resources,
        texture_format,
    }
}

fn invoke_installer(
    installer: &RendererInstaller,
    cpu_cx: CpuRendererInstallCx,
    gpu_cx: Option<GpuRendererInstallCx<'_>>,
) -> Result<Option<RendererType>, String> {
    match (&installer.kind, gpu_cx) {
        (RendererInstallerKind::Cpu(install), _) => install(cpu_cx).map(Some),
        (RendererInstallerKind::Gpu(install), Some(gpu_cx)) => install(gpu_cx).map(Some),
        (RendererInstallerKind::Gpu(_), None) => Ok(None),
    }
}

fn create_window_backend(
    installer_name: &'static str,
    renderer_type: RendererType,
    window: Arc<dyn Window>,
    gpu_resources: Option<&GpuResources>,
    surface: Option<wgpu::Surface<'static>>,
    transparent: bool,
    width: u32,
    height: u32,
) -> Result<WindowBackend, String> {
    match renderer_type {
        RendererType::TargetCpu { render, .. } => Ok(Box::new(TargetCpuWindowRenderer {
            renderer: render,
            target: CpuWindowTarget::new(window, width, height)?,
            capture: CaptureBuffer {
                data: Vec::new(),
                width: 0,
                height: 0,
                bytes_per_row: 0,
            },
            debug_name: installer_name,
        })),
        RendererType::CopyGpu(renderer) => {
            let gpu_resources = gpu_resources
                .ok_or_else(|| format!("renderer installer {installer_name} requires GPU"))?;
            let surface = surface.ok_or_else(|| {
                format!("renderer installer {installer_name} requires GPU surface")
            })?;
            Ok(Box::new(GpuCopyWindowRenderer {
                renderer,
                target: GpuWindowTarget::new(gpu_resources, surface, width, height, transparent)?,
            }))
        }
        RendererType::CopyCpu(renderer) => Ok(Box::new(CpuCopyWindowRenderer {
            renderer,
            target: CpuWindowTarget::new(window, width, height)?,
        })),
    }
}

fn create_installed_renderer(
    installers: &[RendererInstaller],
    window: Arc<dyn Window>,
    gpu_resources: Option<&GpuResources>,
    surface: Option<wgpu::Surface<'static>>,
    transparent: bool,
    scale: f64,
    size: Size,
    font_embolden: f32,
    allow_gpu: bool,
) -> Result<WindowBackend, String> {
    let size = Size::new(size.width.max(1.0), size.height.max(1.0));
    let width = size.width as u32;
    let height = size.height as u32;
    let cpu_target_info = floem_renderer::CpuBufferTargetInfo {
        width,
        height,
        bytes_per_row: width as usize * 4,
        format: floem_renderer::CpuBufferFormat::Bgra8Opaque,
    };
    let cpu_cx = cpu_install_cx(size, scale, font_embolden);
    let texture_format = if allow_gpu {
        match (surface.as_ref(), gpu_resources) {
            (Some(surface), Some(gpu_resources)) => {
                Some(choose_surface_texture_format(surface, gpu_resources)?)
            }
            _ => None,
        }
    } else {
        None
    };
    let gpu_cx = match (gpu_resources, texture_format) {
        (Some(gpu_resources), Some(texture_format)) => Some(gpu_install_cx(
            size,
            scale,
            font_embolden,
            gpu_resources,
            texture_format,
        )),
        _ => None,
    };

    let mut surface = surface;
    let mut errors = Vec::new();
    for installer in installers {
        if !allow_gpu && installer.requires_gpu() {
            continue;
        }
        let renderer_type = match invoke_installer(installer, cpu_cx, gpu_cx) {
            Ok(Some(renderer_type)) => renderer_type,
            Ok(None) => continue,
            Err(err) => {
                errors.push(format!("{}: {err}", installer.name));
                continue;
            }
        };

        if let RendererType::TargetCpu { supports, .. } = &renderer_type
            && let Err(err) = supports(cpu_target_info)
        {
            errors.push(format!("{}: {err}", installer.name));
            continue;
        }

        let window_backend = match create_window_backend(
            installer.name,
            renderer_type,
            window.clone(),
            gpu_resources,
            surface.take(),
            transparent,
            width,
            height,
        ) {
            Ok(window_backend) => window_backend,
            Err(err) => return Err(format!("{}: {err}", installer.name)),
        };
        return Ok(window_backend);
    }

    if errors.is_empty() {
        Err("no renderer installers succeeded".to_string())
    } else {
        Err(errors.join("; "))
    }
}

pub(crate) fn new(
    installers: &[RendererInstaller],
    window: Arc<dyn Window>,
    gpu_resources: GpuResources,
    surface: wgpu::Surface<'static>,
    transparent: bool,
    scale: f64,
    size: Size,
    font_embolden: f32,
) -> WindowBackend {
    create_installed_renderer(
        installers,
        window,
        Some(&gpu_resources),
        Some(surface),
        transparent,
        scale,
        size,
        font_embolden,
        !force_cpu_requested(),
    )
    .expect("create renderer")
}

#[allow(unreachable_code)]
pub(crate) fn new_cpu(
    installers: &[RendererInstaller],
    window: Arc<dyn Window>,
    scale: f64,
    size: Size,
    font_embolden: f32,
) -> WindowBackend {
    create_installed_renderer(
        installers,
        window,
        None,
        None,
        false,
        scale,
        size,
        font_embolden,
        false,
    )
    .expect("create cpu renderer")
}

pub fn default_renderer_installers() -> Vec<RendererInstaller> {
    let mut installers = Vec::new();

    #[cfg(feature = "vello")]
    installers.push(RendererInstaller::gpu("Vello", |cx| {
        Ok(RendererType::CopyGpu(Box::new(
            floem_vello_renderer::VelloRenderer::new(
                cx.gpu_resources.clone(),
                cx.size.width.max(1.0) as u32,
                cx.size.height.max(1.0) as u32,
                cx.texture_format,
                cx.scale,
                cx.font_embolden,
            )
            .map_err(|err| err.to_string())?,
        )))
    }));

    #[cfg(feature = "vger")]
    installers.push(RendererInstaller::gpu("Vger", |cx| {
        Ok(RendererType::CopyGpu(Box::new(
            floem_vger_renderer::VgerRenderer::new(
                cx.gpu_resources.clone(),
                cx.size.width.max(1.0) as u32,
                cx.size.height.max(1.0) as u32,
                cx.texture_format,
                cx.scale,
                cx.font_embolden,
            )
            .map_err(|err| err.to_string())?,
        )))
    }));

    #[cfg(feature = "skia")]
    installers.push(RendererInstaller::gpu("Skia", |cx| {
        Ok(RendererType::CopyGpu(Box::new(
            floem_skia_renderer::SkiaRenderer::new(
                cx.gpu_resources.clone(),
                cx.size.width.max(1.0) as u32,
                cx.size.height.max(1.0) as u32,
                cx.texture_format,
                cx.scale,
                cx.font_embolden,
            )
            .map_err(|err| err.to_string())?,
        )))
    }));

    #[cfg(feature = "vello-hybrid")]
    installers.push(RendererInstaller::cpu("Vello Hybrid", |_cx| {
        Ok(RendererType::TargetCpu {
            render: Box::new(|begin, target, paint| {
                use floem_renderer::TargetRenderer;
                let mut renderer =
                    imaging_vello_hybrid::VelloHybridTargetRenderer::create(begin, target)?;
                paint(&mut renderer);
                renderer.finish();
                Ok(())
            }),
            supports: Box::new(|target| {
                <imaging_vello_hybrid::VelloHybridTargetRenderer<'_> as floem_renderer::TargetRenderer>::supports_cpu_buffer_target(&target)
            }),
        })
    }));

    #[cfg(feature = "vello-cpu")]
    installers.push(RendererInstaller::cpu("Vello CPU", |_cx| {
        Ok(RendererType::TargetCpu {
            render: Box::new(|begin, target, paint| {
                use floem_renderer::TargetRenderer;
                let mut renderer = imaging_vello_cpu::VelloCpuTargetRenderer::create(begin, target)?;
                paint(&mut renderer);
                renderer.finish();
                Ok(())
            }),
            supports: Box::new(|target| {
                <imaging_vello_cpu::VelloCpuTargetRenderer<'_> as floem_renderer::TargetRenderer>::supports_cpu_buffer_target(&target)
            }),
        })
    }));

    #[cfg(feature = "skia-cpu")]
    installers.push(RendererInstaller::cpu("Skia CPU", |_cx| {
        Ok(RendererType::TargetCpu {
            render: Box::new(|begin, target, paint| {
                use floem_renderer::TargetRenderer;
                let mut renderer = imaging_skia::SkiaCpuTargetRenderer::create(begin, target)?;
                paint(&mut renderer);
                renderer.finish();
                Ok(())
            }),
            supports: Box::new(|target| {
                <imaging_skia::SkiaCpuTargetRenderer<'_> as floem_renderer::TargetRenderer>::supports_cpu_buffer_target(&target)
            }),
        })
    }));

    #[cfg(feature = "tiny-skia")]
    installers.push(RendererInstaller::cpu("Tiny Skia", |_cx| {
        Ok(RendererType::TargetCpu {
            render: Box::new(|begin, target, paint| {
                use floem_renderer::TargetRenderer;
                let mut renderer =
                    floem_tiny_skia_renderer::TinySkiaTargetRenderer::create(begin, target)?;
                paint(&mut renderer);
                renderer.finish();
                Ok(())
            }),
            supports: Box::new(|target| {
                <floem_tiny_skia_renderer::TinySkiaTargetRenderer<'_> as floem_renderer::TargetRenderer>::supports_cpu_buffer_target(&target)
            }),
        })
    }));

    #[cfg(feature = "tiny-skia")]
    installers.push(RendererInstaller::cpu("Tiny Skia Copy", |cx| {
        Ok(RendererType::CopyCpu(Box::new(
            floem_tiny_skia_renderer::TinySkiaRenderer::new(
                cx.size.width.max(1.0) as u32,
                cx.size.height.max(1.0) as u32,
                cx.scale,
                cx.font_embolden,
            )
            .map_err(|err| err.to_string())?,
        )))
    }));

    #[cfg(feature = "vello-cpu")]
    installers.push(RendererInstaller::cpu("Vello CPU Copy", |cx| {
        let width = u16::try_from(cx.size.width.max(1.0) as u32)
            .map_err(|_| "width exceeds vello_cpu limit".to_string())?;
        let height = u16::try_from(cx.size.height.max(1.0) as u32)
            .map_err(|_| "height exceeds vello_cpu limit".to_string())?;
        Ok(RendererType::CopyCpu(Box::new(
            imaging_vello_cpu::VelloCpuRenderer::new(width, height),
        )))
    }));

    installers
}
