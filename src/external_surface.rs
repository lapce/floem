use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use peniko::{ImageData, kurbo::Size};
use winit::window::WindowId;

use crate::{Application, app::UserEvent};

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
        (surface, SubductionWgpuSurface { native })
    }
}

#[cfg(feature = "subduction")]
#[derive(Clone, Debug)]
pub struct SubductionWgpuSurface {
    native: Arc<subduction_platform::ExternalWgpuSurface>,
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

    fn submit(&self, content: ExternalSurfaceContent) {
        Application::send_proxy_event(UserEvent::ExternalSurfaceContent {
            window_id: self.window_id,
            surface_id: self.id,
            content,
        });
    }
}
