//! Direct compositor surfaces owned by an external producer.
//!
//! [`ExternalSurface`] is the direct-composition counterpart to
//! [`crate::compositor_surface::CompositorSurfaceImage`]. It is for content that
//! the producer already owns as a compositor-compatible texture or native
//! layer. Floem validates each submission synchronously and returns an error
//! when the content cannot be directly composited. It does not fall back to
//! renderer sampling.
//!
//! Use [`crate::compositor_surface::CompositorSurfaceImage`] with
//! [`crate::compositor_surface::CompositorSurfaceProducer`] when Floem should
//! place the content as an image and remain free to flatten it for clips,
//! masks, filters, effects, or grouped rendering.

use std::fmt;

use peniko::kurbo::Size;
use winit::window::WindowId;

use crate::{
    Application,
    app::UserEvent,
    compositor_surface::{CompositorSurfaceContent, CompositorSurfaceId, ExternalTexture},
};

/// Stable identity for a direct external compositor slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalSurfaceId(CompositorSurfaceId);

impl ExternalSurfaceId {
    #[must_use]
    pub fn image_id(self) -> imaging::ExternalImageId {
        self.0.image_id()
    }

    #[must_use]
    pub fn compositor_surface_id(self) -> CompositorSurfaceId {
        self.0
    }
}

/// Direct compositor slot supplied by an external producer.
///
/// Use this for producer-owned textures or native layers that must be
/// published as compositor content. Submissions are validated immediately. If a
/// texture or native layer cannot be attached directly, the submit call returns
/// [`ExternalSurfaceError`] instead of silently changing to renderer fallback.
///
/// This API is intentionally stricter than
/// [`crate::compositor_surface::CompositorSurfaceImage`]. If the content needs
/// normal image behavior inside the Floem scene, including flattening under
/// clips, masks, filters, effects, or opacity groups, use a compositor surface
/// image instead.
#[derive(Clone, Debug)]
pub struct ExternalSurface {
    id: ExternalSurfaceId,
    window_id: WindowId,
}

impl ExternalSurface {
    /// Creates a direct external surface associated with `window_id`.
    #[must_use]
    pub fn new(window_id: WindowId) -> Self {
        Self {
            id: ExternalSurfaceId(CompositorSurfaceId::next()),
            window_id,
        }
    }

    #[must_use]
    pub fn id(&self) -> ExternalSurfaceId {
        self.id
    }

    /// Creates an Imaging external image identifier for this surface.
    ///
    /// This is useful for sharing identity with the display list, but direct
    /// external surfaces still require direct-compositable content on submit.
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
    pub fn handle(&self) -> ExternalSurfaceHandle {
        ExternalSurfaceHandle {
            id: self.id,
            window_id: self.window_id,
        }
    }

    /// Submits a texture for direct compositor publication.
    ///
    /// Validation happens before the update is sent to the window. The texture
    /// must wrap a Subduction-submitted surface frame with a compositor resource
    /// key and a size matching `texture.size`.
    pub fn submit_texture(&self, texture: ExternalTexture) -> Result<(), ExternalSurfaceError> {
        self.handle().submit_texture(texture)
    }

    /// Submits an opaque platform layer for direct compositor publication.
    ///
    /// Platform-specific layer types live in Subduction. Floem only stores and
    /// orders the opaque `NativeLayer`; backend attachment happens during
    /// compositor commit.
    pub fn submit_native_layer(
        &self,
        native_layer: subduction::NativeLayer,
    ) -> Result<(), ExternalSurfaceError> {
        self.handle().submit_native_layer(native_layer)
    }

    pub fn clear(&self) {
        self.handle().clear();
    }

    /// Sets the preferred maximum update rate for this direct compositor layer.
    ///
    /// Floem rounds the value down to a display-friendly cadence. `None`
    /// clears the preference.
    pub fn set_target_fps(&self, target_fps: Option<f64>) {
        self.handle().set_target_fps(target_fps);
    }
}

/// Sendable handle for updating a direct external surface.
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

    /// Submits a texture for direct compositor publication.
    ///
    /// See [`ExternalSurface::submit_texture`] for validation rules.
    pub fn submit_texture(&self, texture: ExternalTexture) -> Result<(), ExternalSurfaceError> {
        validate_direct_texture(&texture)?;
        Application::send_proxy_event(UserEvent::CompositorSurfaceContent {
            window_id: self.window_id,
            surface_id: self.id.compositor_surface_id(),
            content: CompositorSurfaceContent::Texture(texture),
        });
        Ok(())
    }

    /// Submits an opaque platform layer for direct compositor publication.
    pub fn submit_native_layer(
        &self,
        native_layer: subduction::NativeLayer,
    ) -> Result<(), ExternalSurfaceError> {
        Application::send_proxy_event(UserEvent::CompositorSurfaceContent {
            window_id: self.window_id,
            surface_id: self.id.compositor_surface_id(),
            content: CompositorSurfaceContent::NativeLayer(native_layer),
        });
        Ok(())
    }

    pub fn clear(&self) {
        Application::send_proxy_event(UserEvent::CompositorSurfaceContent {
            window_id: self.window_id,
            surface_id: self.id.compositor_surface_id(),
            content: CompositorSurfaceContent::Empty,
        });
    }

    /// Sets the preferred maximum update rate for this direct compositor layer.
    pub fn set_target_fps(&self, target_fps: Option<f64>) {
        Application::send_proxy_event(UserEvent::CompositorSurfaceTargetFps {
            window_id: self.window_id,
            surface_id: self.id.compositor_surface_id(),
            target_fps,
        });
    }
}

/// Error returned when direct external content cannot be accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalSurfaceError {
    /// The submitted texture has a zero width or height.
    EmptyTextureSize,
    /// The payload is not a direct-compositable Subduction submitted frame.
    UnsupportedTexturePayload,
    /// The submitted frame has no compositor resource key.
    MissingCompositorResource,
    /// The declared texture size does not match the submitted frame size.
    SizeMismatch {
        texture: (u32, u32),
        submitted: (u32, u32),
    },
}

impl fmt::Display for ExternalSurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTextureSize => write!(f, "external surface texture size must be non-zero"),
            Self::UnsupportedTexturePayload => write!(
                f,
                "external surface texture is not directly publishable by the compositor"
            ),
            Self::MissingCompositorResource => write!(
                f,
                "external surface texture has no compositor resource key for direct publication"
            ),
            Self::SizeMismatch { texture, submitted } => write!(
                f,
                "external surface texture size {:?} does not match submitted frame size {:?}",
                texture, submitted
            ),
        }
    }
}

impl std::error::Error for ExternalSurfaceError {}

fn validate_direct_texture(texture: &ExternalTexture) -> Result<(), ExternalSurfaceError> {
    if texture.size.width <= 0.0 || texture.size.height <= 0.0 {
        return Err(ExternalSurfaceError::EmptyTextureSize);
    }
    let Some(frame) = texture
        .payload
        .downcast_ref::<subduction::wgpu::SubmittedSurfaceFrame>()
    else {
        return Err(ExternalSurfaceError::UnsupportedTexturePayload);
    };
    if frame.resource_key.is_none() {
        return Err(ExternalSurfaceError::MissingCompositorResource);
    }
    let texture_size = size_to_u32(texture.size);
    let submitted_size = (frame.size.width, frame.size.height);
    if texture_size != submitted_size {
        return Err(ExternalSurfaceError::SizeMismatch {
            texture: texture_size,
            submitted: submitted_size,
        });
    }
    Ok(())
}

fn size_to_u32(size: Size) -> (u32, u32) {
    (
        size.width.round().max(0.0) as u32,
        size.height.round().max(0.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_external_surface_rejects_empty_texture_size() {
        let texture = ExternalTexture::new(Size::ZERO, 7_u32);
        assert_eq!(
            validate_direct_texture(&texture),
            Err(ExternalSurfaceError::EmptyTextureSize)
        );
    }

    #[test]
    fn direct_external_surface_rejects_non_compositor_payload() {
        let texture = ExternalTexture::new(Size::new(16.0, 16.0), 7_u32);
        assert_eq!(
            validate_direct_texture(&texture),
            Err(ExternalSurfaceError::UnsupportedTexturePayload)
        );
    }
}
