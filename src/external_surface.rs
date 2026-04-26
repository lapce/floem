use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
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
};

static NEXT_EXTERNAL_SURFACE_ID: AtomicU64 = AtomicU64::new(1);

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

    #[must_use]
    pub fn new_subduction_wgpu(
        window_id: WindowId,
        size: Size,
    ) -> (Self, Arc<subduction_platform::ExternalWgpuSurface>) {
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
        (surface, native)
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
