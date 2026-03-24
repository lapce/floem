//! Backend contract for Floem's compositor layer.
//!
//! Floem keeps compositor identity, frame requests, and app-facing policy in
//! [`crate::compositor`]. Backends sit underneath that layer and mirror the
//! retained compositor state into a concrete presentation engine such as
//! `subduction`.

use std::time::{Duration, Instant};

use floem_renderer::gpu_resources::GpuResources;
use peniko::kurbo::Rect;

use super::{
    CompositorLayerDescriptor, CompositorLayerId, CompositorTiming, ExternalSurfaceDescriptor,
    ExternalSurfaceHandle, ExternalSurfaceId, FrameRequestReason,
};

pub type FloemPaintedSurfaceVisitor<'a> =
    dyn FnMut(CompositorLayerId, Rect, (u32, u32), wgpu::TextureFormat, &wgpu::TextureView) + 'a;

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

    /// Attaches shared wgpu resources so the backend can prepare any presenter-
    /// side resources against Floem's device and queue.
    fn attach_wgpu_presenter(
        &mut self,
        _gpu_resources: &GpuResources,
        _output_format: wgpu::TextureFormat,
        _output_size: (u32, u32),
    ) {
    }

    /// Called before Floem paints a frame.
    fn begin_frame(&mut self, _output_size: (u32, u32), _started_at: Instant) {}

    /// Called after Floem completes and presents a frame.
    fn finish_frame(
        &mut self,
        _output_size: (u32, u32),
        _started_at: Instant,
        _completed_at: Instant,
    ) {
    }

    /// Applies retained layer updates so compositor-owned Floem surfaces exist
    /// before the paint phase tries to rasterize into them.
    fn prepare_floem_surfaces(&mut self, _output_size: (u32, u32)) {}

    /// Returns a backend-specific frame interval hint when available.
    fn preferred_frame_interval(&self) -> Option<Duration> {
        None
    }

    /// Iterates compositor-owned surfaces that expect Floem to rasterize content
    /// into them before final composition.
    fn for_each_floem_painted_surface(&self, _visit: &mut FloemPaintedSurfaceVisitor<'_>) {}

    /// Composites the current backend layer tree into the provided output view.
    fn composite_to_output(&mut self, _output: &wgpu::TextureView) {}
}
