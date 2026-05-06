//! Compositor-owned image surfaces.
//!
//! This module models compositor content that still behaves like an image in
//! the Floem display list. A [`SurfaceView`] is the paint-facing
//! identity used by views and `imaging::ImageBrush`. A
//! [`CompositorSurfaceProducer`] is the producer-facing handle that renders
//! frames for that image into Subduction-owned wgpu targets.
//!
//! During display-list lowering, Floem chooses how to use the image for the
//! current paint state. If the placement can be directly composed, the image
//! can become a platform compositor layer. If a clip, mask, filter, effect,
//! opacity group, or other paint state requires renderer participation, Floem
//! can sample the same submitted content through Imaging and flatten it into
//! an intermediate pass.
//!
//! Use this API when produced content should be placed by the view tree and
//! must remain correct under normal Floem paint operations. Use
//! [`crate::external_surface::ExternalSurface`] when content is submitted
//! directly by external code instead of through Floem-owned frame opportunities.

use rustc_hash::FxHashMap;
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
    ImageAlphaType, ImageData,
    kurbo::{Rect, Size},
};
use winit::window::WindowId;

use crate::{
    Application,
    app::UserEvent,
    frame::{FrameOutcome, FrameRatePreference, PresentationInterval},
    gpu_resources::GpuResources,
};

static NEXT_SURFACE_SLOT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable window-local identity for submitted surface content.
///
/// A surface slot is the shared content identity behind both compositor-surface
/// producers and direct external surfaces. It is not itself a platform layer
/// or a producer; it is the slot Floem uses to connect submitted frames to
/// [`SurfaceView`] images during display-list lowering and composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceSlotId(u64);

impl SurfaceSlotId {
    pub(crate) fn next() -> Self {
        Self(NEXT_SURFACE_SLOT_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn test_new(value: u64) -> Self {
        Self(value)
    }
}

/// Backwards-compatible name for older compositor-surface APIs.
///
/// New code should use [`SurfaceSlotId`], because the same slot identity is
/// used by compositor-surface producers and external surfaces.
pub type CompositorSurfaceId = SurfaceSlotId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SurfaceImageKey {
    image_id: crate::effects::SurfaceImageId,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RegisteredSurfaceImage {
    pub slot_id: SurfaceSlotId,
    pub source_size: Size,
}

/// Window-local registry for compositor surface image brush payloads.
///
/// Floem records surface images as Floem-owned image values first. During
/// display-list lowering the window state registers each `(image, resolved
/// size)` pair here and receives an `imaging::ExternalImage` id for
/// renderer-facing scene commands. Separate image handles can reference the
/// same producer surface without sharing source-size state. Composition
/// planning resolves those ids through this side table instead of relying on
/// bit patterns inside `ExternalImageId`.
#[derive(Debug)]
pub(crate) struct SurfaceImageRegistry {
    next_id: u64,
    by_key: FxHashMap<SurfaceImageKey, imaging::ExternalImageId>,
    by_id: FxHashMap<imaging::ExternalImageId, SurfaceImageKey>,
    by_registered_id: FxHashMap<imaging::ExternalImageId, RegisteredSurfaceImage>,
}

impl Default for SurfaceImageRegistry {
    fn default() -> Self {
        Self {
            next_id: 1,
            by_key: FxHashMap::default(),
            by_id: FxHashMap::default(),
            by_registered_id: FxHashMap::default(),
        }
    }
}

impl SurfaceImageRegistry {
    pub(crate) fn register(
        &mut self,
        image: &crate::effects::SurfaceImage,
        source_size: Size,
    ) -> imaging::ExternalImage {
        let key = SurfaceImageKey {
            image_id: image.image_id,
            width: source_size.width.ceil().max(1.0) as u32,
            height: source_size.height.ceil().max(1.0) as u32,
        };
        let id = if let Some(id) = self.by_key.get(&key).copied() {
            id
        } else {
            let id = imaging::ExternalImageId(self.next_id);
            self.next_id = self.next_id.saturating_add(1);
            self.by_key.insert(key, id);
            self.by_id.insert(id, key);
            id
        };
        self.by_registered_id.insert(
            id,
            RegisteredSurfaceImage {
                slot_id: image.slot_id,
                source_size,
            },
        );
        imaging::ExternalImage::new(
            id,
            key.width,
            key.height,
            ImageAlphaType::AlphaPremultiplied,
        )
    }

    pub(crate) fn resolve_registered(
        &self,
        image_id: imaging::ExternalImageId,
    ) -> Option<RegisteredSurfaceImage> {
        if let Some(registered) = self.by_registered_id.get(&image_id) {
            return Some(*registered);
        }
        #[cfg(test)]
        {
            Some(RegisteredSurfaceImage {
                slot_id: SurfaceSlotId(image_id.0),
                source_size: Size::new(0.0, 0.0),
            })
        }
        #[cfg(not(test))]
        {
            None
        }
    }
}

/// Paint-facing identity for compositor-produced image content.
///
/// `SurfaceView` is the consumer side of the API. It gives the view
/// tree a stable image identity, but it does not render frames itself. Use
/// [`SurfaceView::image`] to create the Floem image payload that an
/// `ImageBrush` paints.
///
/// The same surface can be viewed through more than one image handle and at
/// more than one source size. Each [`SurfaceView::image`] call
/// creates a distinct image handle; clones of that handle preserve identity.
/// At composition time each placement may either be promoted to a compositor
/// layer or sampled by the renderer, depending on the surrounding display-list
/// state.
///
/// A view can come from either a [`CompositorSurfaceProducer`] or an
/// [`crate::external_surface::ExternalSurface`]. The producer/external handle
/// owns frame production and presentation policy; `SurfaceView` only describes
/// how that submitted content is used as an image in paint.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceView {
    slot_id: SurfaceSlotId,
    window_id: WindowId,
}

impl SurfaceView {
    #[must_use]
    pub(crate) fn new(window_id: WindowId, slot_id: SurfaceSlotId) -> Self {
        Self { slot_id, window_id }
    }

    #[must_use]
    pub fn slot_id(&self) -> SurfaceSlotId {
        self.slot_id
    }

    #[must_use]
    pub fn id(&self) -> SurfaceSlotId {
        self.slot_id
    }

    /// Creates a Floem image handle for this surface at `size`.
    ///
    /// The returned image can be used with `floem::ImageBrush`. `size` is the
    /// logical source size for this brush placement. It may be absolute or
    /// length-based; length-based sizes are resolved against the painted
    /// geometry bounds during display-list lowering. It does not create a new
    /// surface identity: multiple calls to `image` return handles for the same
    /// submitted compositor content with different advertised source sizes.
    ///
    /// Each `image(size)` call creates a distinct image handle for the same
    /// submitted surface content. Cloning that handle preserves its identity;
    /// reusing it at the same resolved source size is deduped during lowering.
    ///
    /// If the brush is used in a simple promotable fill, Floem may publish the
    /// surface directly as a compositor layer. If active group state requires
    /// flattening, the renderer samples the same submitted surface content.
    #[must_use]
    pub fn image(&self, size: impl Into<crate::effects::ImageSize>) -> crate::effects::Image {
        crate::effects::SurfaceImage::new(self.slot_id, size).into()
    }

    #[must_use]
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub fn handle(&self) -> CompositorSurfaceHandle {
        CompositorSurfaceHandle {
            id: self.slot_id,
            window_id: self.window_id,
        }
    }
}

/// Configuration for a wgpu-backed [`CompositorSurfaceProducer`].
#[derive(Clone, Debug)]
pub struct CompositorSurfaceProducerConfig {
    /// Subduction target configuration for leased wgpu frame targets.
    ///
    /// This controls target allocation policy such as maximum frame latency and
    /// backend format preferences. It does not control where the produced image
    /// is placed in the Floem scene.
    pub surface: subduction::wgpu::ExternalSurfaceConfig,
    /// Alpha interpretation for completed frames when they are published or
    /// sampled.
    pub alpha_mode: CompositorSurfaceAlphaMode,
    /// Frame-rate preference for producer callbacks.
    ///
    /// This is a pacing preference for producer frame opportunities.
    ///
    /// Fixed-rate displays round capped values down to stable divisors of the
    /// refresh rate. For example, 60 fps on fixed 75 Hz becomes 37.5 fps; 60 fps
    /// on fixed 120 Hz becomes 60 fps. Variable-refresh displays may use an
    /// in-range request directly, so 60 fps on a 48-75 Hz VRR display can remain
    /// 60 fps. Out-of-range requests fall back to the nearest supported
    /// display-friendly cadence.
    ///
    /// The actual visible rate can still be lower when frame work is skipped,
    /// coalesced, waiting on a transaction-presented dependent layer, or waiting
    /// on compositor/GPU completion.
    pub frame_rate: FrameRatePreference,
}

impl Default for CompositorSurfaceProducerConfig {
    fn default() -> Self {
        Self {
            surface: subduction::wgpu::ExternalSurfaceConfig::default(),
            alpha_mode: CompositorSurfaceAlphaMode::Premultiplied,
            frame_rate: FrameRatePreference::full(),
        }
    }
}

/// Producer-side frame source for a [`SurfaceView`].
///
/// A producer supplies rendered frames for one [`SurfaceView`]
/// identity that Floem owns and places in the scene. Create one with
/// [`CompositorSurfaceProducer::new`]: the returned
/// [`SurfaceView`] is painted by the view tree, while the producer
/// receives frame opportunities and leases wgpu targets for the renderer.
/// Multiple `ImageBrush` placements can reference that same image identity;
/// they do not create separate producers.
///
/// The producer is not a view and does not affect paint order directly. Keep it
/// with the renderer or state object that can service frame callbacks.
///
/// The producer callback receives frame opportunities when the current
/// compositor plan needs new content for that image identity. A typical
/// callback reads [`CompositorSurfaceFrameCx::frame_time`], acquires a
/// Subduction-managed wgpu target with
/// [`CompositorSurfaceFrameCx::acquire_target`], renders on the caller's worker
/// or queue, sends the resulting
/// [`subduction::wgpu::SurfaceFrameCompletion`] through
/// [`CompositorSurfaceFrameCx::completion_sender`], and returns. Transaction
/// behavior is configured on the producer with
/// [`CompositorSurfaceProducer::presents_with_transaction`] and applies until
/// changed again.
///
/// Use this when Floem owns placement and fallback behavior, but another
/// renderer owns the contents. Use [`crate::external_surface::ExternalSurface`]
/// when the producer owns already-compositable content and unsupported
/// submissions should be rejected synchronously.
#[derive(Clone)]
pub struct CompositorSurfaceProducer {
    view: SurfaceView,
    handle: CompositorSurfaceHandle,
    config: CompositorSurfaceProducerConfig,
    target_size: Arc<Mutex<Size>>,
    callback: Arc<Mutex<Option<CompositorSurfaceProducerCallback>>>,
    completions: Arc<Mutex<mpsc::Receiver<subduction::wgpu::SurfaceFrameCompletion>>>,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
}

impl std::fmt::Debug for CompositorSurfaceProducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositorSurfaceProducer")
            .field("id", &self.handle.id())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

type CompositorSurfaceProducerCallback = Box<
    dyn FnMut(&mut CompositorSurfaceFrameCx) -> Result<(), subduction::wgpu::SurfaceFrameError>
        + Send,
>;

impl CompositorSurfaceProducer {
    /// Creates a producer that renders into a fixed physical-pixel wgpu target.
    ///
    /// `size` is the producer's target texture size in physical pixels, not a
    /// logical paint size. Floem leases wgpu targets at this exact pixel size;
    /// moving the window between displays with different scale factors does not
    /// change the producer's allocation size.
    ///
    /// Paint code gets the lightweight [`SurfaceView`] with [`Self::view`] and
    /// then creates one or more [`SurfaceView::image`] handles with explicit
    /// logical source sizes.
    ///
    /// This separation is intentional: producer size is physical
    /// allocation/rendering state, while image size is logical
    /// brush/source-coordinate state.
    #[must_use]
    pub fn new(window_id: WindowId, size: Size, config: CompositorSurfaceProducerConfig) -> Self {
        let view = SurfaceView::new(window_id, SurfaceSlotId::next());
        view.handle().set_frame_rate_preference(config.frame_rate);
        let producer = Self::from_view(view, size, config);
        producer
            .handle
            .set_provider(Arc::new(Mutex::new(producer.provider())));
        producer
    }

    fn from_view(
        view: SurfaceView,
        target_size: Size,
        config: CompositorSurfaceProducerConfig,
    ) -> Self {
        let (completion_tx, completion_rx) = mpsc::channel();
        Self {
            view,
            handle: view.handle(),
            config,
            target_size: Arc::new(Mutex::new(target_size)),
            callback: Arc::new(Mutex::new(None)),
            completions: Arc::new(Mutex::new(completion_rx)),
            completion_tx,
            in_flight: Arc::new(Mutex::new(0)),
        }
    }

    #[must_use]
    pub fn view(&self) -> SurfaceView {
        self.view
    }

    /// Returns the producer's current physical-pixel target size.
    #[must_use]
    pub fn target_size(&self) -> Size {
        *self.target_size.lock().unwrap()
    }

    /// Sets the physical-pixel target size used for future frame opportunities.
    ///
    /// This changes the wgpu target size Floem leases for the producer. It does
    /// not change existing [`SurfaceView::image`] handles or their logical
    /// image-brush source sizes.
    pub fn set_target_size(&self, size: Size) {
        *self.target_size.lock().unwrap() = size;
        self.handle.request_frame();
    }

    #[must_use]
    pub fn slot_id(&self) -> SurfaceSlotId {
        self.handle.id()
    }

    #[must_use]
    pub fn surface_id(&self) -> SurfaceSlotId {
        self.slot_id()
    }

    /// Installs the frame callback for this producer and requests a frame.
    ///
    /// Floem invokes the callback when the associated image participates in a
    /// compositor plan that needs producer work. The callback may skip the
    /// opportunity, or it may acquire one target and complete it
    /// asynchronously. Returning from the callback does not mean rendering is
    /// complete; completion is reported through
    /// [`CompositorSurfaceFrameCx::completion_sender`].
    pub fn set_frame_callback(
        &self,
        callback: impl FnMut(
            &mut CompositorSurfaceFrameCx,
        ) -> Result<(), subduction::wgpu::SurfaceFrameError>
        + Send
        + 'static,
    ) {
        *self.callback.lock().unwrap() = Some(Box::new(callback));
        self.handle.request_frame();
    }

    /// Sets the frame-rate preference for producer callbacks and diagnostics.
    pub fn set_frame_rate_preference(&self, frame_rate: FrameRatePreference) {
        self.handle.set_frame_rate_preference(frame_rate);
    }

    /// Sets whether completed frames publish with the window compositor transaction.
    ///
    /// The default is `true`. When enabled, newly completed frames publish
    /// atomically with the rest of the window layer tree. This is the correct
    /// mode when other Floem layers sample the produced image, when resize
    /// synchronization matters, or when independent publication would let layers
    /// visibly update out of phase.
    ///
    /// Passing `false` allows completed frames to publish outside the window
    /// transaction. Use that only for independent compositor content whose
    /// updates are allowed to race ahead of normal scene commits.
    ///
    /// The setting is persistent surface state. It applies to future
    /// completions until changed again.
    pub fn presents_with_transaction(&self, presents_with_transaction: bool) {
        self.handle
            .presents_with_transaction(presents_with_transaction);
    }

    fn provider(&self) -> CompositorSurfaceProducerProvider {
        CompositorSurfaceProducerProvider {
            handle: self.handle.clone(),
            config: self.config.clone(),
            callback: self.callback.clone(),
            completions: self.completions.clone(),
            completion_tx: self.completion_tx.clone(),
            in_flight: self.in_flight.clone(),
            target_size: self.target_size.clone(),
            current_content: None,
            pending_request_started_at: FxHashMap::default(),
            last_requested_frame_index: None,
        }
    }
}

/// Per-frame context passed to a [`CompositorSurfaceProducer`] callback.
///
/// Each context represents one frame opportunity for one image placement group.
/// The callback can acquire at most one wgpu target. If it acquires a target,
/// it is responsible for sending exactly one
/// [`subduction::wgpu::SurfaceFrameCompletion`] through
/// [`Self::completion_sender`] when rendering finishes.
///
/// Presentation policy is persistent surface state. Producers present with the
/// window compositor transaction by default. Call
/// [`CompositorSurfaceProducer::presents_with_transaction`] to change that
/// policy until it is changed again.
pub struct CompositorSurfaceFrameCx {
    frame_time: crate::frame::FrameTime,
    is_window_resize: bool,
    opportunity: subduction::wgpu::SurfaceFrameOpportunity,
    config: CompositorSurfaceProducerConfig,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
    target: Option<subduction::wgpu::SurfaceFrameLease>,
    acquired_target: bool,
    decision: SurfaceFrameCallbackDecision,
}

impl CompositorSurfaceFrameCx {
    #[must_use]
    pub fn frame_time(&self) -> crate::frame::FrameTime {
        self.frame_time
    }

    /// Returns true when this frame opportunity was produced in response to a
    /// window resize.
    ///
    /// Producers can use this to prioritize resize-correct content over normal
    /// animation cadence, or to choose lower-latency work during live resize.
    #[must_use]
    pub fn is_window_resize(&self) -> bool {
        self.is_window_resize
    }

    #[must_use]
    pub fn opportunity(&self) -> subduction::wgpu::SurfaceFrameOpportunity {
        self.opportunity
    }

    #[must_use]
    pub fn config(&self) -> &CompositorSurfaceProducerConfig {
        &self.config
    }

    /// Returns the channel used to deliver asynchronous frame completions.
    ///
    /// The callback normally clones this sender, moves the acquired target to
    /// a render task, then sends the task's completion when rendering finishes.
    #[must_use]
    pub fn completion_sender(&self) -> mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion> {
        self.completion_tx.clone()
    }

    /// Acquires the Subduction-managed wgpu target for this frame opportunity.
    ///
    /// This is non-blocking. It returns [`SurfaceFrameError::NoTargetAvailable`]
    /// when the producer already has the configured number of frames in flight,
    /// and [`SurfaceFrameError::Unsupported`] when no target can be provided.
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
            self.acquired_target = true;
            return Ok(target);
        }
        unreachable!("target existence was checked before in-flight reservation")
    }

    /// Marks this opportunity as skipped.
    pub fn skip(&mut self, reason: subduction::wgpu::SurfaceSkipReason) {
        self.decision = SurfaceFrameCallbackDecision::Skip(reason);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceFrameCallbackDecision {
    None,
    Skip(subduction::wgpu::SurfaceSkipReason),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositorSurfaceConfig {
    pub kind: CompositorSurfaceKind,
    pub alpha_mode: CompositorSurfaceAlphaMode,
    /// Frame-rate preference for this compositor surface.
    ///
    /// This is interpreted the same way as
    /// [`CompositorSurfaceProducer::set_frame_rate_preference`].
    pub frame_rate: FrameRatePreference,
}

impl CompositorSurfaceConfig {
    #[must_use]
    pub fn texture() -> Self {
        Self {
            kind: CompositorSurfaceKind::WgpuTexture,
            alpha_mode: CompositorSurfaceAlphaMode::Premultiplied,
            frame_rate: FrameRatePreference::full(),
        }
    }

    #[must_use]
    pub fn video() -> Self {
        Self {
            kind: CompositorSurfaceKind::NativeTexture,
            alpha_mode: CompositorSurfaceAlphaMode::Opaque,
            frame_rate: FrameRatePreference::full(),
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

    #[must_use]
    pub fn from_submitted_frame(frame: subduction::wgpu::SubmittedSurfaceFrame) -> Self {
        let size = Size::new(f64::from(frame.size.width), f64::from(frame.size.height));
        Self::new(size, frame)
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

    /// Returns the physical-pixel wgpu target size to lease for a requested
    /// logical source size.
    ///
    /// The default scales the requested logical source size by the window's
    /// effective scale. Producer-backed compositor surfaces override this with
    /// the fixed physical texture size passed to
    /// [`CompositorSurfaceProducer::new`].
    fn target_size(&self, requested_source_size: Size, effective_scale: f64) -> Size {
        requested_source_size * effective_scale
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

struct CompositorSurfaceProducerProvider {
    handle: CompositorSurfaceHandle,
    config: CompositorSurfaceProducerConfig,
    callback: Arc<Mutex<Option<CompositorSurfaceProducerCallback>>>,
    completions: Arc<Mutex<mpsc::Receiver<subduction::wgpu::SurfaceFrameCompletion>>>,
    completion_tx: mpsc::Sender<subduction::wgpu::SurfaceFrameCompletion>,
    in_flight: Arc<Mutex<u32>>,
    target_size: Arc<Mutex<Size>>,
    current_content: Option<CompositorSurfaceContent>,
    pending_request_started_at: FxHashMap<u64, Instant>,
    last_requested_frame_index: Option<u64>,
}

impl CompositorSurfaceProvider for CompositorSurfaceProducerProvider {
    fn target_size(&self, _requested_source_size: Size, _effective_scale: f64) -> Size {
        *self.target_size.lock().unwrap()
    }

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
        let callback_started_at = Instant::now();
        let diag = crate::frame_source::frame_pacing_diag_enabled()
            || std::env::var_os("FLOEM_CUBE_DIAG").is_some();

        if !args.visible {
            if diag {
                crate::floem_debug_log!(
                    "floem compositor surface provider skip surface={:?} frame={} reason=not_visible",
                    args.surface_id,
                    args.frame_index,
                );
            }
            return CompositorSurfaceFrameUpdate {
                content_changed: frame_update.content_changed,
                request_next_frame: false,
                producer_observed_latency: frame_update.producer_observed_latency,
            };
        }

        if args.gpu_resources.is_none() {
            if diag {
                crate::floem_debug_log!(
                    "floem compositor surface provider skip surface={:?} frame={} reason=no_gpu_resources",
                    args.surface_id,
                    args.frame_index,
                );
            }
            return CompositorSurfaceFrameUpdate {
                content_changed: frame_update.content_changed,
                request_next_frame: true,
                producer_observed_latency: frame_update.producer_observed_latency,
            };
        }

        if self.last_requested_frame_index == Some(args.frame_index) {
            if diag {
                crate::floem_debug_log!(
                    "floem compositor surface provider skip surface={:?} frame={} reason=already_requested content_changed={}",
                    args.surface_id,
                    args.frame_index,
                    frame_update.content_changed,
                );
            }
            frame_update.request_next_frame = true;
            return frame_update;
        }

        if diag {
            crate::floem_debug_log!(
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
        let cx = CompositorSurfaceFrameCx {
            frame_time: args.frame_time,
            is_window_resize: args.is_window_resize,
            opportunity,
            config: self.config.clone(),
            completion_tx: self.completion_tx.clone(),
            in_flight: self.in_flight.clone(),
            target: args.target,
            acquired_target: false,
            decision: SurfaceFrameCallbackDecision::None,
        };

        let mut cx = cx;
        let callback_result = self
            .callback
            .lock()
            .unwrap()
            .as_mut()
            .map(|callback| callback(&mut cx));
        let decision = match callback_result {
            Some(Ok(())) => Some(Ok((cx.decision, cx.acquired_target))),
            Some(Err(err)) => Some(Err(err)),
            None => None,
        };
        let request_next_frame = match decision {
            Some(Ok((SurfaceFrameCallbackDecision::None, true))) => {
                self.last_requested_frame_index = Some(args.frame_index);
                self.pending_request_started_at
                    .insert(args.frame_index, callback_started_at);
                if diag {
                    crate::floem_debug_log!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=present",
                        args.surface_id,
                        args.frame_index,
                    );
                }
                true
            }
            Some(Ok((SurfaceFrameCallbackDecision::Skip(reason), acquired_target))) => {
                if acquired_target {
                    self.release_acquired_target();
                }
                if diag {
                    crate::floem_debug_log!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=skip reason={reason:?}",
                        args.surface_id,
                        args.frame_index,
                    );
                }
                false
            }
            Some(Ok((SurfaceFrameCallbackDecision::None, false))) => {
                if diag {
                    crate::floem_debug_log!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=none",
                        args.surface_id,
                        args.frame_index,
                    );
                }
                false
            }
            Some(Err(err)) => {
                if cx.acquired_target {
                    self.release_acquired_target();
                }
                if diag {
                    crate::floem_debug_log!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=error error={err:?}",
                        args.surface_id,
                        args.frame_index,
                    );
                }
                true
            }
            None => {
                if diag {
                    crate::floem_debug_log!(
                        "floem compositor surface provider decision surface={:?} frame={} decision=no_callback",
                        args.surface_id,
                        args.frame_index,
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

impl CompositorSurfaceProducerProvider {
    fn release_acquired_target(&self) {
        let mut in_flight = self.in_flight.lock().unwrap();
        *in_flight = in_flight.saturating_sub(1);
    }

    fn drain_completions(&mut self) -> CompositorSurfaceFrameUpdate {
        let diag = crate::frame_source::frame_pacing_diag_enabled()
            || std::env::var_os("FLOEM_CUBE_DIAG").is_some();
        let mut content_changed = false;
        let mut max_observed_latency = None;
        while let Ok(completion) = self.completions.lock().unwrap().try_recv() {
            let mut in_flight = self.in_flight.lock().unwrap();
            *in_flight = in_flight.saturating_sub(1);
            drop(in_flight);
            match completion {
                subduction::wgpu::SurfaceFrameCompletion::Submitted(frame) => {
                    let observed_latency = self
                        .pending_request_started_at
                        .remove(&frame.opportunity.frame_index)
                        .map(|started_at| Instant::now().saturating_duration_since(started_at));
                    if diag {
                        crate::floem_debug_log!(
                            "floem compositor surface provider completion surface={:?} frame={} submitted size={}x{} resource_key={:?} observed_latency={:?}",
                            self.handle.id(),
                            frame.opportunity.frame_index,
                            frame.size.width,
                            frame.size.height,
                            frame.resource_key,
                            observed_latency,
                        );
                    }
                    self.current_content = Some(CompositorSurfaceContent::Texture(
                        ExternalTexture::from_submitted_frame(frame),
                    ));
                    content_changed = true;
                    if let Some(observed_latency) = observed_latency {
                        max_observed_latency = Some(
                            max_observed_latency
                                .map(|latency: Duration| latency.max(observed_latency))
                                .unwrap_or(observed_latency),
                        );
                    }
                }
                subduction::wgpu::SurfaceFrameCompletion::Skipped(frame) => {
                    self.pending_request_started_at
                        .remove(&frame.opportunity.frame_index);
                    if diag {
                        crate::floem_debug_log!(
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
            producer_observed_latency: max_observed_latency,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompositorSurfaceFrameUpdate {
    pub content_changed: bool,
    pub request_next_frame: bool,
    pub producer_observed_latency: Option<Duration>,
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
            producer_observed_latency: None,
        }
    }

    #[must_use]
    pub fn request_next_frame() -> Self {
        Self {
            content_changed: false,
            request_next_frame: true,
            producer_observed_latency: None,
        }
    }

    #[must_use]
    pub fn content_changed_and_request_next_frame() -> Self {
        Self {
            content_changed: true,
            request_next_frame: true,
            producer_observed_latency: None,
        }
    }
}

#[derive(Debug)]
pub struct CompositorSurfaceFrameArgs {
    pub surface_id: CompositorSurfaceId,
    pub frame_index: u64,
    pub frame_time: crate::frame::FrameTime,
    pub interval: PresentationInterval,
    pub is_window_resize: bool,
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

    pub fn set_frame_rate_preference(&self, frame_rate: FrameRatePreference) {
        Application::send_proxy_event(UserEvent::CompositorSurfaceFrameRatePreference {
            window_id: self.window_id,
            surface_id: self.id,
            frame_rate,
        });
    }

    /// Sets whether submitted content publishes with the window compositor transaction.
    ///
    /// `true` is the default and keeps surface updates atomic with the window
    /// layer tree. `false` allows future surface updates to publish
    /// independently until changed again.
    pub fn presents_with_transaction(&self, presents_with_transaction: bool) {
        Application::send_proxy_event(UserEvent::CompositorSurfacePresentsWithTransaction {
            window_id: self.window_id,
            surface_id: self.id,
            presents_with_transaction,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::SurfaceImage;

    #[test]
    fn surface_image_registry_tracks_image_identity_separately_from_surface() {
        let surface_id = CompositorSurfaceId::test_new(7);
        let first = SurfaceImage::new(surface_id, Size::new(100.0, 50.0));
        let first_clone = first.clone();
        let second = SurfaceImage::new(surface_id, Size::new(100.0, 50.0));
        let mut registry = SurfaceImageRegistry::default();

        let first_external = registry.register(&first, Size::new(100.0, 50.0));
        let first_clone_external = registry.register(&first_clone, Size::new(100.0, 50.0));
        let second_external = registry.register(&second, Size::new(100.0, 50.0));

        assert_eq!(first_external.id, first_clone_external.id);
        assert_ne!(first_external.id, second_external.id);
        assert_eq!((first_external.width, first_external.height), (200, 100));
        assert_eq!(
            registry
                .resolve_registered(first_external.id)
                .unwrap()
                .slot_id,
            surface_id
        );
        assert_eq!(
            registry
                .resolve_registered(second_external.id)
                .unwrap()
                .slot_id,
            surface_id
        );
    }
}
