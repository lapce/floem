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

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
pub(crate) const HAS_GPU_ACTIVE_RENDERER: bool = true;
#[cfg(not(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
)))]
pub(crate) const HAS_GPU_ACTIVE_RENDERER: bool = false;

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
))]
pub(crate) const HAS_IMMEDIATE_RENDERER: bool = true;
#[cfg(not(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
)))]
pub(crate) const HAS_IMMEDIATE_RENDERER: bool = false;

use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::{
    BeginFrame, CustomRenderer, DisplayCommandExt, RasterizerOutput, RenderCore, Renderer,
    SceneRenderer, TargetRenderer,
};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::ImageData;
use peniko::kurbo::Size;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use wgpu::util::TextureBlitter;
use winit::window::Window;

type CpuTargetRenderFn = dyn for<'a> FnMut(
    BeginFrame,
    floem_renderer::CpuBufferTarget<'a>,
    &mut dyn FnMut(&mut dyn SceneRenderer),
) -> Result<(), String>;

struct NullRenderer;

impl RenderCore for NullRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        f(self)
    }

    fn finish(&mut self) {}

    fn readback(&mut self) -> Option<RasterizerOutput> {
        None
    }
}

impl Renderer for NullRenderer {
    type Target = RasterizerOutput;

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
    CopyGpu(Box<dyn Renderer<Target = wgpu::TextureView>>),
    CopyCpu(Box<dyn Renderer<Target = ImageData>>),
}

pub(crate) trait WindowRenderer {
    fn resize(&mut self, width: u32, height: u32);
    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer));
    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        None
    }
}

struct CpuDirectWindowRenderer<R> {
    target: CpuWindowTarget<Arc<dyn Window>>,
    _marker: std::marker::PhantomData<R>,
}

struct GpuCopyWindowRenderer<R> {
    renderer: R,
    target: GpuWindowTarget,
}

struct CpuCopyWindowRenderer<R> {
    renderer: R,
    target: CpuWindowTarget<Arc<dyn Window>>,
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
}

#[cfg(feature = "active-vello-cpu")]
impl WindowRenderer
    for CpuDirectWindowRenderer<floem_vello_cpu_renderer::VelloCpuTargetRenderer<'static>>
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                floem_vello_cpu_renderer::VelloCpuTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
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
        self.target.with_cpu_target(&mut |target| {
            let mut renderer = floem_skia_renderer::SkiaCpuTargetRenderer::create(begin, target)
                .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
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
        self.target.with_cpu_target(&mut |target| {
            let mut renderer =
                floem_tiny_skia_renderer::TinySkiaTargetRenderer::create(begin, target)
                    .expect("failed to create cpu target renderer");
            paint(&mut renderer);
            renderer.finish();
        });
    }
}

impl<R> WindowRenderer for GpuCopyWindowRenderer<R>
where
    R: Renderer<Target = wgpu::TextureView> + SceneRenderer + 'static,
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.renderer.set_size(begin);
        self.renderer.reset();
        paint(&mut self.renderer);
        self.renderer.finish();
        if let Some(output) = self.renderer.read_target() {
            self.target.present(&output);
        }
    }

    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        Some(self.target.gpu_surface())
    }
}

impl<R> WindowRenderer for CpuCopyWindowRenderer<R>
where
    R: Renderer<Target = ImageData> + SceneRenderer + 'static,
{
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(&mut self, begin: BeginFrame, paint: &mut dyn FnMut(&mut dyn SceneRenderer)) {
        self.renderer.set_size(begin);
        self.renderer.reset();
        paint(&mut self.renderer);
        self.renderer.finish();
        if let Some(output) = self.renderer.read_target() {
            self.target.present(&output);
        }
    }
}

fn env_flag_requested(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| value.as_str() == "1")
}

fn force_cpu_requested() -> bool {
    env_flag_requested("FLOEM_FORCE_CPU") || env_flag_requested("FLOEM_FORCE_TINY_SKIA")
}

pub(crate) type WindowBackend = Box<dyn WindowRenderer>;

pub(crate) fn uninitialized_rasterizer() -> Box<dyn crate::paint::WindowRasterizer> {
    Box::new(NullRenderer)
}

#[cfg(feature = "fallback-vello-cpu")]
type CpuFallbackRenderer = floem_vello_cpu_renderer::VelloCpuRenderer;
#[cfg(feature = "fallback-tiny-skia")]
type CpuFallbackRenderer = floem_tiny_skia_renderer::TinySkiaRenderer;

fn boxed_rasterizer(
    renderer: impl crate::paint::WindowRasterizer + 'static,
) -> Box<dyn crate::paint::WindowRasterizer> {
    Box::new(renderer)
}

pub(crate) fn new(
    window: Arc<dyn Window>,
    gpu_resources: GpuResources,
    surface: wgpu::Surface<'static>,
    _transparent: bool,
    scale: f64,
    size: Size,
    font_embolden: f32,
) -> (Box<dyn crate::paint::WindowRasterizer>, WindowBackend) {
    let size = Size::new(size.width.max(1.0), size.height.max(1.0));
    let width = size.width as u32;
    let height = size.height as u32;
    let texture_format =
        choose_surface_texture_format(&surface, &gpu_resources).expect("surface format");

    if !force_cpu_requested() {
        #[cfg(feature = "active-vello")]
        {
            let renderer = floem_vello_renderer::VelloRenderer::new(
                gpu_resources.clone(),
                width,
                height,
                texture_format,
                scale,
                font_embolden,
            )
            .map_err(|err| err.to_string())
            .expect("create vello renderer");
            let window_backend: WindowBackend = Box::new(GpuCopyWindowRenderer {
                renderer: floem_vello_renderer::VelloRenderer::new(
                    gpu_resources.clone(),
                    width,
                    height,
                    texture_format,
                    scale,
                    font_embolden,
                )
                .map_err(|err| err.to_string())
                .expect("create vello window renderer"),
                target: GpuWindowTarget::new(&gpu_resources, surface, width, height, _transparent)
                    .expect("create gpu target"),
            });
            return (boxed_rasterizer(renderer), window_backend);
        }

        #[cfg(feature = "active-vger")]
        {
            let renderer = floem_vger_renderer::VgerRenderer::new(
                gpu_resources.clone(),
                width,
                height,
                texture_format,
                scale,
                font_embolden,
            )
            .map_err(|err| err.to_string())
            .expect("create vger renderer");
            let window_backend: WindowBackend = Box::new(GpuCopyWindowRenderer {
                renderer: floem_vger_renderer::VgerRenderer::new(
                    gpu_resources.clone(),
                    width,
                    height,
                    texture_format,
                    scale,
                    font_embolden,
                )
                .map_err(|err| err.to_string())
                .expect("create vger window renderer"),
                target: GpuWindowTarget::new(&gpu_resources, surface, width, height, _transparent)
                    .expect("create gpu target"),
            });
            return (boxed_rasterizer(renderer), window_backend);
        }

        #[cfg(feature = "active-skia")]
        {
            let renderer = floem_skia_renderer::SkiaRenderer::new(
                gpu_resources.clone(),
                width,
                height,
                texture_format,
                scale,
                font_embolden,
            )
            .map_err(|err| err.to_string())
            .expect("create skia renderer");
            let window_backend: WindowBackend = Box::new(GpuCopyWindowRenderer {
                renderer: floem_skia_renderer::SkiaRenderer::new(
                    gpu_resources.clone(),
                    width,
                    height,
                    texture_format,
                    scale,
                    font_embolden,
                )
                .map_err(|err| err.to_string())
                .expect("create skia window renderer"),
                target: GpuWindowTarget::new(&gpu_resources, surface, width, height, _transparent)
                    .expect("create gpu target"),
            });
            return (boxed_rasterizer(renderer), window_backend);
        }
    }

    let window_backend: WindowBackend = Box::new(CpuCopyWindowRenderer {
        renderer: CpuFallbackRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())
            .expect("create cpu fallback renderer"),
        target: CpuWindowTarget::new(window, width, height).expect("create cpu target"),
    });
    (
        boxed_rasterizer(
            CpuFallbackRenderer::new(width, height, scale, font_embolden)
                .map_err(|err| err.to_string())
                .expect("create cpu fallback renderer"),
        ),
        window_backend,
    )
}

#[allow(unreachable_code)]
pub(crate) fn new_cpu(
    window: Arc<dyn Window>,
    scale: f64,
    size: Size,
    font_embolden: f32,
) -> (Box<dyn crate::paint::WindowRasterizer>, WindowBackend) {
    let size = Size::new(size.width.max(1.0), size.height.max(1.0));
    let width = size.width as u32;
    let height = size.height as u32;

    #[cfg(feature = "active-vello-hybrid")]
    {
        return (
            boxed_rasterizer(
                floem_vello_hybrid_renderer::VelloHybridRenderer::new(
                    width,
                    height,
                    scale,
                    font_embolden,
                )
                .map_err(|err| err.to_string())
                .expect("create vello hybrid renderer"),
            ),
            Box::new(CpuDirectWindowRenderer::<
                floem_vello_hybrid_renderer::VelloHybridTargetRenderer<'static>,
            > {
                target: CpuWindowTarget::new(window, width, height).expect("create cpu target"),
                _marker: std::marker::PhantomData,
            }),
        );
    }

    #[cfg(feature = "active-vello-cpu")]
    {
        return (
            boxed_rasterizer(
                floem_vello_cpu_renderer::VelloCpuRenderer::new(
                    width,
                    height,
                    scale,
                    font_embolden,
                )
                .map_err(|err| err.to_string())
                .expect("create vello cpu renderer"),
            ),
            Box::new(CpuDirectWindowRenderer::<
                floem_vello_cpu_renderer::VelloCpuTargetRenderer<'static>,
            > {
                target: CpuWindowTarget::new(window, width, height).expect("create cpu target"),
                _marker: std::marker::PhantomData,
            }),
        );
    }

    #[cfg(feature = "active-skia-cpu")]
    {
        return (
            boxed_rasterizer(
                floem_skia_renderer::SkiaCpuRenderer::new(width, height, scale, font_embolden)
                    .map_err(|err| err.to_string())
                    .expect("create skia cpu renderer"),
            ),
            Box::new(CpuDirectWindowRenderer::<
                floem_skia_renderer::SkiaCpuTargetRenderer<'static>,
            > {
                target: CpuWindowTarget::new(window, width, height).expect("create cpu target"),
                _marker: std::marker::PhantomData,
            }),
        );
    }

    #[cfg(feature = "active-tiny-skia")]
    {
        return (
            boxed_rasterizer(
                floem_tiny_skia_renderer::TinySkiaRenderer::new(
                    width,
                    height,
                    scale,
                    font_embolden,
                )
                .map_err(|err| err.to_string())
                .expect("create tiny-skia renderer"),
            ),
            Box::new(CpuDirectWindowRenderer::<
                floem_tiny_skia_renderer::TinySkiaTargetRenderer<'static>,
            > {
                target: CpuWindowTarget::new(window, width, height).expect("create cpu target"),
                _marker: std::marker::PhantomData,
            }),
        );
    }

    unreachable!("immediate renderer requested without an enabled immediate renderer")
}
