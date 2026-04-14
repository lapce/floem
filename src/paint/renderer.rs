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
use std::{
    num::NonZeroU32,
    sync::{Arc, mpsc},
    thread,
};

use crate::Application;
use crate::gpu_resources::GpuResources;
use imaging::{
    BlurredRoundedRect, ClipRef, FillRef, GlyphRunRef, GroupRef, ImageBufferTarget, ImageRenderer,
    PaintSink, RenderSource, RgbaImage, StrokeRef, record::Scene,
};
use imaging_wgpu::{TextureRenderer, TextureViewTarget};
use peniko::ImageData;
use peniko::kurbo::Size;
#[cfg(not(target_arch = "wasm32"))]
use pixels::{Pixels, SurfaceTexture};
use softbuffer::{Context, Surface};
use wgpu::util::TextureBlitter;
use winit::window::{Window, WindowId};

use crate::app::UserEvent;
use crate::platform::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default)]
pub struct TimingSpan {
    pub start: Option<Instant>,
    pub end: Option<Instant>,
}

impl TimingSpan {
    pub fn new(start: Instant, end: Instant) -> Self {
        Self {
            start: Some(start),
            end: Some(end),
        }
    }

    pub fn duration(&self) -> Duration {
        match (self.start, self.end) {
            (Some(start), Some(end)) => end.saturating_duration_since(start),
            _ => Duration::ZERO,
        }
    }
}

pub(crate) type WindowBackend = Box<dyn WindowRenderer>;
pub(crate) type RendererChooser = Arc<dyn Fn(NewRendererCx) -> RendererSpec + Send + Sync>;

pub struct NewRendererCx {
    pub window: Arc<dyn Window>,
    pub gpu_resources: Option<GpuResources>,
    pub surface_caps: Option<wgpu::SurfaceCapabilities>,
    pub transparent: bool,
    pub size: Size,
    pub scale: f64,
}

impl NewRendererCx {
    #[cfg(target_arch = "wasm32")]
    fn normalized_size(&self) -> Size {
        Size::new(self.size.width.max(1.0), self.size.height.max(1.0))
    }

    pub fn gpu(&self) -> Option<GpuRendererChooserCx<'_>> {
        if force_cpu_requested() {
            return None;
        }

        match (&self.surface_caps, &self.gpu_resources) {
            (Some(surface_caps), Some(gpu_resources)) => Some(GpuRendererChooserCx {
                gpu_resources,
                surface_caps,
            }),
            _ => None,
        }
    }

    pub fn image_renderer(
        self,
        backend: impl ImageRenderer + 'static,
        name: &'static str,
    ) -> RendererSpec {
        RendererSpec(RendererSpecInner::Cpu(CpuRenderer::new(backend, name)))
    }

    pub fn provided_texture_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::provided_texture(backend, device, name),
            surface_format,
        })
    }

    pub fn owned_texture_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::owned_texture(backend, device, name),
            surface_format,
        })
    }

    pub fn provided_texture_view_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::provided_texture_view(backend, device, name),
            surface_format,
        })
    }

    pub fn owned_texture_view_renderer(
        self,
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        surface_format: wgpu::TextureFormat,
        name: &'static str,
    ) -> RendererSpec {
        let device = self
            .gpu_resources
            .as_ref()
            .expect("renderer requires GPU device")
            .device
            .clone();
        RendererSpec(RendererSpecInner::Gpu {
            backend: GpuRenderer::owned_texture_view(backend, device, name),
            surface_format,
        })
    }
}

pub struct GpuRendererChooserCx<'a> {
    pub gpu_resources: &'a GpuResources,
    pub surface_caps: &'a wgpu::SurfaceCapabilities,
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

impl NewRendererCx {
    pub(crate) fn build(
        chooser: &RendererChooser,
        window: Arc<dyn Window>,
        gpu_resources: Option<GpuResources>,
        surface: Option<wgpu::Surface<'static>>,
        transparent: bool,
        scale: f64,
        size: Size,
    ) -> WindowBackend {
        let surface_caps = match (&surface, &gpu_resources) {
            (Some(surface), Some(gpu_resources)) => {
                Some(surface.get_capabilities(&gpu_resources.adapter))
            }
            _ => None,
        };
        let cx = Self {
            window,
            gpu_resources,
            surface_caps,
            transparent,
            size,
            scale,
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::new(
                ThreadedWindowRenderer::new(Arc::clone(chooser), cx, surface)
                    .expect("create renderer"),
            )
        }

        #[cfg(target_arch = "wasm32")]
        {
            build_window_renderer(chooser(cx), cx, surface).expect("create renderer")
        }
    }

    #[allow(
        unreachable_code,
        dead_code,
        reason = "This CPU window path may be unused when no CPU renderer is enabled in the current build."
    )]
    pub(crate) fn build_cpu(
        chooser: &RendererChooser,
        window: Arc<dyn Window>,
        scale: f64,
        size: Size,
    ) -> WindowBackend {
        let cx = Self {
            window,
            gpu_resources: None,
            surface_caps: None,
            transparent: false,
            size,
            scale,
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::new(
                ThreadedWindowRenderer::new(Arc::clone(chooser), cx, None)
                    .expect("create renderer"),
            )
        }

        #[cfg(target_arch = "wasm32")]
        {
            build_window_renderer(chooser(cx), cx, None).expect("create renderer")
        }
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
struct SoftbufferWindowTarget<W> {
    surface: softbuffer::Surface<W, W>,
    width: u32,
    height: u32,
}

#[allow(
    dead_code,
    reason = "CPU window targets may be unused when no CPU renderer is enabled in the current build."
)]
enum CpuWindowTarget<W> {
    #[cfg(not(target_arch = "wasm32"))]
    Pixels(Box<PixelsWindowTarget>),
    Softbuffer(SoftbufferWindowTarget<W>),
}

#[cfg(not(target_arch = "wasm32"))]
struct PixelsWindowTarget {
    pixels: Pixels<'static>,
    width: u32,
    height: u32,
}

struct GpuWindowTarget {
    blitter: TextureBlitter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

#[allow(
    dead_code,
    reason = "CPU window backend factories may be unused when no CPU renderer is enabled in the current build."
)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderTiming {
    pub total: Duration,
    pub prepare: Duration,
    pub scene: Duration,
    pub finalize: Duration,
    pub read_output: Duration,
    pub total_span: TimingSpan,
    pub prepare_span: TimingSpan,
    pub scene_span: TimingSpan,
    pub finalize_span: TimingSpan,
    pub read_output_span: TimingSpan,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderPassTiming {
    pub total: Duration,
    pub resize: Duration,
    pub render_cpu: Duration,
    pub render: Option<RenderTiming>,
    pub total_span: TimingSpan,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PresentPassTiming {
    pub total: Duration,
    pub resize: Duration,
    pub ready_wait: Duration,
    pub pre_present_notify: Duration,
    pub present_cpu: Duration,
    pub present: Option<PresentTiming>,
    pub total_span: TimingSpan,
    pub ready_wait_span: TimingSpan,
    pub pre_present_notify_span: TimingSpan,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PresentTiming {
    pub total: Duration,
    pub acquire_surface: Duration,
    pub compose: Duration,
    pub submit: Duration,
    pub present_call: Duration,
    pub total_span: TimingSpan,
    pub acquire_surface_span: TimingSpan,
    pub compose_span: TimingSpan,
    pub submit_span: TimingSpan,
    pub present_call_span: TimingSpan,
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
    pub error: Option<String>,
    pub timing: CaptureTiming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PresentFrameError {
    MissingFrame,
}

pub(crate) trait WindowRenderer {
    /// Updates backend-owned presentation resources to the requested window
    /// size.
    ///
    /// This is presentation/backend setup only. It does not paint, prepare a
    /// frame, or present one.
    fn resize(&mut self, width: u32, height: u32);

    /// Runs one backend render operation from the supplied scene source.
    ///
    /// This is the renderer-internal notion of "render": consume a scene and
    /// begin or complete backend preparation of a frame. The window layer calls
    /// this from its paint stage.
    ///
    /// Returning `Some(RenderTiming)` means the backend accepted paint work for
    /// the supplied frame id. A non-zero frame id participates in the live
    /// window frame pipeline and becomes ready later through an explicit
    /// `FrameReady` app event.
    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        frame_id: u64,
    ) -> Option<RenderTiming>;

    /// Presents the explicit prepared frame selected by the caller.
    ///
    /// This must not paint or poll for work. It only consumes the prepared
    /// frame identified by `frame_id` and attempts the backend/platform present
    /// step.
    fn present_frame(&mut self, _frame_id: u64) -> Result<PresentTiming, PresentFrameError> {
        Err(PresentFrameError::MissingFrame)
    }

    /// Captures output from the supplied scene source without going through the
    /// live window frame pipeline.
    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput;

    /// Returns a human-readable backend description for diagnostics.
    fn debug_info(&mut self) -> String;

    /// Returns the platform surface when the backend presents directly to one.
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
        _size: Size,
        source: &mut dyn RenderSource,
        _frame_id: u64,
    ) -> Option<RenderTiming> {
        source.paint_into(&mut self.renderer);
        None
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
    backend: CpuRenderer,
    window_id: WindowId,
    target: CpuWindowTarget<Arc<dyn Window>>,
    prepared_frame: Option<(u64, RgbaImage)>,
}

#[cfg(not(target_arch = "wasm32"))]
struct ThreadedWindowRenderer {
    name: &'static str,
    presenter: ThreadedRendererPresenter,
    worker: OffscreenRenderWorker,
}

#[cfg(not(target_arch = "wasm32"))]
struct OffscreenRenderWorker {
    sender: mpsc::Sender<RenderWorkerCommand>,
    receiver: mpsc::Receiver<ReadyFrame>,
    join_handle: Option<thread::JoinHandle<()>>,
    in_flight: bool,
}

#[cfg(not(target_arch = "wasm32"))]
struct OffscreenRenderJob {
    frame_id: u64,
    scene: Scene,
    size: Size,
}

#[cfg(not(target_arch = "wasm32"))]
struct ReadyFrame {
    frame_id: u64,
    frame: RenderedFrame,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum RenderedFrame {
    Cpu(RenderedImageFrame),
    Gpu(wgpu::Texture),
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct RenderedImageFrame {
    image: RgbaImage,
}

#[cfg(not(target_arch = "wasm32"))]
enum ThreadedRendererPresenter {
    Cpu(CpuWindowTarget<Arc<dyn Window>>),
    Gpu(GpuWindowTarget),
}

pub struct RendererSpec(RendererSpecInner);

enum RendererSpecInner {
    Cpu(CpuRenderer),
    Gpu {
        backend: GpuRenderer,
        surface_format: wgpu::TextureFormat,
    },
}

#[cfg(not(target_arch = "wasm32"))]
enum RendererInit {
    Cpu {
        name: &'static str,
    },
    Gpu {
        name: &'static str,
        surface_format: wgpu::TextureFormat,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl RendererInit {
    fn from_spec(spec: &RendererSpec) -> Self {
        match &spec.0 {
            RendererSpecInner::Cpu(backend) => Self::Cpu { name: backend.name },
            RendererSpecInner::Gpu {
                backend,
                surface_format,
            } => Self::Gpu {
                name: backend.name,
                surface_format: *surface_format,
            },
        }
    }

    fn name(&self) -> &'static str {
        match self {
            RendererInit::Cpu { name } | RendererInit::Gpu { name, .. } => name,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum RenderWorkerCommand {
    Render {
        frame_id: u64,
        scene: Scene,
        size: Size,
    },
    Capture {
        scene: Scene,
        size: Size,
        response: mpsc::Sender<CaptureOutput>,
    },
    Shutdown,
}

struct CpuRenderer {
    backend: Box<dyn ImageRenderer>,
    name: &'static str,
}

impl CpuRenderer {
    fn new(backend: impl ImageRenderer + 'static, name: &'static str) -> Self {
        Self {
            backend: Box::new(backend),
            name,
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn build_window_renderer(
    spec: RendererSpec,
    cx: NewRendererCx,
    surface: Option<wgpu::Surface<'static>>,
) -> Result<WindowBackend, String> {
    let size = cx.normalized_size();
    match spec.0 {
        RendererSpecInner::Cpu(backend) => Ok(Box::new(ImageWindowRenderer {
            backend,
            window_id: cx.window.id(),
            target: CpuWindowTarget::new(cx.window, size.width as u32, size.height as u32)?,
            prepared_frame: None,
        })),
        RendererSpecInner::Gpu {
            backend,
            surface_format,
        } => {
            let gpu_resources = cx
                .gpu_resources
                .ok_or_else(|| "renderer requires GPU".to_string())?;
            let surface = surface.ok_or_else(|| "renderer requires GPU surface".to_string())?;
            Ok(Box::new(TargetGpuWindowRenderer {
                backend,
                window_id: cx.window.id(),
                target: GpuWindowTarget::new(
                    &gpu_resources,
                    surface,
                    size.width as u32,
                    size.height as u32,
                    cx.transparent,
                    Some(surface_format),
                )?,
                ready_frame: None,
            }))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ThreadedWindowRenderer {
    fn new(
        chooser: RendererChooser,
        cx: NewRendererCx,
        surface: Option<wgpu::Surface<'static>>,
    ) -> Result<Self, String> {
        let width = cx.size.width.max(1.0) as u32;
        let height = cx.size.height.max(1.0) as u32;
        let window = Arc::clone(&cx.window);
        let transparent = cx.transparent;
        let gpu_resources = cx.gpu_resources.clone();
        let (worker, init) = OffscreenRenderWorker::spawn(cx.window.id(), chooser, cx)?;
        let presenter = match init {
            RendererInit::Cpu { .. } => {
                ThreadedRendererPresenter::Cpu(CpuWindowTarget::new(window, width, height)?)
            }
            RendererInit::Gpu { surface_format, .. } => {
                let gpu_resources =
                    gpu_resources.ok_or_else(|| "renderer requires GPU resources".to_string())?;
                let surface = surface.ok_or_else(|| "renderer requires GPU surface".to_string())?;
                ThreadedRendererPresenter::Gpu(GpuWindowTarget::new(
                    &gpu_resources,
                    surface,
                    width,
                    height,
                    transparent,
                    Some(surface_format),
                )?)
            }
        };
        Ok(Self {
            name: init.name(),
            presenter,
            worker,
        })
    }

    fn record_scene(source: &mut dyn RenderSource) -> Scene {
        let mut scene = Scene::new();
        source.paint_into(&mut scene);
        scene
    }

    fn submit_job(&mut self, job: OffscreenRenderJob) {
        self.worker.submit(job);
    }

    fn present_frame(&mut self, frame: RenderedFrame) -> Option<PresentTiming> {
        match (&mut self.presenter, frame) {
            (ThreadedRendererPresenter::Cpu(target), RenderedFrame::Cpu(frame)) => {
                if !target.matches_size(frame.image.width, frame.image.height) {
                    return None;
                }
                Some(target.present_rgba(&frame.image))
            }
            (ThreadedRendererPresenter::Gpu(target), RenderedFrame::Gpu(texture)) => {
                let start = Instant::now();
                let acquire_start = start;
                let surface_texture = target
                    .surface
                    .get_current_texture()
                    .expect("failed to acquire surface texture");
                let acquire_surface = acquire_start.elapsed();
                let compose_start = Instant::now();
                target.copy_texture_to_surface(&texture, &surface_texture.texture);
                let compose = compose_start.elapsed();
                let present_call_start = Instant::now();
                surface_texture.present();
                let present_call = present_call_start.elapsed();
                let end = Instant::now();
                Some(PresentTiming {
                    total: start.elapsed(),
                    acquire_surface,
                    compose,
                    submit: Duration::ZERO,
                    present_call,
                    total_span: TimingSpan::new(start, end),
                    acquire_surface_span: TimingSpan::new(acquire_start, compose_start),
                    compose_span: TimingSpan::new(compose_start, present_call_start),
                    submit_span: TimingSpan::default(),
                    present_call_span: TimingSpan::new(present_call_start, end),
                })
            }
            _ => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl OffscreenRenderWorker {
    fn spawn(
        window_id: winit::window::WindowId,
        chooser: RendererChooser,
        cx: NewRendererCx,
    ) -> Result<(Self, RendererInit), String> {
        let (command_tx, command_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();
        let join_handle = thread::Builder::new()
            .name(format!("floem-render-{window_id:?}"))
            .spawn(move || {
                let mut backend = chooser(cx);
                let init = RendererInit::from_spec(&backend);
                if init_tx.send(init).is_err() {
                    return;
                }
                while let Ok(command) = command_rx.recv() {
                    match command {
                        RenderWorkerCommand::Render {
                            frame_id,
                            mut scene,
                            size,
                        } => {
                            if let Some(frame) = render_offscreen(&mut backend, &mut scene, size) {
                                if frame_id == 0 {
                                    continue;
                                }
                                let _ = result_tx.send(ReadyFrame { frame_id, frame });
                                Application::send_proxy_event(UserEvent::FrameReady {
                                    window_id,
                                    frame_id,
                                });
                            }
                        }
                        RenderWorkerCommand::Capture {
                            mut scene,
                            size,
                            response,
                        } => {
                            let _ =
                                response.send(capture_offscreen(&mut backend, &mut scene, size));
                        }
                        RenderWorkerCommand::Shutdown => break,
                    }
                }
            })
            .expect("failed to spawn render worker");
        let init = init_rx
            .recv()
            .map_err(|_| "render worker thread stopped during initialization".to_string())?;
        Ok((
            Self {
                sender: command_tx,
                receiver: result_rx,
                join_handle: Some(join_handle),
                in_flight: false,
            },
            init,
        ))
    }

    fn submit(&mut self, job: OffscreenRenderJob) {
        self.sender
            .send(RenderWorkerCommand::Render {
                frame_id: job.frame_id,
                scene: job.scene,
                size: job.size,
            })
            .expect("render worker thread stopped unexpectedly");
        self.in_flight = true;
    }

    fn capture(&mut self, job: OffscreenRenderJob) -> CaptureOutput {
        let (response_tx, response_rx) = mpsc::channel();
        self.sender
            .send(RenderWorkerCommand::Capture {
                scene: job.scene,
                size: job.size,
                response: response_tx,
            })
            .expect("render worker thread stopped unexpectedly");
        response_rx
            .recv()
            .expect("render worker thread stopped during capture")
    }

    fn take_ready_frame(&mut self, frame_id: u64) -> Option<RenderedFrame> {
        let mut latest = self.receiver.try_recv().ok()?;
        while let Ok(next) = self.receiver.try_recv() {
            latest = next;
        }
        self.in_flight = false;
        (latest.frame_id == frame_id).then_some(latest.frame)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for OffscreenRenderWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(RenderWorkerCommand::Shutdown);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct TargetGpuWindowRenderer {
    backend: GpuRenderer,
    window_id: WindowId,
    target: GpuWindowTarget,
    ready_frame: Option<(u64, wgpu::Texture)>,
}

struct GpuRenderer {
    backend: GpuRendererBackend,
    output: GpuOutputMode,
    device: wgpu::Device,
    name: &'static str,
}

#[allow(
    dead_code,
    reason = "Some GPU backend variants are only constructed when optional renderers are enabled."
)]
enum GpuRendererBackend {
    Texture(Box<dyn TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture>>),
    TextureView(
        Box<dyn TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>>,
    ),
}

enum GpuOutputMode {
    ProvidedTarget,
    OwnedTexture,
}

impl GpuRenderer {
    #[allow(
        dead_code,
        reason = "Texture-target GPU constructors are only used when optional renderers are enabled."
    )]
    fn provided_texture(
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::Texture(Box::new(backend)),
            output: GpuOutputMode::ProvidedTarget,
            device,
            name,
        }
    }

    #[allow(
        dead_code,
        reason = "Texture-target GPU constructors are only used when optional renderers are enabled."
    )]
    fn owned_texture(
        backend: impl TextureRenderer<TextureTarget = wgpu::Texture, Texture = wgpu::Texture> + 'static,
        device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::Texture(Box::new(backend)),
            output: GpuOutputMode::OwnedTexture,
            device,
            name,
        }
    }

    fn provided_texture_view(
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::TextureView(Box::new(backend)),
            output: GpuOutputMode::ProvidedTarget,
            device,
            name,
        }
    }

    fn owned_texture_view(
        backend: impl TextureRenderer<TextureTarget = TextureViewTarget, Texture = wgpu::Texture>
        + 'static,
        device: wgpu::Device,
        name: &'static str,
    ) -> Self {
        Self {
            backend: GpuRendererBackend::TextureView(Box::new(backend)),
            output: GpuOutputMode::OwnedTexture,
            device,
            name,
        }
    }

    fn create_render_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floem offscreen frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn render_offscreen(
    backend: &mut RendererSpec,
    source: &mut dyn RenderSource,
    size: Size,
) -> Option<RenderedFrame> {
    let width = size.width.max(1.0) as u32;
    let height = size.height.max(1.0) as u32;
    match &mut backend.0 {
        RendererSpecInner::Cpu(backend) => {
            let mut image = RgbaImage::new(width, height);
            let rendered = backend
                .backend
                .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image));
            rendered
                .ok()
                .map(|_| RenderedFrame::Cpu(RenderedImageFrame { image }))
        }
        RendererSpecInner::Gpu {
            backend,
            surface_format,
        } => {
            let device = backend.device.clone();
            let texture = match (&mut backend.backend, &backend.output) {
                (GpuRendererBackend::Texture(renderer), GpuOutputMode::ProvidedTarget) => {
                    let texture =
                        GpuRenderer::create_render_texture(&device, width, height, *surface_format);
                    renderer
                        .render_source_into_texture(source, texture.clone())
                        .expect("failed to render gpu target");
                    texture
                }
                (GpuRendererBackend::TextureView(renderer), GpuOutputMode::ProvidedTarget) => {
                    let texture =
                        GpuRenderer::create_render_texture(&device, width, height, *surface_format);
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    renderer
                        .render_source_into_texture(
                            source,
                            TextureViewTarget::new(&view, width, height),
                        )
                        .expect("failed to render gpu target");
                    texture
                }
                (GpuRendererBackend::Texture(renderer), GpuOutputMode::OwnedTexture) => renderer
                    .render_source_texture(source, width, height)
                    .expect("failed to render gpu target"),
                (GpuRendererBackend::TextureView(renderer), GpuOutputMode::OwnedTexture) => {
                    renderer
                        .render_source_texture(source, width, height)
                        .expect("failed to render gpu target")
                }
            };
            Some(RenderedFrame::Gpu(texture))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn capture_offscreen(
    backend: &mut RendererSpec,
    source: &mut dyn RenderSource,
    size: Size,
) -> CaptureOutput {
    let total_start = Instant::now();
    let scene_start = total_start;
    let width = size.width.max(1.0) as u32;
    let height = size.height.max(1.0) as u32;
    let mut image = RgbaImage::new(width, height);
    let result =
        match &mut backend.0 {
            RendererSpecInner::Cpu(backend) => backend
                .backend
                .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image)),
            RendererSpecInner::Gpu { backend, .. } => match &mut backend.backend {
                GpuRendererBackend::Texture(renderer) => renderer
                    .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image)),
                GpuRendererBackend::TextureView(renderer) => renderer
                    .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image)),
            },
        };
    let error = result.err().map(|err| err.to_string());
    CaptureOutput {
        image: error.is_none().then(|| rgba_image_into_image_data(image)),
        error,
        timing: CaptureTiming {
            total: total_start.elapsed(),
            scene: scene_start.elapsed(),
            ..Default::default()
        },
    }
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
            blitter: TextureBlitter::new(&gpu_resources.device, config.format),
            device: gpu_resources.device.clone(),
            queue: gpu_resources.queue.clone(),
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

    #[cfg(target_arch = "wasm32")]
    fn create_render_texture(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floem prepared frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    fn copy_texture_to_surface(&self, source: &wgpu::Texture, target: &wgpu::Texture) {
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("floem window texture copy"),
            });
        self.blitter
            .copy(&self.device, &mut encoder, &source_view, &target_view);
        self.queue.submit([encoder.finish()]);
    }
}

#[allow(
    dead_code,
    reason = "CPU window targets may be unused when no CPU renderer is enabled in the current build."
)]
impl<W> SoftbufferWindowTarget<W>
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
        let end = Instant::now();
        PresentTiming {
            total: start.elapsed(),
            acquire_surface,
            compose,
            submit: Duration::ZERO,
            present_call,
            total_span: TimingSpan::new(start, end),
            acquire_surface_span: TimingSpan::new(acquire_start, compose_start),
            compose_span: TimingSpan::new(compose_start, present_call_start),
            submit_span: TimingSpan::default(),
            present_call_span: TimingSpan::new(present_call_start, end),
        }
    }
}

impl<W> CpuWindowTarget<W>
where
    W: Clone
        + raw_window_handle::HasWindowHandle
        + raw_window_handle::HasDisplayHandle
        + Send
        + Sync
        + 'static,
{
    fn new(window: W, width: u32, height: u32) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(target) = PixelsWindowTarget::new(window.clone(), width, height) {
            return Ok(Self::Pixels(Box::new(target)));
        }

        Ok(Self::Softbuffer(SoftbufferWindowTarget::new(
            window, width, height,
        )?))
    }

    fn resize(&mut self, width: u32, height: u32) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Pixels(target) => target.resize(width, height),
            Self::Softbuffer(target) => target.resize(width, height),
        }
    }

    fn present_rgba(&mut self, image: &RgbaImage) -> PresentTiming {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Pixels(target) => target.present_rgba(image),
            Self::Softbuffer(target) => target.present_rgba(image),
        }
    }

    fn matches_size(&self, width: u32, height: u32) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Pixels(target) => target.width == width && target.height == height,
            Self::Softbuffer(target) => target.width == width && target.height == height,
        }
    }

    fn presenter_name(&self) -> &'static str {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Pixels(_) => "Pixels",
            Self::Softbuffer(_) => "Softbuffer",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PixelsWindowTarget {
    fn new<W>(window: W, width: u32, height: u32) -> Result<Self, String>
    where
        W: pixels::wgpu::WindowHandle + Send + Sync + 'static,
    {
        let pixels = Pixels::new(
            width.max(1),
            height.max(1),
            SurfaceTexture::new(width.max(1), height.max(1), window),
        )
        .map_err(|err| err.to_string())?;
        if matches!(
            pixels.adapter().get_info().device_type,
            pixels::wgpu::DeviceType::Cpu
        ) {
            return Err("pixels selected a software adapter".to_string());
        }
        Ok(Self {
            pixels,
            width,
            height,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels
                .resize_surface(width, height)
                .expect("failed to resize pixels surface");
            self.pixels
                .resize_buffer(width, height)
                .expect("failed to resize pixels buffer");
        }
    }

    fn present_rgba(&mut self, image: &RgbaImage) -> PresentTiming {
        let start = Instant::now();
        let compose_start = start;
        self.pixels.frame_mut().copy_from_slice(&image.data);
        let compose = compose_start.elapsed();
        let submit_start = Instant::now();
        self.pixels
            .render()
            .expect("failed to present pixels frame");
        let submit = submit_start.elapsed();
        let end = Instant::now();
        PresentTiming {
            total: start.elapsed(),
            compose,
            submit,
            total_span: TimingSpan::new(start, end),
            compose_span: TimingSpan::new(compose_start, submit_start),
            submit_span: TimingSpan::new(submit_start, end),
            ..Default::default()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WindowRenderer for ThreadedWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        match &mut self.presenter {
            ThreadedRendererPresenter::Cpu(target) => target.resize(width, height),
            ThreadedRendererPresenter::Gpu(target) => target.resize(width, height),
        }
    }

    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        frame_id: u64,
    ) -> Option<RenderTiming> {
        let total_start = Instant::now();
        let scene_start = total_start;
        let job = OffscreenRenderJob {
            frame_id,
            scene: Self::record_scene(source),
            size,
        };
        let scene = scene_start.elapsed();

        if self.worker.in_flight {
            return None;
        }

        self.submit_job(job);

        Some(RenderTiming {
            total: total_start.elapsed(),
            scene,
            total_span: TimingSpan::new(total_start, Instant::now()),
            scene_span: TimingSpan::new(scene_start, Instant::now()),
            ..Default::default()
        })
    }

    fn present_frame(&mut self, frame_id: u64) -> Result<PresentTiming, PresentFrameError> {
        let frame = self
            .worker
            .take_ready_frame(frame_id)
            .ok_or(PresentFrameError::MissingFrame)?;

        let present = self
            .present_frame(frame)
            .ok_or(PresentFrameError::MissingFrame)?;
        Ok(present)
    }

    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        self.worker.capture(OffscreenRenderJob {
            frame_id: 0,
            scene: Self::record_scene(source),
            size,
        })
    }

    fn debug_info(&mut self) -> String {
        let presenter = match &self.presenter {
            ThreadedRendererPresenter::Cpu(target) => target.presenter_name(),
            ThreadedRendererPresenter::Gpu(_) => "WGPU Surface",
        };
        format!("Renderer: {} (threaded, {presenter})", self.name)
    }

    fn gpu_surface(&self) -> Option<&wgpu::Surface<'static>> {
        match &self.presenter {
            ThreadedRendererPresenter::Cpu(_) => None,
            ThreadedRendererPresenter::Gpu(target) => Some(target.gpu_surface()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl WindowRenderer for TargetGpuWindowRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.target.resize(width, height);
    }

    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        frame_id: u64,
    ) -> Option<RenderTiming> {
        let start = Instant::now();
        let prepare_start = Instant::now();
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let texture = self.target.create_render_texture(width, height);
        match (&mut self.backend.backend, &self.backend.output) {
            (GpuRendererBackend::Texture(backend), GpuOutputMode::ProvidedTarget) => backend
                .render_source_into_texture(source, texture.clone())
                .expect("failed to render gpu target"),
            (GpuRendererBackend::TextureView(backend), GpuOutputMode::ProvidedTarget) => {
                let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                backend
                    .render_source_into_texture(
                        source,
                        TextureViewTarget::new(&texture_view, width, height),
                    )
                    .expect("failed to render gpu target");
            }
            (GpuRendererBackend::Texture(backend), GpuOutputMode::OwnedTexture) => {
                let texture = backend
                    .render_source_texture(source, width, height)
                    .expect("failed to render gpu target");
                if frame_id != 0 {
                    self.ready_frame = Some((frame_id, texture));
                    Application::send_proxy_event(UserEvent::FrameReady {
                        window_id: self.window_id,
                        frame_id,
                    });
                }
                let prepare = Duration::ZERO;
                let scene = prepare_start.elapsed();
                let finalize = Duration::ZERO;
                let end = Instant::now();
                return Some(RenderTiming {
                    total: start.elapsed(),
                    prepare,
                    scene,
                    finalize,
                    total_span: TimingSpan::new(start, end),
                    scene_span: TimingSpan::new(prepare_start, end),
                    ..Default::default()
                });
            }
            (GpuRendererBackend::TextureView(backend), GpuOutputMode::OwnedTexture) => {
                let texture = backend
                    .render_source_texture(source, width, height)
                    .expect("failed to render gpu target");
                if frame_id != 0 {
                    self.ready_frame = Some((frame_id, texture));
                    Application::send_proxy_event(UserEvent::FrameReady {
                        window_id: self.window_id,
                        frame_id,
                    });
                }
                let prepare = Duration::ZERO;
                let scene = prepare_start.elapsed();
                let finalize = Duration::ZERO;
                let end = Instant::now();
                return Some(RenderTiming {
                    total: start.elapsed(),
                    prepare,
                    scene,
                    finalize,
                    total_span: TimingSpan::new(start, end),
                    scene_span: TimingSpan::new(prepare_start, end),
                    ..Default::default()
                });
            }
        }
        if frame_id != 0 {
            self.ready_frame = Some((frame_id, texture));
            Application::send_proxy_event(UserEvent::FrameReady {
                window_id: self.window_id,
                frame_id,
            });
        }
        let prepare = Duration::ZERO;
        let scene = prepare_start.elapsed();
        let finalize = Duration::ZERO;
        let end = Instant::now();
        Some(RenderTiming {
            total: start.elapsed(),
            prepare,
            scene,
            finalize,
            total_span: TimingSpan::new(start, end),
            scene_span: TimingSpan::new(prepare_start, end),
            ..Default::default()
        })
    }

    fn present_frame(&mut self, frame_id: u64) -> Result<PresentTiming, PresentFrameError> {
        let (ready_frame_id, texture) = self
            .ready_frame
            .take()
            .ok_or(PresentFrameError::MissingFrame)?;
        if ready_frame_id != frame_id {
            self.ready_frame = Some((ready_frame_id, texture));
            return Err(PresentFrameError::MissingFrame);
        }
        let start = Instant::now();
        let acquire_start = start;
        let surface_texture = self
            .target
            .surface
            .get_current_texture()
            .expect("failed to acquire surface texture");
        let acquire_surface = acquire_start.elapsed();
        let compose_start = Instant::now();
        self.target
            .copy_texture_to_surface(&texture, &surface_texture.texture);
        let compose = compose_start.elapsed();
        let present_call_start = Instant::now();
        surface_texture.present();
        let present_call = present_call_start.elapsed();
        let end = Instant::now();
        Ok(PresentTiming {
            total: start.elapsed(),
            acquire_surface,
            compose,
            submit: Duration::ZERO,
            present_call,
            total_span: TimingSpan::new(start, end),
            acquire_surface_span: TimingSpan::new(acquire_start, compose_start),
            compose_span: TimingSpan::new(compose_start, present_call_start),
            present_call_span: TimingSpan::new(present_call_start, end),
        })
    }

    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        let total_start = Instant::now();
        let scene_start = total_start;
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let mut image = RgbaImage::new(width, height);
        let result =
            match &mut self.backend.backend {
                GpuRendererBackend::Texture(backend) => backend
                    .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image)),
                GpuRendererBackend::TextureView(backend) => backend
                    .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image)),
            };
        let error = result.err().map(|err| {
            let error = err.to_string();
            eprintln!("{} capture failed: {error}", self.backend.name);
            error
        });
        let rendered = error.is_none();
        let scene = scene_start.elapsed();
        CaptureOutput {
            image: rendered.then(|| rgba_image_into_image_data(image)),
            error,
            timing: CaptureTiming {
                total: total_start.elapsed(),
                scene,
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        format!(
            "Renderer: {} ({})",
            self.backend.name,
            self.target.presenter_name()
        )
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

    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        frame_id: u64,
    ) -> Option<RenderTiming> {
        let total_start = Instant::now();
        let scene_start = total_start;
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let mut image = RgbaImage::new(width, height);
        let rendered = self
            .backend
            .backend
            .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image))
            .is_ok();
        self.prepared_frame = (rendered && frame_id != 0).then_some((frame_id, image));
        if rendered && frame_id != 0 {
            Application::send_proxy_event(UserEvent::FrameReady {
                window_id: self.window_id,
                frame_id,
            });
        }
        Some(RenderTiming {
            total: total_start.elapsed(),
            scene: scene_start.elapsed(),
            total_span: TimingSpan::new(total_start, Instant::now()),
            scene_span: TimingSpan::new(scene_start, Instant::now()),
            ..Default::default()
        })
    }

    fn present_frame(&mut self, frame_id: u64) -> Result<PresentTiming, PresentFrameError> {
        let (ready_frame_id, image) = self
            .prepared_frame
            .take()
            .ok_or(PresentFrameError::MissingFrame)?;
        if ready_frame_id != frame_id {
            self.prepared_frame = Some((ready_frame_id, image));
            return Err(PresentFrameError::MissingFrame);
        }
        Ok(self.target.present_rgba(&image))
    }

    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        let total_start = Instant::now();
        let scene_start = total_start;
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let mut image = RgbaImage::new(width, height);
        let result = self
            .backend
            .backend
            .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image));
        let error = result.err().map(|err| {
            let error = err.to_string();
            eprintln!("{} capture failed: {error}", self.backend.name);
            error
        });
        let rendered = error.is_none();
        let scene = scene_start.elapsed();
        CaptureOutput {
            image: rendered.then(|| rgba_image_into_image_data(image)),
            error,
            timing: CaptureTiming {
                total: total_start.elapsed(),
                scene,
                ..Default::default()
            },
        }
    }

    fn debug_info(&mut self) -> String {
        format!("Renderer: {}", self.backend.name)
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

fn pick_supported_texture_format(
    surface_formats: &[wgpu::TextureFormat],
    renderer_formats: &[wgpu::TextureFormat],
) -> Option<wgpu::TextureFormat> {
    surface_formats
        .iter()
        .copied()
        .find(|format| renderer_formats.contains(format))
}

fn is_srgb_texture_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

fn choose_default_renderer(cx: NewRendererCx) -> Result<RendererSpec, String> {
    #[allow(
        unreachable_code,
        reason = "Some feature combinations end the chooser earlier with a concrete fallback renderer."
    )]
    {
        #[cfg(feature = "vello")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !is_srgb_texture_format(*format));
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let backend =
                imaging_vello::VelloRenderer::new(device, queue).map_err(|err| err.to_string())?;
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_view_renderer(backend, surface_format, "Vello GPU"));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_view_renderer(backend, surface_format, "Vello GPU"));
            }
        }

        #[cfg(feature = "vger")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !is_srgb_texture_format(*format));
            let adapter = gpu.gpu_resources.adapter.clone();
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let width = cx.size.width.max(1.0) as u32;
            let height = cx.size.height.max(1.0) as u32;
            let backend =
                floem_vger_renderer::VgerRenderer::new(adapter, device, queue, width, height)
                    .map_err(|err| err.to_string())?;
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_view_renderer(backend, surface_format, "Vger GPU"));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_view_renderer(backend, surface_format, "Vger GPU"));
            }
        }

        #[cfg(feature = "skia")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !(is_srgb_texture_format(*format)));
            let adapter = gpu.gpu_resources.adapter.clone();
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let backend = imaging_skia::SkiaRenderer::new(adapter, device, queue)
                .map_err(|err| err.to_string())?;
            // if let Some(surface_format) = pick_supported_texture_format(
            //     gpu.surface_formats(),
            //     &backend.supported_texture_formats(),
            // ) {
            //     return Ok(cx.provided_texture_renderer(backend, surface_format, "Skia GPU"));
            // }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_renderer(backend, surface_format, "Skia GPU"));
            }
        }

        #[cfg(feature = "vello-hybrid")]
        if let Some(gpu) = cx.gpu() {
            let fallback_surface_format = gpu
                .surface_formats()
                .iter()
                .copied()
                .find(|format| !is_srgb_texture_format(*format));
            let device = gpu.gpu_resources.device.clone();
            let queue = gpu.gpu_resources.queue.clone();
            let backend = imaging_vello_hybrid::VelloHybridRenderer::new(device, queue);
            if let Some(surface_format) = pick_supported_texture_format(
                gpu.surface_formats(),
                &backend.supported_texture_formats(),
            ) {
                return Ok(cx.provided_texture_view_renderer(
                    backend,
                    surface_format,
                    "Vello Hybrid GPU",
                ));
            }
            if let Some(surface_format) = fallback_surface_format {
                return Ok(cx.owned_texture_view_renderer(
                    backend,
                    surface_format,
                    "Vello Hybrid GPU",
                ));
            }
        }

        #[cfg(feature = "vello-cpu")]
        {
            let width = u16::try_from(cx.size.width.max(1.0) as u32)
                .map_err(|_| "width exceeds vello cpu limit".to_string())?;
            let height = u16::try_from(cx.size.height.max(1.0) as u32)
                .map_err(|_| "height exceeds vello cpu limit".to_string())?;
            let backend = imaging_vello_cpu::VelloCpuRenderer::new(width, height);
            return Ok(cx.image_renderer(backend, "Vello CPU"));
        }

        #[cfg(feature = "skia-cpu")]
        {
            let backend = imaging_skia::SkiaCpuRenderer::new();
            return Ok(cx.image_renderer(backend, "Skia CPU"));
        }

        #[cfg(feature = "tiny-skia")]
        {
            let width = cx.size.width.max(1.0) as u32;
            let height = cx.size.height.max(1.0) as u32;
            let backend = imaging_tiny_skia::TinySkiaRenderer::new_with_size(width, height)
                .map_err(|err| err.to_string())?;
            return Ok(cx.image_renderer(backend, "Tiny Skia CPU"));
        }

        #[cfg(feature = "vello-cpu")]
        {
            let width = u16::try_from(cx.size.width.max(1.0) as u32)
                .map_err(|_| "width exceeds vello_cpu limit".to_string())?;
            let height = u16::try_from(cx.size.height.max(1.0) as u32)
                .map_err(|_| "height exceeds vello_cpu limit".to_string())?;
            let backend = imaging_vello_cpu::VelloCpuRenderer::new(width, height);
            return Ok(cx.image_renderer(backend, "Vello CPU"));
        }

        Err("no renderer available for this window target".to_string())
    }
}

pub(crate) fn default_renderer() -> RendererChooser {
    Arc::new(|cx| choose_default_renderer(cx).expect("create renderer"))
}
