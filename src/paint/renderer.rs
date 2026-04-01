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
use floem_renderer::{
    BeginFrame, CustomRenderer, DisplayCommandExt, RenderCore, RenderOutput, Renderer,
    SceneRenderer,
};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    RetainedDrawRef, StrokeRef,
};
use peniko::ImageData;
use peniko::kurbo::Size;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use wgpu::util::TextureBlitter;
use winit::window::Window;

pub type CpuTargetRenderFn = dyn for<'a> FnMut(
    BeginFrame,
    floem_renderer::CpuBufferTarget<'a>,
    &mut dyn FnMut(&mut dyn SceneRenderer),
) -> Result<(), String>;

pub type CpuRendererInstallFn =
    dyn Fn(CpuRendererInstallCx) -> Result<RendererType, String> + Send + Sync;
pub type GpuRendererInstallFn =
    dyn for<'a> Fn(GpuRendererInstallCx<'a>) -> Result<RendererType, String> + Send + Sync;

pub trait GpuCopyRenderer: Renderer<Target = wgpu::TextureView> + SceneRenderer {}
impl<T> GpuCopyRenderer for T where T: Renderer<Target = wgpu::TextureView> + SceneRenderer {}

pub trait CpuCopyRenderer: Renderer<Target = ImageData> + SceneRenderer {}
impl<T> CpuCopyRenderer for T where T: Renderer<Target = ImageData> + SceneRenderer {}

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

impl CustomPaintSink<DisplayCommandExt> for NullRenderer {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}

impl CustomRenderer for NullRenderer {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        f(self)
    }

    fn debug_info(&self) -> String {
        "Uninitialized".to_string()
    }
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
    TargetCpu(Box<CpuTargetRenderFn>),
    CopyGpu(Box<dyn GpuCopyRenderer>),
    CopyCpu(Box<dyn CpuCopyRenderer>),
}

pub(crate) trait WindowRenderer {
    fn resize(&mut self, width: u32, height: u32);
    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer));
    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData>;
    fn debug_info(&self) -> String;
    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        None
    }
}

struct NullWindowBackend {
    renderer: NullRenderer,
}

impl WindowRenderer for NullWindowBackend {
    fn resize(&mut self, _width: u32, _height: u32) {}

    fn render(&mut self, _begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        paint(&mut self.renderer);
        self.renderer.finish();
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        paint(&mut self.renderer);
        self.renderer.finish();
        None
    }

    fn debug_info(&self) -> String {
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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

    fn present(&mut self, output: &wgpu::TextureView) {
        let surface_texture = self
            .surface
            .get_current_texture()
            .expect("failed to acquire surface texture");
        let output_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Floem Surface Blit"),
            });
        self.blitter
            .copy(&self.device, &mut encoder, output, &output_view);
        self.queue.submit([encoder.finish()]);
        surface_texture.present();
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

    fn present(&mut self, image: &ImageData) {
        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");
        let width = image.width as usize;
        for y in 0..image.height as usize {
            let row_start = y * width;
            let src = &image.data.data()[row_start * 4..(row_start + width) * 4];
            let dst = &mut buffer[row_start..row_start + width];
            for (out_pixel, rgba) in dst.iter_mut().zip(src.chunks_exact(4)) {
                *out_pixel = ((rgba[0] as u32) << 16) | ((rgba[1] as u32) << 8) | (rgba[2] as u32);
            }
        }
        buffer
            .present()
            .expect("failed to present the surface buffer");
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

#[cfg(feature = "active-vello-hybrid")]
impl WindowRenderer
    for CpuDirectWindowRenderer<floem_vello_hybrid_renderer::VelloHybridTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                floem_vello_hybrid_renderer::VelloHybridTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        None
    }

    fn debug_info(&self) -> String {
        "Vello Hybrid".to_string()
    }
}

#[cfg(feature = "active-vello-cpu")]
impl WindowRenderer
    for CpuDirectWindowRenderer<floem_vello_cpu_renderer::VelloCpuTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        use floem_renderer::TargetRenderer;
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                floem_vello_cpu_renderer::VelloCpuTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        None
    }

    fn debug_info(&self) -> String {
        "Vello CPU".to_string()
    }
}

#[cfg(feature = "active-skia-cpu")]
impl WindowRenderer
    for CpuDirectWindowRenderer<floem_skia_renderer::SkiaCpuTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        use floem_renderer::TargetRenderer;
        self.target.with_cpu_target(&mut |target| {
            let mut renderer = floem_skia_renderer::SkiaCpuTargetRenderer::create(begin, target)
                .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        None
    }

    fn debug_info(&self) -> String {
        "Skia CPU".to_string()
    }
}

#[cfg(feature = "active-tiny-skia")]
impl WindowRenderer
    for CpuDirectWindowRenderer<floem_tiny_skia_renderer::TinySkiaTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        use floem_renderer::TargetRenderer;
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                floem_tiny_skia_renderer::TinySkiaTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
    }

    fn capture(
        &mut self,
        _begin: BeginFrame,
        _paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        None
    }

    fn debug_info(&self) -> String {
        "Tiny Skia".to_string()
    }
}

impl WindowRenderer for TargetCpuWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.target.with_cpu_target(&mut |target| {
            (self.renderer)(begin, target, paint).expect("failed to render cpu target");
        });
    }

    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        Some(self.capture.with_target(
            begin.size.width as u32,
            begin.size.height as u32,
            &mut |target| {
                (self.renderer)(begin, target, paint).expect("failed to capture cpu target");
            },
        ))
    }

    fn debug_info(&self) -> String {
        self.debug_name.to_string()
    }
}

impl WindowRenderer for GpuCopyWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.renderer.set_size(begin);
        self.renderer.reset();
        paint(&mut *self.renderer);
        self.renderer.finish();
        if let Some(output) = self.renderer.read_target() {
            self.target.present(&output);
        }
    }

    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        self.renderer.set_size(begin);
        self.renderer.reset();
        paint(&mut *self.renderer);
        self.renderer.finish();
        self.renderer
            .readback()
            .and_then(|output| output.into_image_with(&self.target.device, &self.target.queue))
    }

    fn debug_info(&self) -> String {
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

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.renderer.set_size(begin);
        self.renderer.reset();
        paint(&mut *self.renderer);
        self.renderer.finish();
        if let Some(output) = self.renderer.read_target() {
            self.target.present(&output);
        }
    }

    fn capture(
        &mut self,
        begin: BeginFrame,
        paint: &mut dyn FnMut(&mut dyn SceneRenderer),
    ) -> Option<ImageData> {
        self.renderer.set_size(begin);
        self.renderer.reset();
        paint(&mut *self.renderer);
        self.renderer.finish();
        self.renderer.read_target()
    }

    fn debug_info(&self) -> String {
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
        RendererType::TargetCpu(renderer) => Ok(Box::new(TargetCpuWindowRenderer {
            renderer,
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

    #[cfg(feature = "active-vello")]
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

    #[cfg(feature = "active-vger")]
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

    #[cfg(feature = "active-skia")]
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

    #[cfg(feature = "active-vello-hybrid")]
    installers.push(RendererInstaller::cpu("Vello Hybrid", |_cx| {
        Ok(RendererType::TargetCpu(Box::new(|begin, target, paint| {
            use floem_renderer::TargetRenderer;
            let mut renderer =
                floem_vello_hybrid_renderer::VelloHybridTargetRenderer::create(begin, target)?;
            paint(&mut renderer);
            renderer.finish();
            Ok(())
        })))
    }));

    #[cfg(feature = "active-vello-cpu")]
    installers.push(RendererInstaller::cpu("Vello CPU", |_cx| {
        Ok(RendererType::TargetCpu(Box::new(|begin, target, paint| {
            use floem_renderer::TargetRenderer;
            let mut renderer =
                floem_vello_cpu_renderer::VelloCpuTargetRenderer::create(begin, target)?;
            paint(&mut renderer);
            renderer.finish();
            Ok(())
        })))
    }));

    #[cfg(feature = "active-skia-cpu")]
    installers.push(RendererInstaller::cpu("Skia CPU", |_cx| {
        Ok(RendererType::TargetCpu(Box::new(|begin, target, paint| {
            use floem_renderer::TargetRenderer;
            let mut renderer = floem_skia_renderer::SkiaCpuTargetRenderer::create(begin, target)?;
            paint(&mut renderer);
            renderer.finish();
            Ok(())
        })))
    }));

    #[cfg(feature = "active-tiny-skia")]
    installers.push(RendererInstaller::cpu("Tiny Skia", |_cx| {
        Ok(RendererType::TargetCpu(Box::new(|begin, target, paint| {
            use floem_renderer::TargetRenderer;
            let mut renderer =
                floem_tiny_skia_renderer::TinySkiaTargetRenderer::create(begin, target)?;
            paint(&mut renderer);
            renderer.finish();
            Ok(())
        })))
    }));

    #[cfg(feature = "fallback-vello-cpu")]
    installers.push(RendererInstaller::cpu("Vello CPU Copy", |cx| {
        Ok(RendererType::CopyCpu(Box::new(
            floem_vello_cpu_renderer::VelloCpuRenderer::new(
                cx.size.width.max(1.0) as u32,
                cx.size.height.max(1.0) as u32,
                cx.scale,
                cx.font_embolden,
            )
            .map_err(|err| err.to_string())?,
        )))
    }));

    #[cfg(feature = "fallback-tiny-skia")]
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

    installers
}
