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

use crate::gpu_resources::GpuResources;
use imaging::{
    BlurredRoundedRect, ClipRef, FillRef, GlyphRunRef, GroupRef, ImageBufferTarget, ImageRenderer,
    PaintSink, RenderSource, RgbaImage, StrokeRef, TextureRenderer, TextureViewTarget,
};
use peniko::ImageData;
use peniko::kurbo::Size;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use winit::window::Window;

use crate::platform::{Duration, Instant};

pub(crate) type WindowBackend = Box<dyn WindowRenderer>;

pub struct NewRendererCx {
    pub window: Arc<dyn Window>,
    pub gpu_resources: Option<GpuResources>,
    pub surface: Option<wgpu::Surface<'static>>,
    pub transparent: bool,
    pub size: Size,
    pub scale: f64,
}

impl NewRendererCx {
    fn normalized_size(&self) -> Size {
        Size::new(self.size.width.max(1.0), self.size.height.max(1.0))
    }

    pub fn gpu(&self) -> Option<GpuRendererChooserCx<'_>> {
        if force_cpu_requested() {
            return None;
        }

        match (&self.surface, &self.gpu_resources) {
            (Some(surface), Some(gpu_resources)) => Some(GpuRendererChooserCx {
                gpu_resources,
                surface_caps: surface.get_capabilities(&gpu_resources.adapter),
            }),
            _ => None,
        }
    }

    fn into_renderer(
        self,
        renderer: AcceptedRenderer,
        preferred_texture_format: Option<wgpu::TextureFormat>,
    ) -> Result<WindowBackend, String> {
        let size = self.normalized_size();
        match renderer {
            AcceptedRenderer::Image { backend, name } => Ok(Box::new(ImageWindowRenderer {
                backend,
                name,
                target: CpuWindowTarget::new(self.window, size.width as u32, size.height as u32)?,
                scratch: RgbaImage::new(size.width.max(1.0) as u32, size.height.max(1.0) as u32),
            })),
            AcceptedRenderer::Texture { backend, name } => {
                let gpu_resources = self
                    .gpu_resources
                    .ok_or_else(|| "renderer requires GPU".to_string())?;
                let surface = self
                    .surface
                    .ok_or_else(|| "renderer requires GPU surface".to_string())?;
                Ok(Box::new(TargetGpuWindowRenderer {
                    backend: GpuAcceptedRenderer::Texture { backend, name },
                    target: GpuWindowTarget::new(
                        &gpu_resources,
                        surface,
                        size.width as u32,
                        size.height as u32,
                        self.transparent,
                        preferred_texture_format,
                    )?,
                }))
            }
            AcceptedRenderer::TextureView { backend, name } => {
                let gpu_resources = self
                    .gpu_resources
                    .ok_or_else(|| "renderer requires GPU".to_string())?;
                let surface = self
                    .surface
                    .ok_or_else(|| "renderer requires GPU surface".to_string())?;
                Ok(Box::new(TargetGpuWindowRenderer {
                    backend: GpuAcceptedRenderer::TextureView { backend, name },
                    target: GpuWindowTarget::new(
                        &gpu_resources,
                        surface,
                        size.width as u32,
                        size.height as u32,
                        self.transparent,
                        preferred_texture_format,
                    )?,
                }))
            }
        }
    }
}

pub struct GpuRendererChooserCx<'a> {
    pub gpu_resources: &'a GpuResources,
    pub surface_caps: wgpu::SurfaceCapabilities,
}

impl GpuRendererChooserCx<'_> {
    pub fn surface_formats(&self) -> &[wgpu::TextureFormat] {
        &self.surface_caps.formats
    }
}

fn rgba_image_into_image_data(image: RgbaImage) -> ImageData {
    ImageData {
        data: peniko::Blob::new(Arc::new(image.data)),
        format: peniko::ImageFormat::Rgba8,
        width: image.width,
        height: image.height,
        alpha_type: peniko::ImageAlphaType::Alpha,
    }
}

trait TextureImageRenderer: TextureRenderer<TextureTarget = wgpu::Texture> + ImageRenderer {}

impl<T> TextureImageRenderer for T where
    T: TextureRenderer<TextureTarget = wgpu::Texture> + ImageRenderer
{
}

trait TextureViewImageRenderer:
    TextureRenderer<TextureTarget = TextureViewTarget> + ImageRenderer
{
}

impl<T> TextureViewImageRenderer for T where
    T: TextureRenderer<TextureTarget = TextureViewTarget> + ImageRenderer
{
}

struct TextureAndImageRenderer<T, I> {
    texture: T,
    image: I,
}

impl<T, I> TextureAndImageRenderer<T, I> {
    fn new(texture: T, image: I) -> Self {
        Self { texture, image }
    }
}

impl<T, I> TextureRenderer for TextureAndImageRenderer<T, I>
where
    T: TextureRenderer<TextureTarget = wgpu::Texture>,
{
    type TextureTarget = wgpu::Texture;

    fn render_source_to_texture(
        &mut self,
        source: &mut dyn RenderSource,
        target: Self::TextureTarget,
    ) -> Result<(), imaging::TextureRendererError> {
        self.texture.render_source_to_texture(source, target)
    }
}

impl<T, I> ImageRenderer for TextureAndImageRenderer<T, I>
where
    I: ImageRenderer,
{
    fn render_source_into(
        &mut self,
        source: &mut dyn RenderSource,
        target: ImageBufferTarget<'_>,
    ) -> Result<(), imaging::ImageRendererError> {
        self.image.render_source_into(source, target)
    }
}

struct TextureViewAndImageRenderer<T, I> {
    texture: T,
    image: I,
}

impl<T, I> TextureViewAndImageRenderer<T, I> {
    fn new(texture: T, image: I) -> Self {
        Self { texture, image }
    }
}

impl<T, I> TextureRenderer for TextureViewAndImageRenderer<T, I>
where
    T: TextureRenderer<TextureTarget = TextureViewTarget>,
{
    type TextureTarget = TextureViewTarget;

    fn render_source_to_texture(
        &mut self,
        source: &mut dyn RenderSource,
        target: Self::TextureTarget,
    ) -> Result<(), imaging::TextureRendererError> {
        self.texture.render_source_to_texture(source, target)
    }
}

impl<T, I> ImageRenderer for TextureViewAndImageRenderer<T, I>
where
    I: ImageRenderer,
{
    fn render_source_into(
        &mut self,
        source: &mut dyn RenderSource,
        target: ImageBufferTarget<'_>,
    ) -> Result<(), imaging::ImageRendererError> {
        self.image.render_source_into(source, target)
    }
}

enum AcceptedRenderer {
    Image {
        backend: Box<dyn ImageRenderer>,
        name: &'static str,
    },
    Texture {
        backend: Box<dyn TextureImageRenderer>,
        name: &'static str,
    },
    TextureView {
        backend: Box<dyn TextureViewImageRenderer>,
        name: &'static str,
    },
}

enum GpuAcceptedRenderer {
    Texture {
        backend: Box<dyn TextureImageRenderer>,
        name: &'static str,
    },
    TextureView {
        backend: Box<dyn TextureViewImageRenderer>,
        name: &'static str,
    },
}

impl NewRendererCx {
    pub(crate) fn build(
        chooser: &Arc<dyn Fn(NewRendererCx) -> WindowBackend + Send + Sync>,
        window: Arc<dyn Window>,
        gpu_resources: Option<GpuResources>,
        surface: Option<wgpu::Surface<'static>>,
        transparent: bool,
        scale: f64,
        size: Size,
    ) -> WindowBackend {
        chooser(Self {
            window,
            gpu_resources,
            surface,
            transparent,
            size,
            scale,
        })
    }

    #[allow(
        unreachable_code,
        dead_code,
        reason = "This CPU window path may be unused when no CPU renderer is enabled in the current build."
    )]
    pub(crate) fn build_cpu(
        chooser: &Arc<dyn Fn(NewRendererCx) -> WindowBackend + Send + Sync>,
        window: Arc<dyn Window>,
        scale: f64,
        size: Size,
    ) -> WindowBackend {
        chooser(Self {
            window,
            gpu_resources: None,
            surface: None,
            transparent: false,
            size,
            scale,
        })
    }
}

struct NullRenderer;

impl PaintSink for NullRenderer {
    fn push_clip(&mut self, _clip: ClipRef<'_>) {}

    fn pop_clip(&mut self) {}

    fn push_group(&mut self, _group: GroupRef<'_>) {}

    fn pop_group(&mut self) {}

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

#[allow(
    dead_code,
    reason = "CPU window targets may be unused when no CPU renderer is enabled in the current build."
)]
struct CpuWindowTarget<W> {
    surface: softbuffer::Surface<W, W>,
    width: u32,
    height: u32,
}

struct GpuWindowTarget {
    device: wgpu::Device,
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

#[allow(
    dead_code,
    reason = "CPU window backend factories may be unused when no CPU renderer is enabled in the current build."
)]
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

pub trait WindowRenderer {
    fn resize(&mut self, width: u32, height: u32);
    fn render(&mut self, size: Size, source: &mut dyn RenderSource) -> PaintTiming;
    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput;
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

    fn render(&mut self, _size: Size, source: &mut dyn RenderSource) -> PaintTiming {
        source.paint_into(&mut self.renderer);
        PaintTiming::default()
    }

    fn capture(&mut self, _size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        source.paint_into(&mut self.renderer);
        CaptureOutput::default()
    }

    fn debug_info(&mut self) -> String {
        "Uninitialized".to_string()
    }
}

#[allow(
    dead_code,
    reason = "CPU image presentation may be unused when no CPU renderer is enabled in the current build."
)]
struct ImageWindowRenderer {
    backend: Box<dyn ImageRenderer>,
    name: &'static str,
    target: CpuWindowTarget<Arc<dyn Window>>,
    scratch: RgbaImage,
}

struct TargetGpuWindowRenderer {
    backend: GpuAcceptedRenderer,
    target: GpuWindowTarget,
}

impl GpuWindowTarget {
    fn new(
        gpu_resources: &GpuResources,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        transparent: bool,
        preferred_texture_format: Option<wgpu::TextureFormat>,
    ) -> Result<Self, String> {
        let latency = match gpu_resources.adapter.get_info().backend {
            wgpu::Backend::Vulkan => 2,
            _ => 1,
        };
        let texture_format = match preferred_texture_format {
            Some(texture_format) => texture_format,
            None => surface
                .get_capabilities(&gpu_resources.adapter)
                .formats
                .first()
                .copied()
                .ok_or_else(|| "GPU surface reported no supported texture formats".to_string())?,
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
            surface,
            config,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn gpu_surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }
}

#[allow(
    dead_code,
    reason = "CPU window targets may be unused when no CPU renderer is enabled in the current build."
)]
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

    fn present_rgba(&mut self, image: &RgbaImage) -> PresentTiming {
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
            let src = &image.data[row_start * 4..(row_start + width) * 4];
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
}

impl WindowRenderer for TargetGpuWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, size: Size, source: &mut dyn RenderSource) -> PaintTiming {
        let start = Instant::now();
        let acquire_start = start;
        let surface_texture = self
            .target
            .surface
            .get_current_texture()
            .expect("failed to acquire surface texture");
        let acquire_surface = acquire_start.elapsed();
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let prepare_start = Instant::now();
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        match &mut self.backend {
            GpuAcceptedRenderer::Texture { backend, .. } => backend
                .render_source_to_texture(source, surface_texture.texture.clone())
                .expect("failed to render gpu target"),
            GpuAcceptedRenderer::TextureView { backend, .. } => backend
                .render_source_to_texture(
                    source,
                    TextureViewTarget::new(&texture_view, width, height),
                )
                .expect("failed to render gpu target"),
        }
        let prepare = Duration::ZERO;
        let scene = prepare_start.elapsed();
        let finalize = Duration::ZERO;
        let present_call_start = Instant::now();
        surface_texture.present();
        let present_call = present_call_start.elapsed();
        PaintTiming {
            presented: true,
            total: start.elapsed(),
            prepare,
            scene,
            finalize,
            present: PresentTiming {
                total: start.elapsed(),
                acquire_surface,
                compose: scene + finalize,
                submit: Duration::ZERO,
                present_call,
            },
            ..Default::default()
        }
    }

    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        let total_start = Instant::now();
        let scene_start = total_start;
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let mut image = RgbaImage::new(width, height);
        let rendered = match &mut self.backend {
            GpuAcceptedRenderer::Texture { backend, .. } => backend
                .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image))
                .is_ok(),
            GpuAcceptedRenderer::TextureView { backend, .. } => backend
                .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image))
                .is_ok(),
        };
        let scene = scene_start.elapsed();
        CaptureOutput {
            image: rendered.then(|| rgba_image_into_image_data(image)),
            timing: CaptureTiming {
                total: total_start.elapsed(),
                scene,
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        match &self.backend {
            GpuAcceptedRenderer::Texture { name, .. } => format!("Renderer: {name}"),
            GpuAcceptedRenderer::TextureView { name, .. } => format!("Renderer: {name}"),
        }
    }

    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        Some(self.target.gpu_surface())
    }
}

#[allow(
    dead_code,
    reason = "CPU image rendering may be unused when no CPU renderer is enabled in the current build."
)]
impl WindowRenderer for ImageWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, size: Size, source: &mut dyn RenderSource) -> PaintTiming {
        let total_start = Instant::now();
        let scene_start = total_start;
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        self.scratch.resize(width, height);
        let rendered = self
            .backend
            .render_source_into(
                source,
                ImageBufferTarget::from_rgba_image(&mut self.scratch),
            )
            .is_ok();
        let scene = scene_start.elapsed();
        let present = if rendered {
            self.target.present_rgba(&self.scratch)
        } else {
            PresentTiming::default()
        };
        PaintTiming {
            presented: rendered,
            total: total_start.elapsed(),
            scene,
            present,
            ..Default::default()
        }
    }

    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        let total_start = Instant::now();
        let scene_start = total_start;
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let mut image = RgbaImage::new(width, height);
        let rendered = self
            .backend
            .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image))
            .is_ok();
        let scene = scene_start.elapsed();
        CaptureOutput {
            image: rendered.then(|| rgba_image_into_image_data(image)),
            timing: CaptureTiming {
                total: total_start.elapsed(),
                scene,
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        format!("Renderer: {}", self.name)
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

pub(crate) fn uninitialized_backend() -> WindowBackend {
    Box::new(NullWindowBackend {
        renderer: NullRenderer,
    })
}

fn choose_default_renderer(cx: NewRendererCx) -> Result<WindowBackend, String> {
    #[allow(
        unreachable_code,
        reason = "Some feature combinations end the chooser earlier with a concrete fallback renderer."
    )]
    {
        #[cfg(feature = "vello")]
        if let Some(gpu) = cx.gpu()
            && let Some(surface_format) = gpu.surface_formats().iter().copied().find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
                )
            })
        {
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            return cx.into_renderer(
                AcceptedRenderer::TextureView {
                    backend: Box::new(TextureViewAndImageRenderer::new(
                        imaging_vello::VelloTargetRenderer::new(device.clone(), queue.clone())
                            .map_err(|err| err.to_string())?,
                        imaging_vello::VelloRenderer::new(device, queue)
                            .map_err(|err| err.to_string())?,
                    )),
                    name: "Vello GPU",
                },
                Some(surface_format),
            );
        }

        #[cfg(feature = "vger")]
        if let Some(gpu) = cx.gpu()
            && let Some(surface_format) = gpu.surface_formats().iter().copied().find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm
                        | wgpu::TextureFormat::Rgba8UnormSrgb
                        | wgpu::TextureFormat::Bgra8Unorm
                        | wgpu::TextureFormat::Bgra8UnormSrgb
                )
            })
        {
            let adapter = gpu.gpu_resources.adapter.clone();
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let width = cx.size.width.max(1.0) as u32;
            let height = cx.size.height.max(1.0) as u32;
            return cx.into_renderer(
                AcceptedRenderer::TextureView {
                    backend: Box::new(
                        floem_vger_renderer::VgerRenderer::new(
                            adapter,
                            device,
                            queue,
                            width,
                            height,
                            surface_format,
                        )
                        .map_err(|err| err.to_string())?,
                    ),
                    name: "Vger GPU",
                },
                Some(surface_format),
            );
        }

        #[cfg(feature = "skia")]
        if let Some(gpu) = cx.gpu()
            && let Some(surface_format) = gpu.surface_formats().iter().copied().find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm
                        // | wgpu::TextureFormat::Rgba8UnormSrgb
                        | wgpu::TextureFormat::Bgra8Unorm // | wgpu::TextureFormat::Bgra8UnormSrgb
                                                          // | wgpu::TextureFormat::Rgb10a2Unorm
                                                          // | wgpu::TextureFormat::Rgba16Unorm
                                                          // | wgpu::TextureFormat::Rgba16Float
                )
            })
        {
            let adapter = gpu.gpu_resources.adapter.clone();
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            return cx.into_renderer(
                AcceptedRenderer::Texture {
                    backend: Box::new(TextureAndImageRenderer::new(
                        imaging_skia::SkiaGpuTargetRenderer::new(
                            adapter.clone(),
                            device.clone(),
                            queue.clone(),
                        )
                        .map_err(|err| err.to_string())?,
                        imaging_skia::SkiaGpuRenderer::new(adapter, device, queue)
                            .map_err(|err| err.to_string())?,
                    )),
                    name: "Skia GPU",
                },
                Some(surface_format),
            );
        }

        #[cfg(feature = "vello-hybrid")]
        if let Some(gpu) = cx.gpu()
            && let Some(surface_format) = gpu.surface_formats().iter().copied().find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
                )
            })
        {
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            return cx.into_renderer(
                AcceptedRenderer::TextureView {
                    backend: Box::new(TextureViewAndImageRenderer::new(
                        imaging_vello_hybrid::VelloHybridTargetRenderer::new(
                            device.clone(),
                            queue.clone(),
                        ),
                        imaging_vello_hybrid::VelloHybridRenderer::new(device, queue),
                    )),
                    name: "Vello Hybrid GPU",
                },
                Some(surface_format),
            );
        }

        #[cfg(feature = "vello-cpu")]
        {
            let width = u16::try_from(cx.size.width.max(1.0) as u32)
                .map_err(|_| "width exceeds vello cpu limit".to_string())?;
            let height = u16::try_from(cx.size.height.max(1.0) as u32)
                .map_err(|_| "height exceeds vello cpu limit".to_string())?;
            return cx.into_renderer(
                AcceptedRenderer::Image {
                    backend: Box::new(imaging_vello_cpu::VelloCpuRenderer::new(width, height)),
                    name: "Vello CPU",
                },
                None,
            );
        }

        #[cfg(feature = "skia-cpu")]
        {
            return cx.into_renderer(
                AcceptedRenderer::Image {
                    backend: Box::new(imaging_skia::SkiaCpuCopyRenderer::new()),
                    name: "Skia CPU",
                },
                None,
            );
        }

        #[cfg(feature = "tiny-skia")]
        {
            let width = cx.size.width.max(1.0) as u32;
            let height = cx.size.height.max(1.0) as u32;
            return cx.into_renderer(
                AcceptedRenderer::Image {
                    backend: Box::new(
                        imaging_tiny_skia::TinySkiaCpuCopyRenderer::new_with_size(width, height)
                            .map_err(|err| err.to_string())?,
                    ),
                    name: "Tiny Skia CPU",
                },
                None,
            );
        }

        #[cfg(feature = "vello-cpu")]
        {
            let width = u16::try_from(cx.size.width.max(1.0) as u32)
                .map_err(|_| "width exceeds vello_cpu limit".to_string())?;
            let height = u16::try_from(cx.size.height.max(1.0) as u32)
                .map_err(|_| "height exceeds vello_cpu limit".to_string())?;
            return cx.into_renderer(
                AcceptedRenderer::Image {
                    backend: Box::new(imaging_vello_cpu::VelloCpuRenderer::new(width, height)),
                    name: "Vello CPU",
                },
                None,
            );
        }

        Err("no renderer available for this window target".to_string())
    }
}

pub(crate) fn default_renderer() -> Arc<dyn Fn(NewRendererCx) -> WindowBackend + Send + Sync> {
    Arc::new(|cx| choose_default_renderer(cx).expect("create renderer"))
}
