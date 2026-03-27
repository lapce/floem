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

#[cfg(not(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia",
)))]
compile_error!("Enable exactly one active renderer feature.");
#[cfg(any(
    all(feature = "active-vello", feature = "active-vger"),
    all(feature = "active-vello", feature = "active-vello-hybrid"),
    all(feature = "active-vello", feature = "active-vello-cpu"),
    all(feature = "active-vello", feature = "active-skia"),
    all(feature = "active-vello", feature = "active-skia-cpu"),
    all(feature = "active-vello", feature = "active-tiny-skia"),
    all(feature = "active-vger", feature = "active-vello-hybrid"),
    all(feature = "active-vger", feature = "active-vello-cpu"),
    all(feature = "active-vger", feature = "active-skia"),
    all(feature = "active-vger", feature = "active-skia-cpu"),
    all(feature = "active-vger", feature = "active-tiny-skia"),
    all(feature = "active-vello-hybrid", feature = "active-vello-cpu"),
    all(feature = "active-vello-hybrid", feature = "active-skia"),
    all(feature = "active-vello-hybrid", feature = "active-skia-cpu"),
    all(feature = "active-vello-hybrid", feature = "active-tiny-skia"),
    all(feature = "active-vello-cpu", feature = "active-skia"),
    all(feature = "active-vello-cpu", feature = "active-skia-cpu"),
    all(feature = "active-vello-cpu", feature = "active-tiny-skia"),
    all(feature = "active-skia", feature = "active-tiny-skia"),
    all(feature = "active-skia", feature = "active-skia-cpu"),
    all(feature = "active-skia-cpu", feature = "active-tiny-skia"),
))]
compile_error!("Enable only one active renderer feature.");
#[cfg(not(any(feature = "fallback-vello-cpu", feature = "fallback-tiny-skia")))]
compile_error!("Enable exactly one CPU fallback renderer feature.");
#[cfg(all(feature = "fallback-vello-cpu", feature = "fallback-tiny-skia"))]
compile_error!("Enable only one CPU fallback renderer feature.");

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
use floem_renderer::rasterizer::{CpuOrRasterizer, GpuOrRasterizer};
use floem_renderer::{
    BeginFrame, CustomRasterizer, DisplayCommandExt, FinishMode, RasterCore, Rasterizer,
    RasterizerOutput, SceneRasterizer,
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

struct NullRasterizer;

impl RasterCore for NullRasterizer {
    fn with_paint_sink(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        f(self)
    }

    fn finish(&mut self) {}

    fn readback(&mut self) -> Option<RasterizerOutput> {
        None
    }
}

impl Rasterizer for NullRasterizer {
    fn begin(&mut self, _frame: BeginFrame) {}
}

impl PaintSink for NullRasterizer {
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

impl CustomPaintSink<DisplayCommandExt> for NullRasterizer {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}

impl CustomRasterizer for NullRasterizer {
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

pub(crate) struct CpuImagePresenter<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
}

pub(crate) struct GpuWindowPresenter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: TextureBlitter,
    frame_texture: Option<wgpu::SurfaceTexture>,
}

pub(crate) enum FrameTarget<'a> {
    Gpu {
        begin: BeginFrame,
        target: wgpu::TextureView,
    },
    Cpu {
        begin: BeginFrame,
        target: floem_renderer::CpuBufferTarget<'a>,
    },
}

pub(crate) enum WindowBackend {
    Gpu {
        presenter: GpuWindowPresenter,
        rasterizer: GpuOrRasterizer,
    },
    Cpu {
        presenter: CpuImagePresenter<Arc<dyn Window>>,
        rasterizer: CpuOrRasterizer,
    },
}

impl WindowBackend {
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::Gpu { presenter, .. } => presenter.resize(width, height),
            Self::Cpu { presenter, .. } => presenter.resize(width, height),
        }
    }

    pub(crate) fn rasterizer(&self) -> &(dyn SceneRasterizer + '_) {
        match self {
            Self::Gpu { rasterizer, .. } => rasterizer.as_ref(),
            Self::Cpu { rasterizer, .. } => rasterizer.as_ref(),
        }
    }

    pub(crate) fn rasterizer_mut(&mut self) -> &mut (dyn SceneRasterizer + '_) {
        match self {
            Self::Gpu { rasterizer, .. } => rasterizer.as_mut(),
            Self::Cpu { rasterizer, .. } => rasterizer.as_mut(),
        }
    }

    pub(crate) fn present(&mut self, output: &RasterizerOutput) {
        match (self, output) {
            (Self::Gpu { presenter, .. }, RasterizerOutput::GpuTexture(output)) => {
                presenter.present(output)
            }
            (Self::Cpu { presenter, .. }, RasterizerOutput::Image(image)) => {
                presenter.present(image)
            }
            _ => {}
        }
    }

    pub(crate) fn present_rasterizer(&mut self, rasterizer: &mut dyn SceneRasterizer) -> bool {
        match self {
            Self::Gpu { presenter, .. } => presenter.present_prepared(),
            Self::Cpu { presenter, .. } => presenter.present_rasterizer(rasterizer),
        }
    }
}

pub(crate) struct RasterizerInit {
    pub(crate) window_backend: WindowBackend,
}

pub(crate) fn uninitialized_rasterizer() -> crate::paint::WindowRasterizer {
    crate::paint::WindowRasterizer::Rasterizer(Box::new(NullRasterizer))
}

impl GpuWindowPresenter {
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
            frame_texture: None,
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

    #[cfg(feature = "active-skia")]
    fn prepare_skia_target(&mut self, rasterizer: &mut floem_skia_renderer::SkiaRenderer) -> bool {
        let surface_texture = self
            .surface
            .get_current_texture()
            .expect("failed to acquire surface texture");
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        floem_renderer::RasterTarget::set_target(rasterizer, view)
            .expect("failed to retarget rasterizer");
        self.frame_texture = Some(surface_texture);
        true
    }

    fn present_prepared(&mut self) -> bool {
        if let Some(surface_texture) = self.frame_texture.take() {
            surface_texture.present();
            true
        } else {
            false
        }
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

impl<W> CpuImagePresenter<W>
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
        Ok(Self { context, surface })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.surface
            .resize(
                NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .expect("failed to resize surface");
    }

    fn present_rasterizer(&mut self, _rasterizer: &mut dyn SceneRasterizer) -> bool {
        false
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
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn env_flag_requested(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| value.as_str() == "1")
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn force_cpu_requested() -> bool {
    env_flag_requested("FLOEM_FORCE_CPU") || env_flag_requested("FLOEM_FORCE_TINY_SKIA")
}

#[cfg(feature = "active-vello")]
type ActiveGpuRenderer = floem_vello_renderer::VelloRenderer;
#[cfg(feature = "active-vello")]
const ACTIVE_GPU_RENDERER_NAME: &str = "VelloRenderer";

#[cfg(feature = "active-vger")]
type ActiveGpuRenderer = floem_vger_renderer::VgerRenderer;
#[cfg(feature = "active-vger")]
const ACTIVE_GPU_RENDERER_NAME: &str = "VgerRenderer";

#[cfg(feature = "active-skia")]
type ActiveGpuRenderer = floem_skia_renderer::SkiaRenderer;
#[cfg(feature = "active-skia")]
const ACTIVE_GPU_RENDERER_NAME: &str = "SkiaRenderer";

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn new_active_gpu_rasterizer(
    gpu_resources: GpuResources,
    width: u32,
    height: u32,
    texture_format: wgpu::TextureFormat,
    scale: f64,
    font_embolden: f32,
) -> Result<Box<dyn SceneRasterizer>, String> {
    Ok(Box::new(
        ActiveGpuRenderer::new(
            gpu_resources,
            width,
            height,
            texture_format,
            scale,
            font_embolden,
        )
        .map_err(|err| err.to_string())?,
    ))
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn try_new_active_gpu_backend(
    gpu_resources: GpuResources,
    surface: wgpu::Surface<'static>,
    width: u32,
    height: u32,
    transparent: bool,
    scale: f64,
    font_embolden: f32,
) -> Result<RasterizerInit, String> {
    let texture_format = choose_surface_texture_format(&surface, &gpu_resources)?;

    let presenter = GpuWindowPresenter::new(&gpu_resources, surface, width, height, transparent)?;
    let rasterizer = new_active_gpu_rasterizer(
        gpu_resources,
        width,
        height,
        texture_format,
        scale,
        font_embolden,
    )?;
    Ok(RasterizerInit {
        window_backend: WindowBackend::Gpu {
            presenter,
            rasterizer,
        },
    })
}

#[cfg(feature = "active-vello-hybrid")]
type ActiveImmediateRenderer = floem_vello_hybrid_renderer::VelloHybridRenderer;
#[cfg(feature = "active-vello-hybrid")]
const ACTIVE_IMMEDIATE_RENDERER_NAME: &str = "VelloHybridRenderer";

#[cfg(feature = "active-vello-cpu")]
type ActiveImmediateRenderer = floem_vello_cpu_renderer::VelloCpuRenderer;
#[cfg(feature = "active-vello-cpu")]
const ACTIVE_IMMEDIATE_RENDERER_NAME: &str = "VelloCpuRenderer";

#[cfg(feature = "active-skia-cpu")]
type ActiveImmediateRenderer = floem_skia_renderer::SkiaCpuRenderer;
#[cfg(feature = "active-skia-cpu")]
const ACTIVE_IMMEDIATE_RENDERER_NAME: &str = "SkiaCpuRenderer";

#[cfg(feature = "active-tiny-skia")]
type ActiveImmediateRenderer = floem_tiny_skia_renderer::TinySkiaRenderer;
#[cfg(feature = "active-tiny-skia")]
const ACTIVE_IMMEDIATE_RENDERER_NAME: &str = "TinySkiaRenderer";

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
))]
fn new_active_immediate_rasterizer(
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<Box<dyn SceneRasterizer>, String> {
    Ok(Box::new(
        ActiveImmediateRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    ))
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
))]
fn try_new_active_immediate_backend(
    window: Arc<dyn Window>,
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<RasterizerInit, String> {
    let presenter = CpuImagePresenter::new(window, width, height)?;
    let rasterizer = new_active_immediate_rasterizer(width, height, scale, font_embolden)?;
    Ok(RasterizerInit {
        window_backend: WindowBackend::Cpu {
            presenter,
            rasterizer,
        },
    })
}

#[cfg(feature = "fallback-vello-cpu")]
type CpuFallbackRenderer = floem_vello_cpu_renderer::VelloCpuRenderer;

#[cfg(feature = "fallback-tiny-skia")]
type CpuFallbackRenderer = floem_tiny_skia_renderer::TinySkiaRenderer;

fn new_cpu_fallback_rasterizer(
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<Box<dyn SceneRasterizer>, String> {
    Ok(Box::new(
        CpuFallbackRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    ))
}

fn try_new_cpu_fallback(
    window: Arc<dyn Window>,
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<RasterizerInit, String> {
    let presenter = CpuImagePresenter::new(window, width, height)?;
    let rasterizer = new_cpu_fallback_rasterizer(width, height, scale, font_embolden)?;
    Ok(RasterizerInit {
        window_backend: WindowBackend::Cpu {
            presenter,
            rasterizer,
        },
    })
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
pub(crate) fn new(
    window: Arc<dyn Window>,
    gpu_resources: GpuResources,
    surface: wgpu::Surface<'static>,
    transparent: bool,
    scale: f64,
    size: Size,
    font_embolden: f32,
) -> RasterizerInit {
    let size = Size::new(size.width.max(1.0), size.height.max(1.0));
    let width = size.width as u32;
    let height = size.height as u32;
    let force_cpu = force_cpu_requested();

    let active_err = if !force_cpu {
        match try_new_active_gpu_backend(
            gpu_resources,
            surface,
            width,
            height,
            transparent,
            scale,
            font_embolden,
        ) {
            Ok(init) => return init,
            Err(err) => Some(err),
        }
    } else {
        None
    };

    let cpu_fallback_err = match try_new_cpu_fallback(window, width, height, scale, font_embolden) {
        Ok(init) => return init,
        Err(err) => err,
    };

    if !force_cpu {
        panic!(
            "Failed to create {active_name}: {active_err}\nFailed to create CPU fallback rasterizer: {cpu_fallback_err}",
            active_name = ACTIVE_GPU_RENDERER_NAME,
            active_err = active_err.unwrap()
        );
    } else {
        panic!("Failed to create CPU fallback rasterizer: {cpu_fallback_err}");
    }
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
))]
pub(crate) fn new_cpu(
    window: Arc<dyn Window>,
    scale: f64,
    size: Size,
    font_embolden: f32,
) -> RasterizerInit {
    let size = Size::new(size.width.max(1.0), size.height.max(1.0));
    let width = size.width as u32;
    let height = size.height as u32;

    let active_err =
        match try_new_active_immediate_backend(window.clone(), width, height, scale, font_embolden)
        {
            Ok(init) => return init,
            Err(err) => Some(err),
        };

    let cpu_fallback_err = match try_new_cpu_fallback(window, width, height, scale, font_embolden) {
        Ok(init) => return init,
        Err(err) => err,
    };

    panic!(
        "Failed to create {active_name}: {active_err}\nFailed to create CPU fallback rasterizer: {cpu_fallback_err}",
        active_name = ACTIVE_IMMEDIATE_RENDERER_NAME,
        active_err = active_err.unwrap()
    );
}
