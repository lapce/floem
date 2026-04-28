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

use crate::gpu_resources::GpuResources;
use imaging::{
    BlurredRoundedRect, ClipRef, FillRef, GlyphRunRef, GroupRef, ImageBufferTarget, ImageRenderer,
    PaintSink, RenderSource, RgbaImage, StrokeRef, record::Scene,
};
use imaging_wgpu::{TextureRenderer, TextureViewTarget};
use peniko::ImageData;
use peniko::kurbo::Size;
use softbuffer::{Context, Surface};
use wgpu::util::TextureBlitter;
use winit::window::{Window, WindowId};

use crate::inspector::TimingKind;
use crate::platform::{Duration, Instant};
#[cfg(not(target_arch = "wasm32"))]
use crate::{Application, app::UserEvent};

#[derive(Clone, Copy, Debug)]
pub struct TimingSpan {
    pub start: Instant,
    pub end: Instant,
}

impl TimingSpan {
    pub fn new(start: Instant, end: Instant) -> Self {
        Self { start, end }
    }

    pub fn duration(&self) -> Duration {
        self.end.saturating_duration_since(self.start)
    }
}

pub(crate) trait RendererTimingRecorder {
    fn record_span(&mut self, label: &'static str, span: Option<TimingSpan>, kind: TimingKind);

    fn record_thread_span(
        &mut self,
        label: &'static str,
        span: Option<TimingSpan>,
        kind: TimingKind,
    ) {
        self.record_span(label, span, kind);
    }
}

struct TimedRenderSource<'a> {
    source: &'a mut dyn RenderSource,
    span: Option<TimingSpan>,
}

impl TimedRenderSource<'_> {
    fn new(source: &mut dyn RenderSource) -> TimedRenderSource<'_> {
        TimedRenderSource { source, span: None }
    }
}

impl RenderSource for TimedRenderSource<'_> {
    fn paint_into(&mut self, sink: &mut dyn PaintSink) {
        let start = Instant::now();
        self.source.paint_into(sink);
        self.span = Some(TimingSpan::new(start, Instant::now()));
    }
}

pub(crate) type WindowBackend = Box<dyn WindowRenderer>;
pub(crate) type RendererChooser = Arc<dyn Fn(NewRendererCx) -> RendererSpec + Send + Sync>;

pub struct NewRendererCx {
    pub window: Arc<dyn Window>,
    pub gpu_resources: Option<GpuResources>,
    pub surface_caps: Option<subduction_platform::WgpuPresentSurfaceCapabilities>,
    pub transparent: bool,
    pub size: Size,
    pub scale: f64,
    pub maximum_drawable_count: u32,
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
    pub surface_caps: &'a subduction_platform::WgpuPresentSurfaceCapabilities,
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
        surface_caps: Option<subduction_platform::WgpuPresentSurfaceCapabilities>,
        transparent: bool,
        scale: f64,
        size: Size,
        maximum_drawable_count: u32,
    ) -> WindowBackend {
        let cx = Self {
            window,
            gpu_resources,
            surface_caps,
            transparent,
            size,
            scale,
            maximum_drawable_count,
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::new(ThreadedWindowRenderer::new(Arc::clone(chooser), cx).expect("create renderer"))
        }

        #[cfg(target_arch = "wasm32")]
        {
            build_window_renderer(chooser(cx), cx).expect("create renderer")
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
            maximum_drawable_count: 2,
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::new(ThreadedWindowRenderer::new(Arc::clone(chooser), cx).expect("create renderer"))
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
    Softbuffer(SoftbufferWindowTarget<W>),
    Wgpu(CpuWgpuWindowTarget),
}

struct GpuWindowTarget {
    blitter: TextureBlitter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: subduction_platform::WgpuPresentSurface,
    width: u32,
    height: u32,
    scale: f64,
}

struct CpuWgpuWindowTarget {
    gpu: GpuWindowTarget,
    upload_texture: wgpu::Texture,
    width: u32,
    height: u32,
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

pub(crate) trait WindowRenderer {
    /// Updates backend-owned presentation resources to the requested window
    /// size.
    ///
    /// This is presentation/backend setup only. It does not paint, prepare a
    /// frame, or present one.
    fn resize(&mut self, width: u32, height: u32, scale: f64);

    /// Runs one backend render operation from the supplied scene source.
    ///
    /// This is the renderer-internal notion of "render": consume a scene and
    /// begin or complete backend preparation of a frame. The window layer calls
    /// this from its paint stage.
    ///
    /// Returning `true` means the backend accepted paint work and recorded any
    /// backend-local timing spans into `timing`.
    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        frame_id: u64,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool;

    /// Renders and presents a frame synchronously in the current call stack.
    fn render_immediate_and_present(
        &mut self,
        _size: Size,
        _source: &mut dyn RenderSource,
        _timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        false
    }

    /// Drops backend-owned transient work that should not outlive the current
    /// window frame.
    fn discard_pending_frames(&mut self) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn accept_rendered_frame(&mut self, _frame: RenderedFrame, _render_span: TimingSpan) {}

    fn render_in_flight(&self) -> bool {
        false
    }

    fn has_completed_frame(&self) -> bool {
        false
    }

    /// Renders an offscreen scene using this renderer's existing backend.
    ///
    /// Threaded renderers execute this on their render worker, reusing the same
    /// renderer instance and caches as the main window.
    #[cfg(not(target_arch = "wasm32"))]
    fn render_offscreen_frame(&mut self, _scene: Scene, _size: Size) -> Option<RenderedFrame> {
        None
    }

    /// Captures output from the supplied scene source without going through the
    /// live window frame pipeline.
    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput;

    /// Returns a human-readable backend description for diagnostics.
    fn debug_info(&mut self) -> String;
}

struct NullWindowBackend {
    renderer: NullRenderer,
}

impl WindowRenderer for NullWindowBackend {
    fn resize(&mut self, _width: u32, _height: u32, _scale: f64) {}

    fn render(
        &mut self,
        _size: Size,
        source: &mut dyn RenderSource,
        _frame_id: u64,
        _timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        source.paint_into(&mut self.renderer);
        false
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
}

#[cfg(not(target_arch = "wasm32"))]
struct ThreadedWindowRenderer {
    name: &'static str,
    window_id: WindowId,
    presenter: ThreadedRendererPresenter,
    worker: OffscreenRenderWorker,
    async_render_in_flight: bool,
    completed_frame: Option<(RenderedFrame, TimingSpan)>,
}

#[cfg(not(target_arch = "wasm32"))]
struct OffscreenRenderWorker {
    sender: mpsc::Sender<RenderWorkerCommand>,
    join_handle: Option<thread::JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
struct OffscreenRenderJob {
    scene: Scene,
    size: Size,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum RenderedFrame {
    Cpu(RenderedImageFrame),
    Gpu(wgpu::Texture),
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct RenderedImageFrame {
    pub(crate) image: RgbaImage,
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
    RenderSync {
        scene: Scene,
        size: Size,
        response: mpsc::Sender<Option<RenderedFrame>>,
    },
    RenderAsync {
        window_id: WindowId,
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
fn build_window_renderer(spec: RendererSpec, cx: NewRendererCx) -> Result<WindowBackend, String> {
    let size = cx.normalized_size();
    match spec.0 {
        RendererSpecInner::Cpu(backend) => Ok(Box::new(ImageWindowRenderer {
            backend,
            window_id: cx.window.id(),
            target: CpuWindowTarget::new(
                cx.window,
                size.width as u32,
                size.height as u32,
                cx.scale,
                cx.transparent,
                cx.gpu_resources.as_ref(),
                cx.maximum_drawable_count,
            )?,
        })),
        RendererSpecInner::Gpu {
            backend,
            surface_format,
        } => {
            let gpu_resources = cx
                .gpu_resources
                .ok_or_else(|| "renderer requires GPU".to_string())?;
            Ok(Box::new(TargetGpuWindowRenderer {
                backend,
                window_id: cx.window.id(),
                target: GpuWindowTarget::new(
                    &gpu_resources,
                    cx.window,
                    size.width as u32,
                    size.height as u32,
                    cx.scale,
                    cx.transparent,
                    Some(surface_format),
                    cx.maximum_drawable_count,
                )?,
            }))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ThreadedWindowRenderer {
    fn new(chooser: RendererChooser, cx: NewRendererCx) -> Result<Self, String> {
        let width = cx.size.width.max(1.0) as u32;
        let height = cx.size.height.max(1.0) as u32;
        let scale = cx.scale;
        let window_id = cx.window.id();
        let window = Arc::clone(&cx.window);
        let transparent = cx.transparent;
        let maximum_drawable_count = cx.maximum_drawable_count;
        let gpu_resources = cx.gpu_resources.clone();
        let (worker, init) = OffscreenRenderWorker::spawn(cx.window.id(), chooser, cx)?;
        let presenter = match init {
            RendererInit::Cpu { .. } => ThreadedRendererPresenter::Cpu(CpuWindowTarget::new(
                window,
                width,
                height,
                scale,
                transparent,
                gpu_resources.as_ref(),
                maximum_drawable_count,
            )?),
            RendererInit::Gpu { surface_format, .. } => {
                let gpu_resources =
                    gpu_resources.ok_or_else(|| "renderer requires GPU resources".to_string())?;
                ThreadedRendererPresenter::Gpu(GpuWindowTarget::new(
                    &gpu_resources,
                    window,
                    width,
                    height,
                    scale,
                    transparent,
                    Some(surface_format),
                    maximum_drawable_count,
                )?)
            }
        };
        Ok(Self {
            name: init.name(),
            window_id,
            presenter,
            worker,
            async_render_in_flight: false,
            completed_frame: None,
        })
    }

    fn record_scene(source: &mut dyn RenderSource) -> (Scene, TimingSpan) {
        let mut scene = Scene::new();
        let start = Instant::now();
        source.paint_into(&mut scene);
        (scene, TimingSpan::new(start, Instant::now()))
    }

    fn present_frame(
        &mut self,
        frame: RenderedFrame,
        timing: &mut dyn RendererTimingRecorder,
    ) -> Result<(), RenderedFrame> {
        match (&mut self.presenter, frame) {
            (ThreadedRendererPresenter::Cpu(target), RenderedFrame::Cpu(frame)) => {
                if !target.matches_size(frame.image.width, frame.image.height) {
                    return Err(RenderedFrame::Cpu(frame));
                }
                target.present_rgba(&frame.image, timing);
                Ok(())
            }
            (ThreadedRendererPresenter::Gpu(target), RenderedFrame::Gpu(texture)) => {
                let start = Instant::now();
                let Some(surface_frame) = target.surface.try_acquire_frame() else {
                    return Err(RenderedFrame::Gpu(texture));
                };
                let compose_start = Instant::now();
                target.copy_texture_to_surface(&texture, surface_frame.texture());
                let present_call_start = Instant::now();
                surface_frame.present_after_submit(
                    &target.queue,
                    subduction_core::timing::PresentPacing::AsSoonAsPossible,
                );
                let end = Instant::now();
                timing.record_span(
                    "Present",
                    Some(TimingSpan::new(start, end)),
                    TimingKind::Present,
                );
                timing.record_span(
                    "Compose",
                    Some(TimingSpan::new(compose_start, present_call_start)),
                    TimingKind::Present,
                );
                timing.record_span(
                    "PresentCall",
                    Some(TimingSpan::new(present_call_start, end)),
                    TimingKind::Present,
                );
                Ok(())
            }
            (_, frame) => Err(frame),
        }
    }

    fn submit_async_render(&mut self, window_id: WindowId, scene: Scene, size: Size) {
        self.async_render_in_flight = true;
        self.worker.render_async(window_id, scene, size);
    }

    fn present_completed_frame(&mut self, timing: &mut dyn RendererTimingRecorder) -> bool {
        let Some((frame, render_span)) = self.completed_frame.take() else {
            return false;
        };
        timing.record_thread_span("Render", Some(render_span), TimingKind::Renderer);
        match self.present_frame(frame, timing) {
            Ok(()) => true,
            Err(frame) => {
                self.completed_frame = Some((frame, render_span));
                false
            }
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
                        RenderWorkerCommand::RenderSync {
                            mut scene,
                            size,
                            response,
                        } => {
                            let _ = response.send(render_offscreen(&mut backend, &mut scene, size));
                        }
                        RenderWorkerCommand::RenderAsync {
                            window_id,
                            mut scene,
                            size,
                        } => {
                            let start = Instant::now();
                            if let Some(frame) = render_offscreen(&mut backend, &mut scene, size) {
                                Application::send_proxy_event(UserEvent::RenderFrameReady {
                                    window_id,
                                    frame,
                                    render_span: TimingSpan::new(start, Instant::now()),
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
                join_handle: Some(join_handle),
            },
            init,
        ))
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

    fn render_sync(&mut self, scene: Scene, size: Size) -> Option<RenderedFrame> {
        let (response_tx, response_rx) = mpsc::channel();
        self.sender
            .send(RenderWorkerCommand::RenderSync {
                scene,
                size,
                response: response_tx,
            })
            .expect("render worker thread stopped unexpectedly");
        response_rx
            .recv()
            .expect("render worker thread stopped during sync render")
    }

    fn render_async(&mut self, window_id: WindowId, scene: Scene, size: Size) {
        self.sender
            .send(RenderWorkerCommand::RenderAsync {
                window_id,
                scene,
                size,
            })
            .expect("render worker thread stopped unexpectedly");
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
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
        scale: f64,
        transparent: bool,
        preferred_texture_format: Option<wgpu::TextureFormat>,
        maximum_drawable_count: u32,
    ) -> Result<Self, String> {
        let latency = match gpu_resources.adapter.get_info().backend {
            wgpu::Backend::Vulkan => 2,
            _ => 1,
        };
        let texture_format = preferred_texture_format
            .ok_or_else(|| "renderer requires surface format".to_string())?;
        let mut config = subduction_platform::WgpuPresentSurfaceConfig::new(
            width.max(1),
            height.max(1),
            texture_format,
        );
        config.scale = scale;
        config.transparent = transparent;
        config.desired_maximum_frame_latency = latency;
        config.maximum_drawable_count = maximum_drawable_count;
        let surface = subduction_platform::WgpuPresentSurface::from_window(
            window,
            &gpu_resources.instance,
            &gpu_resources.adapter,
            gpu_resources.device.clone(),
            config,
        )
        .map_err(|err| err.to_string())?;
        let capabilities = surface.capabilities();
        if !capabilities.formats.contains(&texture_format) {
            return Err(format!(
                "renderer surface format {texture_format:?} is not supported"
            ));
        }

        Ok(Self {
            blitter: TextureBlitter::new(&gpu_resources.device, texture_format),
            device: gpu_resources.device.clone(),
            queue: gpu_resources.queue.clone(),
            surface,
            width,
            height,
            scale,
        })
    }

    fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if self.width != width || self.height != height || (self.scale - scale).abs() > f64::EPSILON
        {
            self.width = width;
            self.height = height;
            self.scale = scale;
            self.surface.resize(width.max(1), height.max(1), scale);
        }
    }

    fn copy_texture_to_surface(&mut self, source: &wgpu::Texture, target: &wgpu::Texture) {
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

impl CpuWgpuWindowTarget {
    fn new(
        gpu_resources: &GpuResources,
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
        scale: f64,
        transparent: bool,
        maximum_drawable_count: u32,
    ) -> Result<Self, String> {
        let gpu = GpuWindowTarget::new(
            gpu_resources,
            window,
            width,
            height,
            scale,
            transparent,
            Some(wgpu::TextureFormat::Bgra8Unorm),
            maximum_drawable_count,
        )?;
        let upload_texture = Self::create_upload_texture(&gpu.device, width, height);
        Ok(Self {
            gpu,
            upload_texture,
            width,
            height,
        })
    }

    fn create_upload_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floem cpu window upload texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.width = width;
        self.height = height;
        self.gpu.resize(width, height, scale);
        self.upload_texture = Self::create_upload_texture(&self.gpu.device, width, height);
    }

    fn present_rgba(&mut self, image: &RgbaImage, timing: &mut dyn RendererTimingRecorder) {
        let start = Instant::now();
        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.upload_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.width),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        let compose_start = Instant::now();
        let Some(surface_frame) = self.gpu.surface.try_acquire_frame() else {
            return;
        };
        self.gpu
            .copy_texture_to_surface(&self.upload_texture, surface_frame.texture());
        let present_call_start = Instant::now();
        surface_frame.present_after_submit(
            &self.gpu.queue,
            subduction_core::timing::PresentPacing::AsSoonAsPossible,
        );
        let end = Instant::now();
        timing.record_span(
            "Present",
            Some(TimingSpan::new(start, end)),
            TimingKind::Present,
        );
        timing.record_span(
            "Compose",
            Some(TimingSpan::new(compose_start, present_call_start)),
            TimingKind::Present,
        );
        timing.record_span(
            "PresentCall",
            Some(TimingSpan::new(present_call_start, end)),
            TimingKind::Present,
        );
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

    fn present_rgba(&mut self, image: &RgbaImage, timing: &mut dyn RendererTimingRecorder) {
        let start = Instant::now();
        let acquire_start = start;
        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");
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
        let present_call_start = Instant::now();
        buffer
            .present()
            .expect("failed to present the surface buffer");
        let end = Instant::now();
        timing.record_span(
            "Present",
            Some(TimingSpan::new(start, end)),
            TimingKind::Present,
        );
        timing.record_span(
            "AcquireSurface",
            Some(TimingSpan::new(acquire_start, compose_start)),
            TimingKind::Present,
        );
        timing.record_span(
            "Compose",
            Some(TimingSpan::new(compose_start, present_call_start)),
            TimingKind::Present,
        );
        timing.record_span(
            "PresentCall",
            Some(TimingSpan::new(present_call_start, end)),
            TimingKind::Present,
        );
    }
}

impl CpuWindowTarget<Arc<dyn Window>> {
    fn new(
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
        scale: f64,
        transparent: bool,
        gpu_resources: Option<&GpuResources>,
        maximum_drawable_count: u32,
    ) -> Result<Self, String> {
        if let Some(gpu_resources) = gpu_resources {
            return Ok(Self::Wgpu(CpuWgpuWindowTarget::new(
                gpu_resources,
                Arc::clone(&window),
                width,
                height,
                scale,
                transparent,
                maximum_drawable_count,
            )?));
        }

        Ok(Self::Softbuffer(SoftbufferWindowTarget::new(
            window, width, height,
        )?))
    }

    fn resize_with_scale(&mut self, width: u32, height: u32, scale: f64) {
        match self {
            Self::Softbuffer(target) => target.resize(width, height),
            Self::Wgpu(target) => target.resize(width, height, scale),
        }
    }

    fn present_rgba(&mut self, image: &RgbaImage, timing: &mut dyn RendererTimingRecorder) {
        match self {
            Self::Softbuffer(target) => target.present_rgba(image, timing),
            Self::Wgpu(target) => target.present_rgba(image, timing),
        }
    }

    fn matches_size(&self, width: u32, height: u32) -> bool {
        match self {
            Self::Softbuffer(target) => target.width == width && target.height == height,
            Self::Wgpu(target) => target.width == width && target.height == height,
        }
    }

    fn presenter_name(&self) -> &'static str {
        match self {
            Self::Softbuffer(_) => "Softbuffer",
            Self::Wgpu(_) => "subduction present surface",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WindowRenderer for ThreadedWindowRenderer {
    fn resize(&mut self, width: u32, height: u32, scale: f64) {
        match &mut self.presenter {
            ThreadedRendererPresenter::Cpu(target) => {
                target.resize_with_scale(width, height, scale)
            }
            ThreadedRendererPresenter::Gpu(target) => target.resize(width, height, scale),
        }
    }

    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        _frame_id: u64,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        let (scene, scene_span) = Self::record_scene(source);
        let render_start = Instant::now();
        let rendered = self.worker.render_sync(scene, size).is_some();
        let end = Instant::now();
        timing.record_span("Scene", Some(scene_span), TimingKind::Paint);
        timing.record_thread_span(
            "Render",
            Some(TimingSpan::new(render_start, end)),
            TimingKind::Renderer,
        );
        rendered
    }

    fn render_immediate_and_present(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        if self.present_completed_frame(timing) {
            return true;
        }
        if self.async_render_in_flight {
            return false;
        }

        let (scene, scene_span) = Self::record_scene(source);
        timing.record_span("Paint", Some(scene_span), TimingKind::Paint);
        timing.record_span("Scene", Some(scene_span), TimingKind::Paint);
        self.submit_async_render(self.window_id, scene, size);
        false
    }

    fn render_offscreen_frame(&mut self, scene: Scene, size: Size) -> Option<RenderedFrame> {
        self.worker.render_sync(scene, size)
    }

    fn capture(&mut self, size: Size, source: &mut dyn RenderSource) -> CaptureOutput {
        self.worker.capture(OffscreenRenderJob {
            scene: Self::record_scene(source).0,
            size,
        })
    }

    fn debug_info(&mut self) -> String {
        let presenter = match &self.presenter {
            ThreadedRendererPresenter::Cpu(target) => target.presenter_name(),
            ThreadedRendererPresenter::Gpu(_) => "subduction present surface",
        };
        format!("Renderer: {} (threaded, {presenter})", self.name)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn accept_rendered_frame(&mut self, frame: RenderedFrame, render_span: TimingSpan) {
        self.async_render_in_flight = false;
        self.completed_frame = Some((frame, render_span));
    }

    fn render_in_flight(&self) -> bool {
        self.async_render_in_flight
    }

    fn has_completed_frame(&self) -> bool {
        self.completed_frame.is_some()
    }
}

#[cfg(target_arch = "wasm32")]
impl WindowRenderer for TargetGpuWindowRenderer {
    fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.target.resize(width, height, scale);
    }

    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        _frame_id: u64,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        let mut source = TimedRenderSource::new(source);
        let render_start = Instant::now();
        let rendered = self.render_texture(size, &mut source).is_some();
        let end = Instant::now();
        timing.record_span("Scene", source.span, TimingKind::Paint);
        timing.record_span(
            "Render",
            Some(TimingSpan::new(render_start, end)),
            TimingKind::Renderer,
        );
        rendered
    }

    fn render_immediate_and_present(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        let mut source = TimedRenderSource::new(source);
        let render_start = Instant::now();
        let Some(texture) = self.render_texture(size, &mut source) else {
            return false;
        };
        let render_end = Instant::now();
        timing.record_span("Paint", source.span, TimingKind::Paint);
        timing.record_span("Scene", source.span, TimingKind::Paint);
        timing.record_span(
            "Render",
            Some(TimingSpan::new(render_start, render_end)),
            TimingKind::Renderer,
        );

        let start = Instant::now();
        let Some(surface_frame) = self.target.surface.try_acquire_frame() else {
            return false;
        };
        let compose_start = Instant::now();
        self.target
            .copy_texture_to_surface(&texture, surface_frame.texture());
        let present_call_start = Instant::now();
        surface_frame.present_after_submit(
            &self.target.queue,
            subduction_core::timing::PresentPacing::AsSoonAsPossible,
        );
        let end = Instant::now();
        timing.record_span(
            "Present",
            Some(TimingSpan::new(start, end)),
            TimingKind::Present,
        );
        timing.record_span(
            "Compose",
            Some(TimingSpan::new(compose_start, present_call_start)),
            TimingKind::Present,
        );
        timing.record_span(
            "PresentCall",
            Some(TimingSpan::new(present_call_start, end)),
            TimingKind::Present,
        );
        true
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
}

#[cfg(target_arch = "wasm32")]
impl TargetGpuWindowRenderer {
    fn render_texture(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
    ) -> Option<wgpu::Texture> {
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let texture = self.target.create_render_texture(width, height);
        match (&mut self.backend.backend, &self.backend.output) {
            (GpuRendererBackend::Texture(backend), GpuOutputMode::ProvidedTarget) => {
                backend
                    .render_source_into_texture(source, texture.clone())
                    .expect("failed to render gpu target");
                Some(texture)
            }
            (GpuRendererBackend::TextureView(backend), GpuOutputMode::ProvidedTarget) => {
                let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                backend
                    .render_source_into_texture(
                        source,
                        TextureViewTarget::new(&texture_view, width, height),
                    )
                    .expect("failed to render gpu target");
                Some(texture)
            }
            (GpuRendererBackend::Texture(backend), GpuOutputMode::OwnedTexture) => {
                backend.render_source_texture(source, width, height).ok()
            }
            (GpuRendererBackend::TextureView(backend), GpuOutputMode::OwnedTexture) => {
                backend.render_source_texture(source, width, height).ok()
            }
        }
    }
}

#[allow(
    dead_code,
    reason = "CPU image rendering may be unused when no CPU renderer is enabled in the current build."
)]
impl WindowRenderer for ImageWindowRenderer {
    fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.target.resize_with_scale(width, height, scale);
    }

    fn render(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        _frame_id: u64,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        let mut source = TimedRenderSource::new(source);
        let render_start = Instant::now();
        let rendered = self.render_image(size, &mut source);
        let end = Instant::now();
        timing.record_span("Scene", source.span, TimingKind::Paint);
        timing.record_span(
            "Render",
            Some(TimingSpan::new(render_start, end)),
            TimingKind::Renderer,
        );
        rendered.is_some()
    }

    fn render_immediate_and_present(
        &mut self,
        size: Size,
        source: &mut dyn RenderSource,
        timing: &mut dyn RendererTimingRecorder,
    ) -> bool {
        let mut source = TimedRenderSource::new(source);
        let render_start = Instant::now();
        let Some(image) = self.render_image(size, &mut source) else {
            return false;
        };
        let render_end = Instant::now();
        timing.record_span("Paint", source.span, TimingKind::Paint);
        timing.record_span("Scene", source.span, TimingKind::Paint);
        timing.record_span(
            "Render",
            Some(TimingSpan::new(render_start, render_end)),
            TimingKind::Renderer,
        );
        self.target.present_rgba(&image, timing);
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn render_offscreen_frame(&mut self, mut scene: Scene, size: Size) -> Option<RenderedFrame> {
        self.render_image(size, &mut scene)
            .map(|image| RenderedFrame::Cpu(RenderedImageFrame { image }))
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

    fn debug_info(&mut self) -> String {
        format!(
            "Renderer: {} ({})",
            self.backend.name,
            self.target.presenter_name()
        )
    }
}

impl ImageWindowRenderer {
    fn render_image(&mut self, size: Size, source: &mut dyn RenderSource) -> Option<RgbaImage> {
        let width = size.width.max(1.0) as u32;
        let height = size.height.max(1.0) as u32;
        let mut image = RgbaImage::new(width, height);
        self.backend
            .backend
            .render_source_into(source, ImageBufferTarget::from_rgba_image(&mut image))
            .ok()
            .map(|_| image)
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

#[allow(
    dead_code,
    reason = "Used only when a GPU renderer feature with provided texture targets is enabled."
)]
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
