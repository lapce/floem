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

use floem_renderer::DisplayCommandExt;
use floem_renderer::FinishMode;
#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
use floem_renderer::RenderOutput;
#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
use floem_renderer::gpu_resources::GpuResources;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::ImageData;
use peniko::kurbo::Size;
#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
use softbuffer::{Context, Surface};
#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
use std::num::NonZeroU32;
#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
use wgpu::util::TextureBlitter;
use winit::window::Window;

#[derive(Clone, Copy, Debug)]
pub struct BeginFrame {
    pub size: Size,
    pub scale: f64,
    pub font_embolden: f32,
}

#[derive(Debug)]
pub enum RasterizerOutput {
    Image(ImageData),
    #[cfg(any(
        feature = "active-vello",
        feature = "active-vger",
        feature = "active-skia"
    ))]
    GpuTexture(wgpu::TextureView),
}

impl RasterizerOutput {
    pub fn into_image(self) -> Option<ImageData> {
        match self {
            Self::Image(image) => Some(image),
            #[cfg(any(
                feature = "active-vello",
                feature = "active-vger",
                feature = "active-skia"
            ))]
            Self::GpuTexture(_) => None,
        }
    }
}

pub trait Rasterizer: PaintSink + CustomPaintSink<DisplayCommandExt> {
    fn begin(&mut self, frame: BeginFrame);
    fn finish(&mut self, mode: FinishMode) -> Option<RasterizerOutput>;
    fn debug_info(&self) -> String;

    #[cfg(any(
        feature = "active-vello-hybrid",
        feature = "active-vello-cpu",
        feature = "active-skia-cpu",
        feature = "active-tiny-skia",
        feature = "fallback-vello-cpu",
        feature = "fallback-tiny-skia"
    ))]
    fn finish_into_buffer(
        &mut self,
        _buffer: &mut [u8],
        _width: u32,
        _height: u32,
        _bytes_per_row: usize,
        _format: CpuBufferFormat,
    ) -> bool {
        false
    }

    fn is_vger(&self) -> bool {
        false
    }
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
#[derive(Clone, Copy, Debug)]
pub enum CpuBufferFormat {
    Rgba8Opaque,
    Bgra8Opaque,
}

struct NullRasterizer;

impl Rasterizer for NullRasterizer {
    fn begin(&mut self, _frame: BeginFrame) {}

    fn finish(&mut self, _mode: FinishMode) -> Option<RasterizerOutput> {
        None
    }

    fn debug_info(&self) -> String {
        "Uninitialized".to_string()
    }
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

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia",
    feature = "fallback-vello-cpu",
    feature = "fallback-tiny-skia"
))]
pub(crate) struct CpuImagePresenter<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
pub(crate) struct GpuWindowPresenter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: TextureBlitter,
}

pub(crate) enum WindowPresenter {
    None,
    #[cfg(any(
        feature = "active-vello",
        feature = "active-vger",
        feature = "active-skia"
    ))]
    Gpu(GpuWindowPresenter),
    #[cfg(any(
        feature = "active-vello-hybrid",
        feature = "active-vello-cpu",
        feature = "active-skia-cpu",
        feature = "active-tiny-skia",
        feature = "fallback-vello-cpu",
        feature = "fallback-tiny-skia"
    ))]
    Cpu(CpuImagePresenter<Arc<dyn Window>>),
}

impl WindowPresenter {
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::None => {}
            #[cfg(any(
                feature = "active-vello",
                feature = "active-vger",
                feature = "active-skia"
            ))]
            Self::Gpu(presenter) => presenter.resize(width, height),
            #[cfg(any(
                feature = "active-vello-hybrid",
                feature = "active-vello-cpu",
                feature = "active-skia-cpu",
                feature = "active-tiny-skia",
                feature = "fallback-vello-cpu",
                feature = "fallback-tiny-skia"
            ))]
            Self::Cpu(presenter) => presenter.resize(width, height),
        }
    }

    pub(crate) fn present(&mut self, output: &RasterizerOutput) {
        match (self, output) {
            (Self::None, _) => {}
            #[cfg(any(
                feature = "active-vello",
                feature = "active-vger",
                feature = "active-skia"
            ))]
            (Self::Gpu(presenter), RasterizerOutput::GpuTexture(output)) => {
                presenter.present(output)
            }
            #[cfg(any(
                feature = "active-vello-hybrid",
                feature = "active-vello-cpu",
                feature = "active-skia-cpu",
                feature = "active-tiny-skia",
                feature = "fallback-vello-cpu",
                feature = "fallback-tiny-skia"
            ))]
            (Self::Cpu(presenter), RasterizerOutput::Image(image)) => presenter.present(image),
            #[cfg(any(
                feature = "active-vello",
                feature = "active-vger",
                feature = "active-skia"
            ))]
            _ => panic!("presenter/output mismatch"),
        }
    }

    pub(crate) fn present_rasterizer(&mut self, rasterizer: &mut dyn Rasterizer) -> bool {
        match self {
            Self::None => false,
            #[cfg(any(
                feature = "active-vello",
                feature = "active-vger",
                feature = "active-skia"
            ))]
            Self::Gpu(_) => false,
            #[cfg(any(
                feature = "active-vello-hybrid",
                feature = "active-vello-cpu",
                feature = "active-skia-cpu",
                feature = "active-tiny-skia",
                feature = "fallback-vello-cpu",
                feature = "fallback-tiny-skia"
            ))]
            Self::Cpu(presenter) => presenter.present_rasterizer(rasterizer),
        }
    }
}

pub(crate) struct RasterizerInit {
    pub(crate) rasterizer: Box<dyn Rasterizer>,
    pub(crate) presenter: WindowPresenter,
}

pub(crate) fn uninitialized_rasterizer() -> Box<dyn Rasterizer> {
    Box::new(NullRasterizer)
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
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
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
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

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
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

    fn present_rasterizer(&mut self, rasterizer: &mut dyn Rasterizer) -> bool {
        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");
        let width = buffer.width().get();
        let height = buffer.height().get();
        let bytes_per_row = width as usize * 4;
        #[cfg(target_endian = "little")]
        let format = CpuBufferFormat::Bgra8Opaque;
        #[cfg(target_endian = "big")]
        let format = CpuBufferFormat::Rgba8Opaque;
        let rendered = {
            let byte_len = std::mem::size_of_val(&*buffer);
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), byte_len)
            };
            rasterizer.finish_into_buffer(bytes, width, height, bytes_per_row, format)
        };
        if rendered {
            for pixel in buffer.iter_mut() {
                *pixel &= 0x00ff_ffff;
            }
            buffer
                .present()
                .expect("failed to present the surface buffer");
        }
        rendered
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

#[cfg(feature = "active-vello")]
impl Rasterizer for floem_vello_renderer::VelloRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self, mode).map(map_render_output)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

#[cfg(feature = "active-vger")]
impl Rasterizer for floem_vger_renderer::VgerRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self, mode).map(map_render_output)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }

    fn is_vger(&self) -> bool {
        true
    }
}

#[cfg(feature = "active-vello-hybrid")]
impl Rasterizer for floem_vello_hybrid_renderer::VelloHybridRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, _mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self).map(RasterizerOutput::Image)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }

    fn finish_into_buffer(
        &mut self,
        buffer: &mut [u8],
        _width: u32,
        _height: u32,
        bytes_per_row: usize,
        format: CpuBufferFormat,
    ) -> bool {
        let result = match format {
            CpuBufferFormat::Rgba8Opaque => self.finish_into_rgba8_opaque(buffer, bytes_per_row),
            CpuBufferFormat::Bgra8Opaque => self.finish_into_bgra8_opaque(buffer, bytes_per_row),
        };
        result.is_ok()
    }
}

#[cfg(any(feature = "active-vello-cpu", feature = "fallback-vello-cpu"))]
impl Rasterizer for floem_vello_cpu_renderer::VelloCpuRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, _mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self).map(RasterizerOutput::Image)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }

    fn finish_into_buffer(
        &mut self,
        buffer: &mut [u8],
        _width: u32,
        _height: u32,
        bytes_per_row: usize,
        format: CpuBufferFormat,
    ) -> bool {
        let result = match format {
            CpuBufferFormat::Rgba8Opaque => self.finish_into_rgba8_opaque(buffer, bytes_per_row),
            CpuBufferFormat::Bgra8Opaque => self.finish_into_bgra8_opaque(buffer, bytes_per_row),
        };
        result.is_ok()
    }
}

#[cfg(feature = "active-skia")]
impl Rasterizer for floem_skia_renderer::SkiaRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self, mode).map(map_render_output)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

#[cfg(feature = "active-skia-cpu")]
impl Rasterizer for floem_skia_cpu_renderer::SkiaCpuRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, _mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self).map(RasterizerOutput::Image)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }

    fn finish_into_buffer(
        &mut self,
        buffer: &mut [u8],
        _width: u32,
        _height: u32,
        bytes_per_row: usize,
        format: CpuBufferFormat,
    ) -> bool {
        let result = match format {
            CpuBufferFormat::Rgba8Opaque => self.finish_into_rgba8_opaque(buffer, bytes_per_row),
            CpuBufferFormat::Bgra8Opaque => self.finish_into_bgra8_opaque(buffer, bytes_per_row),
        };
        result.is_ok()
    }
}

#[cfg(any(feature = "active-tiny-skia", feature = "fallback-tiny-skia"))]
impl Rasterizer for floem_tiny_skia_renderer::TinySkiaRenderer {
    fn begin(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn finish(&mut self, _mode: FinishMode) -> Option<RasterizerOutput> {
        Self::finish(self).map(RasterizerOutput::Image)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }

    fn finish_into_buffer(
        &mut self,
        buffer: &mut [u8],
        _width: u32,
        _height: u32,
        bytes_per_row: usize,
        format: CpuBufferFormat,
    ) -> bool {
        match format {
            CpuBufferFormat::Rgba8Opaque => self.finish_into_rgba8_opaque(buffer, bytes_per_row),
            CpuBufferFormat::Bgra8Opaque => self.finish_into_bgra8_opaque(buffer, bytes_per_row),
        }
        .is_some()
    }
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn map_render_output(output: RenderOutput) -> RasterizerOutput {
    match output {
        RenderOutput::Image(image) => RasterizerOutput::Image(image),
        RenderOutput::GpuTexture(output) => RasterizerOutput::GpuTexture(output),
    }
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn force_cpu_requested() -> bool {
    std::env::var("FLOEM_FORCE_CPU")
        .ok()
        .map(|val| val.as_str() == "1")
        .or_else(|| {
            std::env::var("FLOEM_FORCE_TINY_SKIA")
                .ok()
                .map(|val| val.as_str() == "1")
        })
        .unwrap_or(false)
}

#[cfg(any(
    feature = "active-vello",
    feature = "active-vger",
    feature = "active-skia"
))]
fn try_new_active(
    gpu_resources: GpuResources,
    surface: wgpu::Surface<'static>,
    width: u32,
    height: u32,
    transparent: bool,
    scale: f64,
    font_embolden: f32,
) -> Result<RasterizerInit, String> {
    let texture_format = choose_surface_texture_format(&surface, &gpu_resources)?;

    let presenter = WindowPresenter::Gpu(GpuWindowPresenter::new(
        &gpu_resources,
        surface,
        width,
        height,
        transparent,
    )?);
    #[cfg(feature = "active-vello")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_vello_renderer::VelloRenderer::new(
            gpu_resources,
            width,
            height,
            texture_format,
            scale,
            font_embolden,
        )
        .map_err(|err| err.to_string())?,
    );
    #[cfg(feature = "active-vger")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_vger_renderer::VgerRenderer::new(
            gpu_resources,
            width,
            height,
            texture_format,
            scale,
            font_embolden,
        )
        .map_err(|err| err.to_string())?,
    );
    #[cfg(feature = "active-skia")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_skia_renderer::SkiaRenderer::new(
            gpu_resources,
            width,
            height,
            texture_format,
            scale,
            font_embolden,
        )
        .map_err(|err| err.to_string())?,
    );
    Ok(RasterizerInit {
        rasterizer,
        presenter,
    })
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
))]
fn try_new_active_rasterizer(
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<Box<dyn Rasterizer>, String> {
    #[cfg(feature = "active-vello-hybrid")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_vello_hybrid_renderer::VelloHybridRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    );
    #[cfg(feature = "active-vello-cpu")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_vello_cpu_renderer::VelloCpuRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    );
    #[cfg(feature = "active-skia-cpu")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_skia_cpu_renderer::SkiaCpuRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    );
    #[cfg(feature = "active-tiny-skia")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_tiny_skia_renderer::TinySkiaRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    );
    Ok(rasterizer)
}

#[cfg(any(
    feature = "active-vello-hybrid",
    feature = "active-vello-cpu",
    feature = "active-skia-cpu",
    feature = "active-tiny-skia"
))]
fn try_new_active(
    window: Arc<dyn Window>,
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<RasterizerInit, String> {
    let presenter = WindowPresenter::Cpu(CpuImagePresenter::new(window, width, height)?);
    let rasterizer = try_new_active_rasterizer(width, height, scale, font_embolden)?;
    Ok(RasterizerInit {
        rasterizer,
        presenter,
    })
}

fn try_new_cpu_fallback_rasterizer(
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<Box<dyn Rasterizer>, String> {
    #[cfg(feature = "fallback-vello-cpu")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_vello_cpu_renderer::VelloCpuRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    );
    #[cfg(feature = "fallback-tiny-skia")]
    let rasterizer: Box<dyn Rasterizer> = Box::new(
        floem_tiny_skia_renderer::TinySkiaRenderer::new(width, height, scale, font_embolden)
            .map_err(|err| err.to_string())?,
    );
    Ok(rasterizer)
}

fn try_new_cpu_fallback(
    window: Arc<dyn Window>,
    width: u32,
    height: u32,
    scale: f64,
    font_embolden: f32,
) -> Result<RasterizerInit, String> {
    let presenter = WindowPresenter::Cpu(CpuImagePresenter::new(window, width, height)?);
    let rasterizer = try_new_cpu_fallback_rasterizer(width, height, scale, font_embolden)?;
    Ok(RasterizerInit {
        rasterizer,
        presenter,
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

    let active_name = active_rasterizer_name();
    let active_err = if !force_cpu {
        match try_new_active(
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
            "Failed to create {active_name}: {}\nFailed to create CPU fallback rasterizer: {cpu_fallback_err}",
            active_err.unwrap()
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

    let active_name = active_rasterizer_name();
    let active_err = match try_new_active(window.clone(), width, height, scale, font_embolden) {
        Ok(init) => return init,
        Err(err) => Some(err),
    };

    let cpu_fallback_err = match try_new_cpu_fallback(window, width, height, scale, font_embolden) {
        Ok(init) => return init,
        Err(err) => err,
    };

    panic!(
        "Failed to create {active_name}: {}\nFailed to create CPU fallback rasterizer: {cpu_fallback_err}",
        active_err.unwrap()
    );
}

#[allow(unreachable_code)]
fn active_rasterizer_name() -> &'static str {
    #[cfg(feature = "active-vello")]
    {
        return "VelloRenderer";
    }
    #[cfg(feature = "active-vger")]
    {
        return "VgerRenderer";
    }
    #[cfg(feature = "active-vello-hybrid")]
    {
        return "VelloHybridRenderer";
    }
    #[cfg(feature = "active-vello-cpu")]
    {
        return "VelloCpuRenderer";
    }
    #[cfg(feature = "active-skia")]
    {
        return "SkiaRenderer";
    }
    #[cfg(feature = "active-skia-cpu")]
    {
        return "SkiaCpuRenderer";
    }
    #[cfg(feature = "active-tiny-skia")]
    {
        return "TinySkiaRenderer";
    }

    unreachable!("one active renderer feature should always be enabled");
}
