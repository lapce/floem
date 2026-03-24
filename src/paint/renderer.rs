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
    feature = "active-tiny-skia",
)))]
compile_error!("Enable exactly one active renderer feature.");
#[cfg(any(
    all(feature = "active-vello", feature = "active-vger"),
    all(feature = "active-vello", feature = "active-vello-hybrid"),
    all(feature = "active-vello", feature = "active-vello-cpu"),
    all(feature = "active-vello", feature = "active-skia"),
    all(feature = "active-vello", feature = "active-tiny-skia"),
    all(feature = "active-vger", feature = "active-vello-hybrid"),
    all(feature = "active-vger", feature = "active-vello-cpu"),
    all(feature = "active-vger", feature = "active-skia"),
    all(feature = "active-vger", feature = "active-tiny-skia"),
    all(feature = "active-vello-hybrid", feature = "active-vello-cpu"),
    all(feature = "active-vello-hybrid", feature = "active-skia"),
    all(feature = "active-vello-hybrid", feature = "active-tiny-skia"),
    all(feature = "active-vello-cpu", feature = "active-skia"),
    all(feature = "active-vello-cpu", feature = "active-tiny-skia"),
    all(feature = "active-skia", feature = "active-tiny-skia"),
))]
compile_error!("Enable only one active renderer feature.");
#[cfg(not(any(feature = "fallback-vello-cpu", feature = "fallback-tiny-skia")))]
compile_error!("Enable exactly one CPU fallback renderer feature.");
#[cfg(all(feature = "fallback-vello-cpu", feature = "fallback-tiny-skia"))]
compile_error!("Enable only one CPU fallback renderer feature.");

#[cfg(any(feature = "active-vello", feature = "active-vger"))]
use floem_renderer::gpu_resources::GpuResources;
#[cfg(any(feature = "active-vello", feature = "active-vger"))]
use floem_renderer::{GpuTextureOutput, RenderOutput};
#[cfg(feature = "active-skia")]
use floem_skia_renderer::SkiaRenderer as ActiveRenderer;
#[cfg(feature = "active-tiny-skia")]
use floem_tiny_skia_renderer::TinySkiaRenderer as ActiveRenderer;
#[cfg(feature = "fallback-tiny-skia")]
use floem_tiny_skia_renderer::TinySkiaRenderer as CpuFallbackRenderer;
#[cfg(feature = "active-vello-cpu")]
use floem_vello_cpu_renderer::VelloCpuRenderer as ActiveRenderer;
#[cfg(feature = "fallback-vello-cpu")]
use floem_vello_cpu_renderer::VelloCpuRenderer as CpuFallbackRenderer;
#[cfg(feature = "active-vello-hybrid")]
use floem_vello_hybrid_renderer::VelloHybridRenderer as ActiveRenderer;
#[cfg(feature = "active-vello")]
use floem_vello_renderer::VelloRenderer as ActiveRenderer;
#[cfg(feature = "active-vger")]
use floem_vger_renderer::VgerRenderer as ActiveRenderer;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::ImageData;
use peniko::kurbo::Size;
#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
use softbuffer::{Context, Surface};
#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
use std::num::NonZeroU32;
#[cfg(any(feature = "active-vello", feature = "active-vger"))]
use wgpu::util::TextureBlitter;
use winit::window::Window;

use floem_renderer::DisplayCommandExt;

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
struct CpuImagePresenter<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
}

#[cfg(any(feature = "fallback-vello-cpu", feature = "fallback-tiny-skia"))]
struct CpuFallbackState {
    renderer: CpuFallbackRenderer,
    presenter: CpuImagePresenter<Arc<dyn Window>>,
    size: Size,
}

#[cfg(any(feature = "active-vello", feature = "active-vger"))]
struct ActiveState {
    renderer: ActiveRenderer,
    presenter: GpuWindowPresenter,
    size: Size,
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia",
    feature = "active-tiny-skia"
))]
struct ActiveState {
    renderer: ActiveRenderer,
    presenter: CpuImagePresenter<Arc<dyn Window>>,
    size: Size,
}

#[cfg(any(feature = "active-vello", feature = "active-vger"))]
struct GpuWindowPresenter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: TextureBlitter,
}

#[cfg(any(feature = "active-vello", feature = "active-vger"))]
impl GpuWindowPresenter {
    fn new(
        gpu_resources: &GpuResources,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let surface_caps = surface.get_capabilities(&gpu_resources.adapter);
        let texture_format = surface_caps
            .formats
            .into_iter()
            .find(|it| {
                matches!(
                    it,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .ok_or_else(|| "surface should support Rgba8Unorm or Bgra8Unorm".to_string())?;

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
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
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

    fn present(&mut self, output: &GpuTextureOutput) {
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
            .copy(&self.device, &mut encoder, &output.view, &output_view);
        self.queue.submit([encoder.finish()]);
        surface_texture.present();
    }
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
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

#[allow(clippy::large_enum_variant, private_interfaces)]
pub enum Renderer {
    #[cfg(any(
        feature = "active-vello",
        feature = "active-vger",
        feature = "active-vello-hybrid",
        feature = "active-vello-cpu",
        feature = "active-skia",
        feature = "active-tiny-skia"
    ))]
    Active(ActiveState),
    #[cfg(any(feature = "fallback-vello-cpu", feature = "fallback-tiny-skia"))]
    CpuFallback(CpuFallbackState),
    /// Uninitialized renderer, used to allow the renderer to be created lazily
    /// All operations on this renderer are no-ops
    Uninitialized { size: Size },
}

#[derive(Clone, Copy, Debug)]
pub struct BeginFrame {
    pub size: Size,
    pub scale: f64,
    pub font_embolden: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishMode {
    Present,
    Capture,
}

impl Renderer {
    #[cfg(feature = "active-vello")]
    const ACTIVE_RENDERER_NAME: &str = "VelloRenderer";
    #[cfg(feature = "active-vger")]
    const ACTIVE_RENDERER_NAME: &str = "VgerRenderer";
    #[cfg(feature = "active-vello-hybrid")]
    const ACTIVE_RENDERER_NAME: &str = "VelloHybridRenderer";
    #[cfg(feature = "active-vello-cpu")]
    const ACTIVE_RENDERER_NAME: &str = "VelloCpuRenderer";
    #[cfg(feature = "active-skia")]
    const ACTIVE_RENDERER_NAME: &str = "SkiaRenderer";
    #[cfg(feature = "active-tiny-skia")]
    const ACTIVE_RENDERER_NAME: &str = "TinySkiaRenderer";

    #[cfg(feature = "active-vger")]
    const ACTIVE_IS_VGER: bool = true;
    #[cfg(not(feature = "active-vger"))]
    const ACTIVE_IS_VGER: bool = false;

    pub(crate) fn is_vger(&self) -> bool {
        Self::ACTIVE_IS_VGER && matches!(self, Renderer::Active(_))
    }

    #[cfg(any(feature = "active-vello", feature = "active-vger"))]
    fn try_new_active(
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        scale: f64,
        font_embolden: f32,
    ) -> Result<ActiveState, String> {
        let presenter = GpuWindowPresenter::new(&gpu_resources, surface, width, height)?;
        let renderer = ActiveRenderer::new(gpu_resources, width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?;
        Ok(ActiveState {
            renderer,
            presenter,
            size: Size::new(width as f64, height as f64),
        })
    }

    #[cfg(any(
        feature = "active-vello-hybrid",
        feature = "active-vello-cpu",
        feature = "active-skia",
        feature = "active-tiny-skia"
    ))]
    fn try_new_active(
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
        _scale: f64,
        _font_embolden: f32,
    ) -> Result<ActiveState, String> {
        let presenter = CpuImagePresenter::new(window, width, height)?;
        #[cfg(feature = "active-tiny-skia")]
        let renderer = ActiveRenderer::new(width, height, _scale, _font_embolden)
            .map_err(|err| err.to_string())?;
        #[cfg(any(
            feature = "active-vello-hybrid",
            feature = "active-vello-cpu",
            feature = "active-skia"
        ))]
        let renderer = ActiveRenderer::new(width, height).map_err(|err| err.to_string())?;
        Ok(ActiveState {
            renderer,
            presenter,
            size: Size::new(width as f64, height as f64),
        })
    }

    #[allow(unused_variables)]
    fn try_new_cpu_fallback(
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
        scale: f64,
        font_embolden: f32,
    ) -> Result<CpuFallbackState, String> {
        let presenter = CpuImagePresenter::new(window, width, height)?;
        #[cfg(feature = "fallback-tiny-skia")]
        let renderer = CpuFallbackRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?;
        #[cfg(feature = "fallback-vello-cpu")]
        let renderer = CpuFallbackRenderer::new(width, height).map_err(|err| err.to_string())?;
        Ok(CpuFallbackState {
            renderer,
            presenter,
            size: Size::new(width as f64, height as f64),
        })
    }

    #[cfg(any(feature = "active-vello", feature = "active-vger"))]
    #[allow(unused_variables)]
    pub fn new(
        window: Arc<dyn Window>,
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));

        let force_cpu = std::env::var("FLOEM_FORCE_CPU")
            .ok()
            .map(|val| val.as_str() == "1")
            .or_else(|| {
                std::env::var("FLOEM_FORCE_TINY_SKIA")
                    .ok()
                    .map(|val| val.as_str() == "1")
            })
            .unwrap_or(false);

        #[cfg(any(feature = "active-vello", feature = "active-vger"))]
        let active_err = if !force_cpu {
            match Self::try_new_active(
                gpu_resources,
                surface,
                size.width as u32,
                size.height as u32,
                scale,
                font_embolden,
            ) {
                Ok(renderer) => return Self::Active(renderer),
                Err(err) => Some(err),
            }
        } else {
            None
        };

        let cpu_fallback_err = match Self::try_new_cpu_fallback(
            window,
            size.width as u32,
            size.height as u32,
            scale,
            font_embolden,
        ) {
            Ok(state) => return Self::CpuFallback(state),
            Err(err) => err,
        };

        if !force_cpu {
            panic!(
                "Failed to create {}: {}\nFailed to create CPU fallback renderer: {cpu_fallback_err}",
                Self::ACTIVE_RENDERER_NAME,
                active_err.unwrap()
            );
        } else {
            panic!("Failed to create CPU fallback renderer: {cpu_fallback_err}");
        }
    }

    #[cfg(any(
        feature = "active-vello-hybrid",
        feature = "active-vello-cpu",
        feature = "active-skia",
        feature = "active-tiny-skia"
    ))]
    pub fn new_cpu(window: Arc<dyn Window>, scale: f64, size: Size, font_embolden: f32) -> Self {
        let size = Size::new(size.width.max(1.0), size.height.max(1.0));

        let active_err = match Self::try_new_active(
            window.clone(),
            size.width as u32,
            size.height as u32,
            scale,
            font_embolden,
        ) {
            Ok(renderer) => return Self::Active(renderer),
            Err(err) => Some(err),
        };

        let cpu_fallback_err = match Self::try_new_cpu_fallback(
            window,
            size.width as u32,
            size.height as u32,
            scale,
            font_embolden,
        ) {
            Ok(state) => return Self::CpuFallback(state),
            Err(err) => err,
        };

        panic!(
            "Failed to create {}: {}\nFailed to create CPU fallback renderer: {cpu_fallback_err}",
            Self::ACTIVE_RENDERER_NAME,
            active_err.unwrap()
        );
    }

    pub(crate) fn debug_info(&self) -> String {
        match self {
            Self::Active(state) => state.renderer.debug_info(),
            Self::CpuFallback(state) => state.renderer.debug_info(),
            Self::Uninitialized { .. } => "Uninitialized".to_string(),
        }
    }

    pub(crate) fn begin(&mut self, frame: BeginFrame) {
        let size = Size::new(frame.size.width.max(1.0), frame.size.height.max(1.0));
        let width = size.width as u32;
        let height = size.height as u32;
        match self {
            Renderer::Active(state) => {
                let size_changed = state.size != size;
                if size_changed {
                    state.presenter.resize(width, height);
                    state.size = size;
                }
                #[cfg(any(
                    feature = "active-vello",
                    feature = "active-vger",
                    feature = "active-tiny-skia"
                ))]
                state
                    .renderer
                    .begin(width, height, frame.scale, frame.font_embolden);
                #[cfg(any(
                    feature = "active-vello-hybrid",
                    feature = "active-vello-cpu",
                    feature = "active-skia"
                ))]
                {
                    if size_changed {
                        state.renderer = ActiveRenderer::new(width, height).unwrap_or_else(|err| {
                            panic!("Failed to recreate {}: {err}", Self::ACTIVE_RENDERER_NAME)
                        });
                    }
                    state.renderer.reset();
                }
            }
            Renderer::CpuFallback(state) => {
                let size_changed = state.size != size;
                if size_changed {
                    state.presenter.resize(width, height);
                    state.size = size;
                }
                #[cfg(feature = "fallback-tiny-skia")]
                state
                    .renderer
                    .begin(width, height, frame.scale, frame.font_embolden);
                #[cfg(feature = "fallback-vello-cpu")]
                {
                    if size_changed {
                        state.renderer =
                            CpuFallbackRenderer::new(width, height).unwrap_or_else(|err| {
                                panic!("Failed to recreate CPU fallback renderer: {err}")
                            });
                    }
                    state.renderer.reset();
                }
            }
            Renderer::Uninitialized { .. } => {}
        }
    }

    pub(crate) fn finish(&mut self, mode: FinishMode) -> Option<ImageData> {
        match self {
            #[cfg(any(feature = "active-vello", feature = "active-vger"))]
            Renderer::Active(state) => match state.renderer.finish(mode == FinishMode::Capture) {
                Some(RenderOutput::Image(image)) => Some(image),
                Some(RenderOutput::GpuTexture(output)) => {
                    if mode == FinishMode::Present {
                        state.presenter.present(&output);
                        None
                    } else {
                        None
                    }
                }
                None => None,
            },
            #[cfg(any(
                feature = "active-vello-hybrid",
                feature = "active-vello-cpu",
                feature = "active-skia",
                feature = "active-tiny-skia"
            ))]
            Renderer::Active(state) => {
                #[cfg(feature = "active-vello-hybrid")]
                let image = state.renderer.finish();
                #[cfg(feature = "active-vello-cpu")]
                let image = state.renderer.finish();
                #[cfg(feature = "active-skia")]
                let image = state.renderer.finish();
                #[cfg(feature = "active-tiny-skia")]
                let image = state.renderer.finish();
                if mode == FinishMode::Present
                    && let Some(image) = image.as_ref()
                {
                    state.presenter.present(image);
                    return None;
                }
                image
            }
            Renderer::CpuFallback(state) => {
                #[cfg(feature = "fallback-vello-cpu")]
                let image = state.renderer.finish();
                #[cfg(feature = "fallback-tiny-skia")]
                let image = state.renderer.finish();
                if mode == FinishMode::Present
                    && let Some(image) = image.as_ref()
                {
                    state.presenter.present(image);
                    return None;
                }
                image
            }
            Renderer::Uninitialized { .. } => None,
        }
    }
}

impl PaintSink for Renderer {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        match self {
            Renderer::Active(state) => PaintSink::push_clip(&mut state.renderer, clip),
            Renderer::CpuFallback(state) => PaintSink::push_clip(&mut state.renderer, clip),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn pop_clip(&mut self) {
        match self {
            Renderer::Active(state) => PaintSink::pop_clip(&mut state.renderer),
            Renderer::CpuFallback(state) => PaintSink::pop_clip(&mut state.renderer),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        match self {
            Renderer::Active(state) => PaintSink::push_group(&mut state.renderer, group),
            Renderer::CpuFallback(state) => PaintSink::push_group(&mut state.renderer, group),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn pop_group(&mut self) {
        match self {
            Renderer::Active(state) => PaintSink::pop_group(&mut state.renderer),
            Renderer::CpuFallback(state) => PaintSink::pop_group(&mut state.renderer),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        match self {
            Renderer::Active(state) => PaintSink::fill(&mut state.renderer, draw),
            Renderer::CpuFallback(state) => PaintSink::fill(&mut state.renderer, draw),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        match self {
            Renderer::Active(state) => PaintSink::stroke(&mut state.renderer, draw),
            Renderer::CpuFallback(state) => PaintSink::stroke(&mut state.renderer, draw),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn glyph_run(
        &mut self,
        draw: GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        match self {
            Renderer::Active(state) => PaintSink::glyph_run(&mut state.renderer, draw, glyphs),
            Renderer::CpuFallback(state) => PaintSink::glyph_run(&mut state.renderer, draw, glyphs),
            Renderer::Uninitialized { .. } => {}
        }
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        match self {
            Renderer::Active(state) => PaintSink::blurred_rounded_rect(&mut state.renderer, draw),
            Renderer::CpuFallback(state) => {
                PaintSink::blurred_rounded_rect(&mut state.renderer, draw)
            }
            Renderer::Uninitialized { .. } => {}
        }
    }
}

impl CustomPaintSink<DisplayCommandExt> for Renderer {
    fn custom(&mut self, command: &DisplayCommandExt) {
        match self {
            Renderer::Active(state) => CustomPaintSink::custom(&mut state.renderer, command),
            Renderer::CpuFallback(state) => CustomPaintSink::custom(&mut state.renderer, command),
            Renderer::Uninitialized { .. } => {}
        }
    }
}
