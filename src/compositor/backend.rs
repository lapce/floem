//! Backend contract for Floem's compositor layer.
//!
//! Floem keeps compositor identity, frame requests, and app-facing policy in
//! [`crate::compositor`]. Backends sit underneath that layer and mirror the
//! retained compositor state into a concrete presentation engine such as
//! `subduction`.

use super::{
    CompositorLayerDescriptor, CompositorLayerId, CompositorTiming, ExternalSurfaceDescriptor,
    ExternalSurfaceHandle, ExternalSurfaceId, FrameRequestReason,
};

/// Backend interface for mirroring Floem compositor state into a concrete
/// presentation/composition engine.
pub trait CompositorBackend {
    /// Human-readable backend name for diagnostics.
    fn name(&self) -> &'static str;

    /// Registers a new external surface with the backend.
    fn register_external_surface(
        &mut self,
        id: ExternalSurfaceId,
        handle: ExternalSurfaceHandle,
        descriptor: ExternalSurfaceDescriptor,
    );

    /// Updates an existing external surface.
    fn update_external_surface(
        &mut self,
        id: ExternalSurfaceId,
        handle: ExternalSurfaceHandle,
        descriptor: ExternalSurfaceDescriptor,
    );

    /// Registers a compositor layer with the backend.
    fn register_layer(&mut self, id: CompositorLayerId, descriptor: CompositorLayerDescriptor);

    /// Updates an existing compositor layer.
    fn update_layer(&mut self, id: CompositorLayerId, descriptor: CompositorLayerDescriptor);

    /// Marks a compositor layer dirty.
    fn mark_layer_dirty(&mut self, id: CompositorLayerId);

    /// Signals that a new frame is ready on an external surface.
    fn notify_external_surface_ready(&mut self, id: ExternalSurfaceId);

    /// Signals that Floem has queued a compositor frame.
    fn request_frame(&mut self, reason: FrameRequestReason);

    /// Clears layer dirty bits after a frame has been consumed.
    fn clear_layer_dirtiness(&mut self);

    /// Updates compositor timing after presentation has advanced.
    fn update_timing(&mut self, timing: CompositorTiming);
}
