//! Compositor-owned surface slots.
//!
//! A [`CompositorSurface`] is content that Floem/Subduction owns well enough to
//! either promote into a platform compositor layer or sample through Imaging
//! when a clip, mask, filter, effect, or group forces flattening. This is the
//! right primitive for embedded renderers, video-like producers, camera frames,
//! and any surface that must remain visually correct when direct layer
//! promotion is not legal.

use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use peniko::{
    ImageData,
    kurbo::{Rect, Size},
};
use winit::window::WindowId;

use crate::{
    Application,
    app::UserEvent,
    frame::{FrameOutcome, PresentationInterval},
    gpu_resources::GpuResources,
};

static NEXT_COMPOSITOR_SURFACE_ID: AtomicU64 = AtomicU64::new(1);

/// Stable window-local identity for a compositor-owned surface slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompositorSurfaceId(u64);

impl CompositorSurfaceId {
    pub(crate) fn next() -> Self {
        Self(NEXT_COMPOSITOR_SURFACE_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn image_id(self) -> imaging::ExternalImageId {
        imaging::ExternalImageId(self.0 | COMPOSITOR_SURFACE_IMAGE_ID_MASK)
    }

    #[must_use]
    pub fn from_image_id(image_id: imaging::ExternalImageId) -> Option<Self> {
        (image_id.0 & COMPOSITOR_SURFACE_IMAGE_ID_MASK != 0)
            .then_some(Self(image_id.0 & !COMPOSITOR_SURFACE_IMAGE_ID_MASK))
    }

    #[cfg(test)]
    pub(crate) fn test_new(value: u64) -> Self {
        Self(value)
    }
}

const COMPOSITOR_SURFACE_IMAGE_ID_MASK: u64 = 1 << 63;

/// A Floem/Subduction-owned compositor surface.
///
/// The surface has stable identity and content, but placement still comes from
/// paint order. Use [`CompositorSurface::image`] to place the surface through
/// the normal Imaging brush path; display-list lowering may promote that brush
/// back to a compositor layer when it is safe.
///
/// Unlike [`crate::external_surface::ExternalSurface`], this path may flatten:
/// Floem can resolve the submitted content as an external image and render it
/// into an intermediate pass when correctness requires it.
#[derive(Clone, Debug)]
pub struct CompositorSurface {
    id: CompositorSurfaceId,
    window_id: WindowId,
    config: CompositorSurfaceConfig,
}

impl CompositorSurface {
    #[must_use]
    pub fn new(window_id: WindowId, config: CompositorSurfaceConfig) -> Self {
        Self {
            id: CompositorSurfaceId::next(),
            window_id,
            config,
        }
    }

    #[must_use]
    pub fn id(&self) -> CompositorSurfaceId {
        self.id
    }

    /// Creates an Imaging external image handle for this surface.
    ///
    /// The returned image can be used with `imaging::ImageBrush`. If the brush
    /// is used in a simple promotable fill, Floem may publish the surface
    /// directly as a compositor layer. If active group state requires
    /// flattening, the renderer samples the same submitted surface content.
    #[must_use]
    pub fn image(&self, width: u32, height: u32) -> imaging::ExternalImage {
        imaging::ExternalImage::new(
            self.id.image_id(),
            width,
            height,
            peniko::ImageAlphaType::AlphaPremultiplied,
        )
    }

    #[must_use]
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub fn config(&self) -> &CompositorSurfaceConfig {
        &self.config
    }

    #[must_use]
    pub fn handle(&self) -> CompositorSurfaceHandle {
        CompositorSurfaceHandle {
            id: self.id,
            window_id: self.window_id,
        }
    }

    pub fn submit_texture(&self, texture: ExternalTexture) {
        self.handle().submit_texture(texture);
    }

    pub fn submit_image(&self, image: ImageData) {
        self.handle().submit_image(image);
    }

    pub fn clear(&self) {
        self.handle().clear();
    }

    pub fn request_frame(&self) {
        self.handle().request_frame();
    }

    pub fn set_provider(&self, provider: CompositorSurfaceProviderHandle) {
        self.handle().set_provider(provider);
    }

    /// Creates a surface with a frame-opportunity driven wgpu render target.
    ///
    /// The returned [`RenderableCompositorSurface`] owns the producer callback.
    /// On each frame opportunity the callback may acquire a Subduction-managed
    /// target, render into it, and complete asynchronously. Floem then either
    /// publishes that content directly or samples it during compositor
    /// flattening.
    #[must_use]
    pub fn new_renderable(
        window_id: WindowId,
        size: Size,
        config: RenderableCompositorSurfaceConfig,
    ) -> (Self, RenderableCompositorSurface) {
        let surface = Self::new(
            window_id,
            CompositorSurfaceConfig {
                kind: CompositorSurfaceKind::WgpuTexture,
                alpha_mode: config.alpha_mode,
                preferred_size: Some(size),
            },
        );
        let renderable = RenderableCompositorSurface::new(surface.handle(), size, config);
        surface.set_provider(Arc::new(Mutex::new(renderable.provider())));
        (surface, renderable)
    }
}

/// Configuration for a frame-opportunity driven compositor surface.
#[derive(Clone, Debug)]
pub struct RenderableCompositorSurfaceConfig {
    /// Subduction wgpu target configuration, including latency and format
    /// preferences.
    pub surface: subduction::wgpu::ExternalSurfaceConfig,
    /// Alpha interpretation for the published content.
    pub alpha_mode: CompositorSurfaceAlphaMode,
}

impl Default for RenderableCompositorSurfaceConfig {
    fn default() -> Self {
        Self {
            surface: subduction::wgpu::ExternalSurfaceConfig::default(),
            alpha_mode: CompositorSurfaceAlphaMode::Premultiplied,
        }
    }
}

/// Producer-side handle for rendering into a compositor-owned wgpu target.
#[derive(Clone)]
pub struct RenderableCompositorSurface {
    handle: CompositorSurfaceHandle,
    preferred_size: Size,
    config: RenderableCompositorSurfaceConfig,
    callback: Arc<Mutex<Option<RenderableCompositorSurfaceCallback>>>,
    completions: Arc<Mutex<mpsc::Receiver<subduction::wgpu::SurfaceFrameCompletion>>>,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
}

impl std::fmt::Debug for RenderableCompositorSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderableCompositorSurface")
            .field("id", &self.handle.id())
            .field("preferred_size", &self.preferred_size)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

type RenderableCompositorSurfaceCallback = Box<
    dyn FnMut(
            RenderableCompositorSurfaceFrameCx,
        )
            -> Result<subduction::wgpu::SurfaceFrameDecision, subduction::wgpu::SurfaceFrameError>
        + Send,
>;

impl RenderableCompositorSurface {
    fn new(
        handle: CompositorSurfaceHandle,
        preferred_size: Size,
        config: RenderableCompositorSurfaceConfig,
    ) -> Self {
        let (completion_tx, completion_rx) = mpsc::channel();
        Self {
            handle,
            preferred_size,
            config,
            callback: Arc::new(Mutex::new(None)),
            completions: Arc::new(Mutex::new(completion_rx)),
            completion_tx,
            in_flight: Arc::new(Mutex::new(0)),
        }
    }

    #[must_use]
    pub fn surface_id(&self) -> CompositorSurfaceId {
        self.handle.id()
    }

    pub fn set_frame_callback(
        &self,
        callback: impl FnMut(
            RenderableCompositorSurfaceFrameCx,
        ) -> Result<
            subduction::wgpu::SurfaceFrameDecision,
            subduction::wgpu::SurfaceFrameError,
        > + Send
        + 'static,
    ) {
        *self.callback.lock().unwrap() = Some(Box::new(callback));
        self.handle.request_frame();
    }

    #[must_use]
    pub fn completion_sender(&self) -> mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion> {
        self.completion_tx.clone()
    }

    fn provider(&self) -> RenderableCompositorSurfaceProvider {
        RenderableCompositorSurfaceProvider {
            handle: self.handle.clone(),
            config: self.config.clone(),
            callback: self.callback.clone(),
            completions: self.completions.clone(),
            completion_tx: self.completion_tx.clone(),
            in_flight: self.in_flight.clone(),
            current_content: None,
            last_requested_frame_index: None,
        }
    }
}

/// Per-frame render context passed to a renderable compositor surface callback.
///
/// `acquire_target` is non-blocking. It fails when no compositor target is
/// available or when the configured max frame latency is already in flight.
pub struct RenderableCompositorSurfaceFrameCx {
    opportunity: subduction::wgpu::SurfaceFrameOpportunity,
    config: RenderableCompositorSurfaceConfig,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
    target: Option<subduction::wgpu::SurfaceFrameLease>,
}

impl RenderableCompositorSurfaceFrameCx {
    #[must_use]
    pub fn opportunity(&self) -> subduction::wgpu::SurfaceFrameOpportunity {
        self.opportunity
    }

    #[must_use]
    pub fn config(&self) -> &RenderableCompositorSurfaceConfig {
        &self.config
    }

    #[must_use]
    pub fn completion_sender(&self) -> mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion> {
        self.completion_tx.clone()
    }

    pub fn acquire_target(
        &mut self,
    ) -> Result<subduction::wgpu::SurfaceFrameLease, subduction::wgpu::SurfaceFrameError> {
        if self.target.is_none() {
            return Err(subduction::wgpu::SurfaceFrameError::Unsupported);
        }
        let max_latency = self.config.surface.max_frame_latency.max(1);
        {
            let mut in_flight = self.in_flight.lock().unwrap();
            if *in_flight >= max_latency {
                return Err(subduction::wgpu::SurfaceFrameError::NoTargetAvailable);
            }
            *in_flight += 1;
        }
        if let Some(target) = self.target.take() {
            return Ok(target);
        }
        unreachable!("target existence was checked before in-flight reservation")
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
pub struct SubductionFrameTick {
    pub received_at: Instant,
    pub frame_index: u64,
    pub refresh_interval: Option<Duration>,
    pub predicted_present: Option<Instant>,
    pub display_timing: Option<crate::frame::DisplayTiming>,
}

/// Preferred storage/presentation class for compositor surface content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorSurfaceKind {
    /// Native platform texture or layer-like content, when available.
    NativeTexture,
    /// Subduction-managed wgpu texture content.
    WgpuTexture,
    /// CPU image fallback content.
    CpuImageFallback,
}

/// Alpha mode for compositor surface content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorSurfaceAlphaMode {
    Opaque,
    Premultiplied,
    Unpremultiplied,
}

/// Configuration for a compositor-owned surface slot.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositorSurfaceConfig {
    pub kind: CompositorSurfaceKind,
    pub alpha_mode: CompositorSurfaceAlphaMode,
    pub preferred_size: Option<Size>,
}

impl CompositorSurfaceConfig {
    #[must_use]
    pub fn texture() -> Self {
        Self {
            kind: CompositorSurfaceKind::WgpuTexture,
            alpha_mode: CompositorSurfaceAlphaMode::Premultiplied,
            preferred_size: None,
        }
    }

    #[must_use]
    pub fn video() -> Self {
        Self {
            kind: CompositorSurfaceKind::NativeTexture,
            alpha_mode: CompositorSurfaceAlphaMode::Opaque,
            preferred_size: None,
        }
    }
}

impl Default for CompositorSurfaceConfig {
    fn default() -> Self {
        Self::texture()
    }
}

/// Opaque texture-like payload submitted to a compositor surface.
///
/// The payload is intentionally type-erased so producer/backends can pass
/// backend-specific handles without making Floem platform-specific. Direct
/// external surfaces validate this payload synchronously before accepting it.
#[derive(Clone, Debug)]
pub struct ExternalTexture {
    pub size: Size,
    pub payload: Arc<dyn Any + Send + Sync>,
}

impl ExternalTexture {
    #[must_use]
    pub fn new(size: Size, payload: impl Any + Send + Sync) -> Self {
        Self {
            size,
            payload: Arc::new(payload),
        }
    }
}

/// Current content for a compositor-owned surface slot.
#[derive(Clone, Debug)]
pub enum CompositorSurfaceContent {
    Empty,
    Texture(ExternalTexture),
    /// Opaque platform layer. Floem only stores and orders it; Subduction owns
    /// platform-specific attachment.
    NativeLayer(subduction::NativeLayer),
    Image(ImageData),
    Subduction(Arc<dyn Any + Send + Sync>),
}

pub type CompositorSurfaceProviderHandle = Arc<Mutex<dyn CompositorSurfaceProvider + Send>>;

/// Producer interface used by Floem to request and poll compositor content.
///
/// Providers should avoid blocking the UI thread. If a frame cannot be
/// produced immediately, return a deferred update and complete through the
/// configured completion channel.
pub trait CompositorSurfaceProvider {
    fn can_accept_frame_target(&self) -> bool {
        true
    }

    fn poll_current_content(&mut self) -> CompositorSurfaceFrameUpdate {
        CompositorSurfaceFrameUpdate::idle()
    }

    fn update_current_content(
        &mut self,
        args: CompositorSurfaceFrameArgs,
    ) -> CompositorSurfaceFrameUpdate;

    fn current_content(&self) -> Option<CompositorSurfaceContent>;

    fn release_current_content(&mut self, outcome: CompositorSurfaceOutcome);
}

struct RenderableCompositorSurfaceProvider {
    handle: CompositorSurfaceHandle,
    config: RenderableCompositorSurfaceConfig,
    callback: Arc<Mutex<Option<RenderableCompositorSurfaceCallback>>>,
    completions: Arc<Mutex<mpsc::Receiver<subduction::wgpu::SurfaceFrameCompletion>>>,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
    current_content: Option<CompositorSurfaceContent>,
    last_requested_frame_index: Option<u64>,
}

impl CompositorSurfaceProvider for RenderableCompositorSurfaceProvider {
    fn can_accept_frame_target(&self) -> bool {
        if self.callback.lock().unwrap().is_none() {
            return false;
        }
        let max_latency = self.config.surface.max_frame_latency.max(1);
        let in_flight = self.in_flight.lock().unwrap();
        *in_flight < max_latency
    }

    fn poll_current_content(&mut self) -> CompositorSurfaceFrameUpdate {
        self.drain_completions()
    }

    fn update_current_content(
        &mut self,
        args: CompositorSurfaceFrameArgs,
    ) -> CompositorSurfaceFrameUpdate {
        let mut frame_update = self.drain_completions();
        let diag = crate::frame_clock::frame_pacing_diag_enabled()
            || std::env::var_os("FLOEM_CUBE_DIAG").is_some();

        if !args.visible {
            if diag {
                eprintln!(
                    "floem compositor surface provider skip surface={:?} frame={} reason=not_visible",
                    args.surface_id, args.frame_index,
                );
            }
            return CompositorSurfaceFrameUpdate {
                content_changed: frame_update.content_changed,
                request_next_frame: false,
            };
        }

        if args.gpu_resources.is_none() {
            if diag {
                eprintln!(
                    "floem compositor surface provider skip surface={:?} frame={} reason=no_gpu_resources",
                    args.surface_id, args.frame_index,
                );
            }
            return CompositorSurfaceFrameUpdate {
                content_changed: frame_update.content_changed,
                request_next_frame: true,
            };
        }

        if self.last_requested_frame_index == Some(args.frame_index) {
            if diag {
                eprintln!(
                    "floem compositor surface provider skip surface={:?} frame={} reason=already_requested content_changed={}",
                    args.surface_id, args.frame_index, frame_update.content_changed,
                );
            }
            frame_update.request_next_frame = true;
            return frame_update;
        }

        if diag {
            eprintln!(
                "floem compositor surface provider opportunity surface={:?} frame={} rect={:?} size_px={:.1}x{:.1} has_target={}",
                args.surface_id,
                args.frame_index,
                args.rect,
                args.size_px.width,
                args.size_px.height,
                args.target.is_some(),
            );
        }

        let opportunity = subduction::wgpu::SurfaceFrameOpportunity {
            surface_id: subduction_core::layer::SurfaceId(args.surface_id.get() as u32),
            frame_index: args.frame_index,
            now: subduction_core::time::HostTime(0),
            target_timestamp: None,
            target_present: None,
            previous_present: None,
            refresh_interval: None,
            confidence: subduction_core::timing::TimingConfidence::PacingOnly,
        };
        let cx = RenderableCompositorSurfaceFrameCx {
            opportunity,
            config: self.config.clone(),
            completion_tx: self.completion_tx.clone(),
            in_flight: self.in_flight.clone(),
            target: args.target,
        };

        let decision = self
            .callback
            .lock()
            .unwrap()
            .as_mut()
            .map(|callback| callback(cx));
        let request_next_frame = match decision {
            Some(Ok(subduction::wgpu::SurfaceFrameDecision::Deferred)) => {
                self.last_requested_frame_index = Some(args.frame_index);
                if diag {
                    eprintln!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=deferred",
                        args.surface_id, args.frame_index,
                    );
                }
                true
            }
            Some(Ok(subduction::wgpu::SurfaceFrameDecision::Skip(reason))) => {
                if diag {
                    eprintln!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=skip reason={reason:?}",
                        args.surface_id, args.frame_index,
                    );
                }
                false
            }
            Some(Err(err)) => {
                if diag {
                    eprintln!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=error error={err:?}",
                        args.surface_id, args.frame_index,
                    );
                }
                true
            }
            None => {
                if diag {
                    eprintln!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=no_callback",
                        args.surface_id, args.frame_index,
                    );
                }
                false
            }
        };

        if request_next_frame {
            self.handle.request_frame();
        }

        frame_update.request_next_frame |= request_next_frame;
        frame_update
    }

    fn current_content(&self) -> Option<CompositorSurfaceContent> {
        self.current_content.clone()
    }

    fn release_current_content(&mut self, _outcome: CompositorSurfaceOutcome) {}
}

impl RenderableCompositorSurfaceProvider {
    fn drain_completions(&mut self) -> CompositorSurfaceFrameUpdate {
        let diag = crate::frame_clock::frame_pacing_diag_enabled()
            || std::env::var_os("FLOEM_CUBE_DIAG").is_some();
        let mut content_changed = false;
        while let Ok(completion) = self.completions.lock().unwrap().try_recv() {
            let mut in_flight = self.in_flight.lock().unwrap();
            *in_flight = in_flight.saturating_sub(1);
            drop(in_flight);
            match completion {
                subduction::wgpu::SurfaceFrameCompletion::Submitted(frame) => {
                    if diag {
                        eprintln!(
                            "floem compositor surface provider completion surface={:?} frame={} submitted size={}x{} resource_key={:?}",
                            self.handle.id(),
                            frame.opportunity.frame_index,
                            frame.size.width,
                            frame.size.height,
                            frame.resource_key,
                        );
                    }
                    let size = Size::new(f64::from(frame.size.width), f64::from(frame.size.height));
                    self.current_content = Some(CompositorSurfaceContent::Texture(
                        ExternalTexture::new(size, frame),
                    ));
                    content_changed = true;
                }
                subduction::wgpu::SurfaceFrameCompletion::Skipped(frame) => {
                    if diag {
                        eprintln!(
                            "floem compositor surface provider completion surface={:?} frame={} skipped reason={:?}",
                            self.handle.id(),
                            frame.opportunity.frame_index,
                            frame.reason,
                        );
                    }
                }
            }
        }
        CompositorSurfaceFrameUpdate {
            content_changed,
            request_next_frame: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompositorSurfaceFrameUpdate {
    pub content_changed: bool,
    pub request_next_frame: bool,
}

impl CompositorSurfaceFrameUpdate {
    #[must_use]
    pub fn idle() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn content_changed() -> Self {
        Self {
            content_changed: true,
            request_next_frame: false,
        }
    }

    #[must_use]
    pub fn request_next_frame() -> Self {
        Self {
            content_changed: false,
            request_next_frame: true,
        }
    }

    #[must_use]
    pub fn content_changed_and_request_next_frame() -> Self {
        Self {
            content_changed: true,
            request_next_frame: true,
        }
    }
}

#[derive(Debug)]
pub struct CompositorSurfaceFrameArgs {
    pub surface_id: CompositorSurfaceId,
    pub frame_index: u64,
    pub interval: PresentationInterval,
    pub visible: bool,
    pub rect: Rect,
    pub size_px: Size,
    pub gpu_resources: Option<GpuResources>,
    pub target: Option<subduction::wgpu::SurfaceFrameLease>,
    pub previous_outcome: Option<CompositorSurfaceOutcome>,
}

#[derive(Clone, Copy, Debug)]
pub struct CompositorSurfaceOutcome {
    pub surface_id: CompositorSurfaceId,
    pub frame_index: u64,
    pub visible: bool,
    pub outcome: FrameOutcome,
}

#[derive(Clone, Debug)]
pub struct CompositorSurfaceHandle {
    id: CompositorSurfaceId,
    window_id: WindowId,
}

impl CompositorSurfaceHandle {
    #[must_use]
    pub fn id(&self) -> CompositorSurfaceId {
        self.id
    }

    pub fn submit_texture(&self, texture: ExternalTexture) {
        self.submit(CompositorSurfaceContent::Texture(texture));
    }

    pub fn submit_image(&self, image: ImageData) {
        self.submit(CompositorSurfaceContent::Image(image));
    }

    pub fn submit_subduction_surface(&self, surface: impl Any + Send + Sync) {
        self.submit(CompositorSurfaceContent::Subduction(Arc::new(surface)));
    }

    pub fn submit_subduction_surface_arc(&self, surface: Arc<dyn Any + Send + Sync>) {
        self.submit(CompositorSurfaceContent::Subduction(surface));
    }

    pub fn clear(&self) {
        self.submit(CompositorSurfaceContent::Empty);
    }

    pub fn request_frame(&self) {
        Application::send_proxy_event(UserEvent::CompositorSurfaceRequestFrame {
            window_id: self.window_id,
            surface_id: self.id,
        });
    }

    pub fn set_provider(&self, provider: CompositorSurfaceProviderHandle) {
        Application::send_proxy_event(UserEvent::CompositorSurfaceProvider {
            window_id: self.window_id,
            surface_id: self.id,
            provider,
        });
    }

    fn submit(&self, content: CompositorSurfaceContent) {
        Application::send_proxy_event(UserEvent::CompositorSurfaceContent {
            window_id: self.window_id,
            surface_id: self.id,
            content,
        });
    }
}
