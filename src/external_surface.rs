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

static NEXT_EXTERNAL_SURFACE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalSurfaceId(u64);

impl ExternalSurfaceId {
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn image_id(self) -> imaging::ExternalImageId {
        imaging::ExternalImageId(self.0 | EXTERNAL_SURFACE_IMAGE_ID_MASK)
    }

    #[must_use]
    pub fn from_image_id(image_id: imaging::ExternalImageId) -> Option<Self> {
        (image_id.0 & EXTERNAL_SURFACE_IMAGE_ID_MASK != 0)
            .then_some(Self(image_id.0 & !EXTERNAL_SURFACE_IMAGE_ID_MASK))
    }

    #[cfg(test)]
    pub(crate) fn test_new(value: u64) -> Self {
        Self(value)
    }
}

const EXTERNAL_SURFACE_IMAGE_ID_MASK: u64 = 1 << 63;

#[derive(Clone, Debug)]
pub struct ExternalSurface {
    id: ExternalSurfaceId,
    window_id: WindowId,
    config: ExternalSurfaceConfig,
}

impl ExternalSurface {
    #[must_use]
    pub fn new(window_id: WindowId, config: ExternalSurfaceConfig) -> Self {
        Self {
            id: ExternalSurfaceId(NEXT_EXTERNAL_SURFACE_ID.fetch_add(1, Ordering::Relaxed)),
            window_id,
            config,
        }
    }

    #[must_use]
    pub fn id(&self) -> ExternalSurfaceId {
        self.id
    }

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
    pub fn config(&self) -> &ExternalSurfaceConfig {
        &self.config
    }

    #[must_use]
    pub fn handle(&self) -> ExternalSurfaceHandle {
        ExternalSurfaceHandle {
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

    pub fn set_provider(&self, provider: ExternalSurfaceProviderHandle) {
        self.handle().set_provider(provider);
    }

    #[must_use]
    pub fn new_renderable(
        window_id: WindowId,
        size: Size,
        config: RenderableExternalSurfaceConfig,
    ) -> (Self, RenderableExternalSurface) {
        let surface = Self::new(
            window_id,
            ExternalSurfaceConfig {
                kind: ExternalSurfaceKind::WgpuTexture,
                alpha_mode: config.alpha_mode,
                preferred_size: Some(size),
            },
        );
        let renderable = RenderableExternalSurface::new(surface.handle(), size, config);
        surface.set_provider(Arc::new(Mutex::new(renderable.provider())));
        (surface, renderable)
    }
}

#[derive(Clone, Debug)]
pub struct RenderableExternalSurfaceConfig {
    pub surface: subduction::wgpu::ExternalSurfaceConfig,
    pub alpha_mode: ExternalSurfaceAlphaMode,
}

impl Default for RenderableExternalSurfaceConfig {
    fn default() -> Self {
        Self {
            surface: subduction::wgpu::ExternalSurfaceConfig::default(),
            alpha_mode: ExternalSurfaceAlphaMode::Premultiplied,
        }
    }
}

#[derive(Clone)]
pub struct RenderableExternalSurface {
    handle: ExternalSurfaceHandle,
    preferred_size: Size,
    config: RenderableExternalSurfaceConfig,
    callback: Arc<Mutex<Option<RenderableExternalSurfaceCallback>>>,
    completions: Arc<Mutex<mpsc::Receiver<subduction::wgpu::SurfaceFrameCompletion>>>,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
}

impl std::fmt::Debug for RenderableExternalSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderableExternalSurface")
            .field("id", &self.handle.id())
            .field("preferred_size", &self.preferred_size)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

type RenderableExternalSurfaceCallback = Box<
    dyn FnMut(
            RenderableExternalSurfaceFrameCx,
        )
            -> Result<subduction::wgpu::SurfaceFrameDecision, subduction::wgpu::SurfaceFrameError>
        + Send,
>;

impl RenderableExternalSurface {
    fn new(
        handle: ExternalSurfaceHandle,
        preferred_size: Size,
        config: RenderableExternalSurfaceConfig,
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
    pub fn surface_id(&self) -> ExternalSurfaceId {
        self.handle.id()
    }

    pub fn set_frame_callback(
        &self,
        callback: impl FnMut(
            RenderableExternalSurfaceFrameCx,
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

    fn provider(&self) -> RenderableExternalSurfaceProvider {
        RenderableExternalSurfaceProvider {
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

pub struct RenderableExternalSurfaceFrameCx {
    opportunity: subduction::wgpu::SurfaceFrameOpportunity,
    config: RenderableExternalSurfaceConfig,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
    target: Option<subduction::wgpu::SurfaceFrameLease>,
}

impl RenderableExternalSurfaceFrameCx {
    #[must_use]
    pub fn opportunity(&self) -> subduction::wgpu::SurfaceFrameOpportunity {
        self.opportunity
    }

    #[must_use]
    pub fn config(&self) -> &RenderableExternalSurfaceConfig {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalSurfaceKind {
    NativeTexture,
    WgpuTexture,
    CpuImageFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalSurfaceAlphaMode {
    Opaque,
    Premultiplied,
    Unpremultiplied,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalSurfaceConfig {
    pub kind: ExternalSurfaceKind,
    pub alpha_mode: ExternalSurfaceAlphaMode,
    pub preferred_size: Option<Size>,
}

impl ExternalSurfaceConfig {
    #[must_use]
    pub fn texture() -> Self {
        Self {
            kind: ExternalSurfaceKind::WgpuTexture,
            alpha_mode: ExternalSurfaceAlphaMode::Premultiplied,
            preferred_size: None,
        }
    }

    #[must_use]
    pub fn video() -> Self {
        Self {
            kind: ExternalSurfaceKind::NativeTexture,
            alpha_mode: ExternalSurfaceAlphaMode::Opaque,
            preferred_size: None,
        }
    }
}

impl Default for ExternalSurfaceConfig {
    fn default() -> Self {
        Self::texture()
    }
}

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

#[derive(Clone, Debug)]
pub enum ExternalSurfaceContent {
    Empty,
    Texture(ExternalTexture),
    Image(ImageData),
    Subduction(Arc<dyn Any + Send + Sync>),
}

pub type ExternalSurfaceProviderHandle = Arc<Mutex<dyn ExternalSurfaceProvider + Send>>;

pub trait ExternalSurfaceProvider {
    fn can_accept_frame_target(&self) -> bool {
        true
    }

    fn poll_current_content(&mut self) -> ExternalSurfaceFrameUpdate {
        ExternalSurfaceFrameUpdate::idle()
    }

    fn update_current_content(
        &mut self,
        args: ExternalSurfaceFrameArgs,
    ) -> ExternalSurfaceFrameUpdate;

    fn current_content(&self) -> Option<ExternalSurfaceContent>;

    fn release_current_content(&mut self, outcome: ExternalSurfaceOutcome);
}

struct RenderableExternalSurfaceProvider {
    handle: ExternalSurfaceHandle,
    config: RenderableExternalSurfaceConfig,
    callback: Arc<Mutex<Option<RenderableExternalSurfaceCallback>>>,
    completions: Arc<Mutex<mpsc::Receiver<subduction::wgpu::SurfaceFrameCompletion>>>,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
    current_content: Option<ExternalSurfaceContent>,
    last_requested_frame_index: Option<u64>,
}

impl ExternalSurfaceProvider for RenderableExternalSurfaceProvider {
    fn can_accept_frame_target(&self) -> bool {
        if self.callback.lock().unwrap().is_none() {
            return false;
        }
        let max_latency = self.config.surface.max_frame_latency.max(1);
        let in_flight = self.in_flight.lock().unwrap();
        *in_flight < max_latency
    }

    fn poll_current_content(&mut self) -> ExternalSurfaceFrameUpdate {
        self.drain_completions()
    }

    fn update_current_content(
        &mut self,
        args: ExternalSurfaceFrameArgs,
    ) -> ExternalSurfaceFrameUpdate {
        let mut frame_update = self.drain_completions();
        let diag = crate::frame_clock::frame_pacing_diag_enabled()
            || std::env::var_os("FLOEM_CUBE_DIAG").is_some();

        if !args.visible {
            if diag {
                eprintln!(
                    "floem external surface provider skip surface={:?} frame={} reason=not_visible",
                    args.surface_id, args.frame_index,
                );
            }
            return ExternalSurfaceFrameUpdate {
                content_changed: frame_update.content_changed,
                request_next_frame: false,
            };
        }

        if args.gpu_resources.is_none() {
            if diag {
                eprintln!(
                    "floem external surface provider skip surface={:?} frame={} reason=no_gpu_resources",
                    args.surface_id, args.frame_index,
                );
            }
            return ExternalSurfaceFrameUpdate {
                content_changed: frame_update.content_changed,
                request_next_frame: true,
            };
        }

        if self.last_requested_frame_index == Some(args.frame_index) {
            if diag {
                eprintln!(
                    "floem external surface provider skip surface={:?} frame={} reason=already_requested content_changed={}",
                    args.surface_id, args.frame_index, frame_update.content_changed,
                );
            }
            frame_update.request_next_frame = true;
            return frame_update;
        }

        if diag {
            eprintln!(
                "floem external surface provider opportunity surface={:?} frame={} rect={:?} size_px={:.1}x{:.1} has_target={}",
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
        let cx = RenderableExternalSurfaceFrameCx {
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
                        "floem external surface provider decision surface={:?} frame={} decision=deferred",
                        args.surface_id, args.frame_index,
                    );
                }
                true
            }
            Some(Ok(subduction::wgpu::SurfaceFrameDecision::Skip(reason))) => {
                if diag {
                    eprintln!(
                        "floem external surface provider decision surface={:?} frame={} decision=skip reason={reason:?}",
                        args.surface_id, args.frame_index,
                    );
                }
                false
            }
            Some(Err(err)) => {
                if diag {
                    eprintln!(
                        "floem external surface provider decision surface={:?} frame={} decision=error error={err:?}",
                        args.surface_id, args.frame_index,
                    );
                }
                true
            }
            None => {
                if diag {
                    eprintln!(
                        "floem external surface provider decision surface={:?} frame={} decision=no_callback",
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

    fn current_content(&self) -> Option<ExternalSurfaceContent> {
        self.current_content.clone()
    }

    fn release_current_content(&mut self, _outcome: ExternalSurfaceOutcome) {}
}

impl RenderableExternalSurfaceProvider {
    fn drain_completions(&mut self) -> ExternalSurfaceFrameUpdate {
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
                            "floem external surface provider completion surface={:?} frame={} submitted size={}x{} resource_key={:?}",
                            self.handle.id(),
                            frame.opportunity.frame_index,
                            frame.size.width,
                            frame.size.height,
                            frame.resource_key,
                        );
                    }
                    let size = Size::new(f64::from(frame.size.width), f64::from(frame.size.height));
                    self.current_content = Some(ExternalSurfaceContent::Texture(
                        ExternalTexture::new(size, frame),
                    ));
                    content_changed = true;
                }
                subduction::wgpu::SurfaceFrameCompletion::Skipped(frame) => {
                    if diag {
                        eprintln!(
                            "floem external surface provider completion surface={:?} frame={} skipped reason={:?}",
                            self.handle.id(),
                            frame.opportunity.frame_index,
                            frame.reason,
                        );
                    }
                }
            }
        }
        ExternalSurfaceFrameUpdate {
            content_changed,
            request_next_frame: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExternalSurfaceFrameUpdate {
    pub content_changed: bool,
    pub request_next_frame: bool,
}

impl ExternalSurfaceFrameUpdate {
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
pub struct ExternalSurfaceFrameArgs {
    pub surface_id: ExternalSurfaceId,
    pub frame_index: u64,
    pub interval: PresentationInterval,
    pub visible: bool,
    pub rect: Rect,
    pub size_px: Size,
    pub gpu_resources: Option<GpuResources>,
    pub target: Option<subduction::wgpu::SurfaceFrameLease>,
    pub previous_outcome: Option<ExternalSurfaceOutcome>,
}

#[derive(Clone, Copy, Debug)]
pub struct ExternalSurfaceOutcome {
    pub surface_id: ExternalSurfaceId,
    pub frame_index: u64,
    pub visible: bool,
    pub outcome: FrameOutcome,
}

#[derive(Clone, Debug)]
pub struct ExternalSurfaceHandle {
    id: ExternalSurfaceId,
    window_id: WindowId,
}

impl ExternalSurfaceHandle {
    #[must_use]
    pub fn id(&self) -> ExternalSurfaceId {
        self.id
    }

    pub fn submit_texture(&self, texture: ExternalTexture) {
        self.submit(ExternalSurfaceContent::Texture(texture));
    }

    pub fn submit_image(&self, image: ImageData) {
        self.submit(ExternalSurfaceContent::Image(image));
    }

    pub fn submit_subduction_surface(&self, surface: impl Any + Send + Sync) {
        self.submit(ExternalSurfaceContent::Subduction(Arc::new(surface)));
    }

    pub fn submit_subduction_surface_arc(&self, surface: Arc<dyn Any + Send + Sync>) {
        self.submit(ExternalSurfaceContent::Subduction(surface));
    }

    pub fn clear(&self) {
        self.submit(ExternalSurfaceContent::Empty);
    }

    pub fn request_frame(&self) {
        Application::send_proxy_event(UserEvent::ExternalSurfaceRequestFrame {
            window_id: self.window_id,
            surface_id: self.id,
        });
    }

    pub fn set_provider(&self, provider: ExternalSurfaceProviderHandle) {
        Application::send_proxy_event(UserEvent::ExternalSurfaceProvider {
            window_id: self.window_id,
            surface_id: self.id,
            provider,
        });
    }

    fn submit(&self, content: ExternalSurfaceContent) {
        Application::send_proxy_event(UserEvent::ExternalSurfaceContent {
            window_id: self.window_id,
            surface_id: self.id,
            content,
        });
    }
}
