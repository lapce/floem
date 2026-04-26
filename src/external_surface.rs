use std::{
    any::Any,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, sync_channel},
    },
    time::{Duration, Instant},
};

#[cfg(all(feature = "subduction", target_os = "macos"))]
use std::{cell::RefCell, collections::HashMap};

use peniko::{
    ImageData,
    kurbo::{Rect, Size},
};
use winit::window::WindowId;

use crate::{
    Application,
    app::UserEvent,
    frame::{DisplayTiming, FrameOutcome, PresentationInterval},
};

static NEXT_EXTERNAL_SURFACE_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(all(feature = "subduction", target_os = "macos"))]
thread_local! {
    static SUBDUCTION_FRAME_TICKERS: RefCell<HashMap<WindowId, Vec<Weak<Mutex<SubductionFrameTickerState>>>>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalSurfaceId(u64);

impl ExternalSurfaceId {
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn test_new(value: u64) -> Self {
        Self(value)
    }
}

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

    #[cfg(feature = "subduction")]
    #[must_use]
    pub fn new_subduction_wgpu(window_id: WindowId, size: Size) -> (Self, SubductionWgpuSurface) {
        let surface = Self::new(
            window_id,
            ExternalSurfaceConfig {
                kind: ExternalSurfaceKind::WgpuTexture,
                alpha_mode: ExternalSurfaceAlphaMode::Premultiplied,
                preferred_size: Some(size),
            },
        );
        let native = Arc::new(subduction_platform::ExternalWgpuSurface::new(
            subduction_core::layer::SurfaceId(surface.id().get() as u32),
            size.width,
            size.height,
        ));
        let content: Arc<dyn Any + Send + Sync> = native.clone();
        surface.handle().submit_subduction_surface_arc(content);
        (surface, SubductionWgpuSurface { native, window_id })
    }
}

#[cfg(feature = "subduction")]
#[derive(Clone, Debug)]
pub struct SubductionWgpuSurface {
    native: Arc<subduction_platform::ExternalWgpuSurface>,
    window_id: WindowId,
}

#[cfg(feature = "subduction")]
pub type SubductionWgpuTarget = subduction_platform::ExternalWgpuTarget;

#[cfg(feature = "subduction")]
impl SubductionWgpuSurface {
    #[must_use]
    pub fn surface_id(&self) -> subduction_core::layer::SurfaceId {
        self.native.surface_id()
    }

    #[must_use]
    pub fn native(&self) -> &subduction_platform::ExternalWgpuSurface {
        self.native.as_ref()
    }

    pub async fn create_target(
        &self,
        width: u32,
        height: u32,
    ) -> Result<
        subduction_platform::ExternalWgpuTarget,
        subduction_platform::ExternalWgpuSurfaceError,
    > {
        self.native.create_target(width, height).await
    }

    /// Creates a configured target using Floem's existing WGPU resources.
    ///
    /// Use this path for video/external-surface producers embedded in a Floem
    /// app so presentation uses the same instance/adapter/device/queue as the
    /// window renderer.
    pub fn create_target_with_gpu_resources(
        &self,
        gpu_resources: &crate::gpu_resources::GpuResources,
        width: u32,
        height: u32,
    ) -> Result<
        subduction_platform::ExternalWgpuTarget,
        subduction_platform::ExternalWgpuSurfaceError,
    > {
        self.native.create_target_with_context(
            &gpu_resources.instance,
            &gpu_resources.adapter,
            gpu_resources.device.clone(),
            gpu_resources.queue.clone(),
            width,
            height,
        )
    }

    #[cfg(target_os = "macos")]
    pub fn start_frame_ticker(
        &self,
    ) -> Result<(SubductionFrameTicker, Receiver<SubductionFrameTick>), String> {
        use subduction_backend_apple::{DisplayLink, TickForwarder, timebase};
        use subduction_core::output::OutputId;

        let display_id = current_window_display_id(&self.window_id);
        let (tx, rx) = sync_channel(2);
        let timebase = timebase();
        let host_origin = subduction_backend_apple::now();
        let instant_origin = Instant::now();
        let forwarder = TickForwarder::new_direct(move |tick| {
            let refresh_interval = tick
                .refresh_interval
                .map(|ticks| Duration::from_nanos(timebase.ticks_to_nanos(ticks)));
            let predicted_present = tick.predicted_present.map(|present| {
                instant_origin
                    + Duration::from_nanos(
                        present
                            .saturating_duration_since(host_origin)
                            .to_nanos(timebase),
                    )
            });
            let display_timing = tick.display_capabilities.map(|capabilities| {
                let min_frame_interval = Duration::from_nanos(
                    timebase.ticks_to_nanos(capabilities.min_frame_interval.0),
                );
                let max_frame_interval = Duration::from_nanos(
                    timebase.ticks_to_nanos(capabilities.max_frame_interval.0),
                );
                if capabilities.is_variable() {
                    DisplayTiming::Variable {
                        min_frame_interval,
                        max_frame_interval,
                    }
                } else {
                    DisplayTiming::fixed(min_frame_interval)
                }
            });
            let _ = tx.try_send(SubductionFrameTick {
                received_at: Instant::now(),
                frame_index: tick.frame_index,
                refresh_interval,
                predicted_present,
                display_timing,
            });
        });
        let display_link = DisplayLink::new(
            forwarder.sender(),
            OutputId(self.surface_id().0),
            display_id,
        )
        .map_err(|err| format!("failed to create external surface frame ticker: {err}"))?;
        display_link
            .start()
            .map_err(|err| format!("failed to start external surface frame ticker: {err}"))?;
        let state = Arc::new(Mutex::new(SubductionFrameTickerState {
            window_id: self.window_id,
            output: OutputId(self.surface_id().0),
            display_id,
            display_link,
            forwarder,
        }));
        register_subduction_frame_ticker(self.window_id, &state);
        Ok((SubductionFrameTicker { state }, rx))
    }
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
fn current_window_display_id(window_id: &WindowId) -> Option<u32> {
    crate::window::tracking::with_window(window_id, |window| {
        window
            .current_monitor()
            .and_then(|monitor| u32::try_from(monitor.native_id()).ok())
    })
    .flatten()
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
fn register_subduction_frame_ticker(
    window_id: WindowId,
    ticker: &Arc<Mutex<SubductionFrameTickerState>>,
) {
    SUBDUCTION_FRAME_TICKERS.with(|tickers| {
        tickers
            .borrow_mut()
            .entry(window_id)
            .or_default()
            .push(Arc::downgrade(ticker));
    });
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
pub(crate) fn refresh_subduction_frame_tickers(window_id: WindowId) {
    SUBDUCTION_FRAME_TICKERS.with(|tickers| {
        let mut tickers = tickers.borrow_mut();
        let Some(window_tickers) = tickers.get_mut(&window_id) else {
            return;
        };

        window_tickers.retain(|ticker| {
            let Some(ticker) = ticker.upgrade() else {
                return false;
            };
            if let Ok(mut ticker) = ticker.lock()
                && let Err(err) = ticker.refresh_display()
            {
                eprintln!("{err}");
            }
            true
        });
    });
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
#[derive(Clone, Copy, Debug)]
pub struct SubductionFrameTick {
    pub received_at: Instant,
    pub frame_index: u64,
    pub refresh_interval: Option<Duration>,
    pub predicted_present: Option<Instant>,
    pub display_timing: Option<DisplayTiming>,
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
#[derive(Debug)]
pub struct SubductionFrameTicker {
    state: Arc<Mutex<SubductionFrameTickerState>>,
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
#[derive(Debug)]
struct SubductionFrameTickerState {
    window_id: WindowId,
    output: subduction_core::output::OutputId,
    display_id: Option<u32>,
    display_link: subduction_backend_apple::DisplayLink,
    forwarder: subduction_backend_apple::TickForwarder,
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
impl SubductionFrameTickerState {
    fn refresh_display(&mut self) -> Result<bool, String> {
        let display_id = current_window_display_id(&self.window_id);
        if self.display_id == display_id {
            return Ok(false);
        }

        let display_link = subduction_backend_apple::DisplayLink::new(
            self.forwarder.sender(),
            self.output,
            display_id,
        )
        .map_err(|err| format!("failed to recreate external surface frame ticker: {err}"))?;
        display_link
            .start()
            .map_err(|err| format!("failed to restart external surface frame ticker: {err}"))?;
        let old_display_link = std::mem::replace(&mut self.display_link, display_link);
        let _ = old_display_link.stop();
        self.display_id = display_id;
        Ok(true)
    }
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
impl Drop for SubductionFrameTicker {
    fn drop(&mut self) {
        if let Ok(ticker) = self.state.lock() {
            let _ = ticker.display_link.stop();
        }
    }
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
    fn update_current_content(
        &mut self,
        args: ExternalSurfaceFrameArgs,
    ) -> ExternalSurfaceFrameUpdate;

    fn current_content(&self) -> Option<ExternalSurfaceContent>;

    fn release_current_content(&mut self, outcome: ExternalSurfaceOutcome);
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

#[derive(Clone, Copy, Debug)]
pub struct ExternalSurfaceFrameArgs {
    pub surface_id: ExternalSurfaceId,
    pub interval: PresentationInterval,
    pub visible: bool,
    pub rect: Rect,
    pub size_px: Size,
    pub previous_outcome: Option<ExternalSurfaceOutcome>,
}

#[derive(Clone, Copy, Debug)]
pub struct ExternalSurfaceOutcome {
    pub surface_id: ExternalSurfaceId,
    pub frame_index: u64,
    pub visible: bool,
    pub outcome: FrameOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExternalSurfacePaintOptions {
    pub opacity: f32,
    pub hit_test: bool,
}

impl Default for ExternalSurfacePaintOptions {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            hit_test: true,
        }
    }
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
