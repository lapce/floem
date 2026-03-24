//! Compositor-facing API and retained compositor state.
//!
//! This module is the public entry point for composition-level concepts that sit
//! above paint recording and renderer backends. Paint/display-list code is focused
//! on describing Floem-drawn content. The compositor is responsible for deciding
//! how that content, external surfaces, timing information, and presentation are
//! orchestrated into frames.
//!
//! The immediate motivation for this module is external media, especially video.
//! A video editor needs to hand Floem an externally produced surface or texture,
//! mark that content ready, and let Floem drive the frame lifecycle from there.
//! The compositor is the right place to express that workflow because it owns:
//! - layer identity
//! - frame requests
//! - presentation timing
//! - the boundary between retained paint artifacts and final composition
//!
//! ## Current scope
//!
//! The compositor module currently provides:
//! - stable ids for layers and external surfaces
//! - descriptors for layer kinds and external surface metadata
//! - a retained registry of compositor layers and external surfaces
//! - a frame request queue with reasons, including "external surface ready"
//! - frame timing information that can be queried by higher-level code
//!
//! This is an API and state-model introduction. It does not yet own final platform
//! presentation or backend-specific composition. That work will come later when
//! Floem's renderer/compositor split is made explicit.
//!
//! ## Intended workflow
//!
//! The intended usage is:
//! 1. Application registers an external surface or texture source.
//! 2. Application associates it with one or more compositor layers.
//! 3. External producer signals that a frame is ready.
//! 4. Floem schedules a frame through the normal update/paint/composite path.
//! 5. The compositor decides what must be repainted, rerasterized, or recomposited.
//! 6. Floem presents and updates timing information.
//!
//! ## Where this is going
//!
//! This module is meant to grow into:
//! - a retained layer tree
//! - external surface integration for GPU/native surfaces
//! - frame pacing / present feedback APIs
//! - compositor scheduling and layer dirtiness
//! - platform/backend composition hooks
//! - the owner of "texture ready" style frame requests
//!
//! In other words, this module is the start of Floem's compositor boundary.

pub mod backend;
#[cfg(feature = "subduction")]
pub mod subduction;

use std::time::{Duration, Instant};

use floem_renderer::gpu_resources::GpuResources;
use peniko::BlendMode;
use peniko::kurbo::{Rect, Size};
use rustc_hash::FxHashMap;

use self::backend::{CompositorBackend, FloemPaintedSurfaceVisitor};
use crate::ElementId;

/// Stable identifier for a compositor layer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompositorLayerId(u64);

impl CompositorLayerId {
    /// Returns the raw numeric identifier.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stable identifier for an externally managed surface.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalSurfaceId(u64);

impl ExternalSurfaceId {
    /// Returns the raw numeric identifier.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Pixel format hint for an external surface.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalPixelFormat {
    #[default]
    Unknown,
    Rgba8Unorm,
    Bgra8Unorm,
    Rgba16Float,
    Nv12,
    P010,
    Custom(u32),
}

/// Alpha handling for an external surface.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalAlphaMode {
    #[default]
    Unknown,
    Opaque,
    Straight,
    Premultiplied,
}

/// Alpha handling for compositor layer contents.
///
/// This is separate from [`ExternalAlphaMode`] because compositor layers are the
/// unit the backend actually composites. A Floem-painted layer may be straight-
/// alpha even when an external producer uses a different convention.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositorAlphaMode {
    #[default]
    Premultiplied,
    Straight,
}

/// Color-space hint for an external surface.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalColorSpace {
    #[default]
    Unknown,
    Srgb,
    DisplayP3,
    Rec709,
    Rec2020,
    Custom(u32),
}

/// Opaque handle type describing how an external surface is represented.
///
/// This is intentionally backend-agnostic for now. Platform- or backend-specific
/// composition code can interpret these handles later without forcing the public
/// compositor API to depend on a single graphics backend.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalSurfaceHandle {
    #[default]
    Unspecified,
    Opaque(u64),
}

/// Metadata describing an externally managed compositing surface.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ExternalSurfaceDescriptor {
    pub size: Size,
    pub pixel_format: ExternalPixelFormat,
    pub alpha_mode: ExternalAlphaMode,
    pub color_space: ExternalColorSpace,
    pub frame_rate_hint: Option<f64>,
}

/// How a compositor layer gets its contents.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CompositorLayerKind {
    /// Layer contents come from normal Floem paint artifacts.
    #[default]
    FloemPainted,
    /// Layer contents come from a registered external surface.
    ExternalSurface {
        surface_id: ExternalSurfaceId,
    },
    /// Layer combines an external surface with Floem-drawn overlays or underlays.
    Mixed {
        surface_id: ExternalSurfaceId,
    },
}

/// How a compositor layer is realized by the compositor/backend.
///
/// This is intentionally separate from [`CompositorLayerKind`]. A Floem-painted
/// layer, an external surface layer, and a mixed layer can all be realized as a
/// texture-backed compositor surface or as a platform/native layer depending on
/// backend capabilities and the use case.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositorLayerBacking {
    /// Backed by an internal compositor texture/surface that Floem can raster
    /// into and composite with normal texture-space operations.
    #[default]
    TextureBacked,
    /// Backed by a platform/native compositor layer or externally managed
    /// presentable surface.
    PlatformSurface,
}

/// Capability summary for a realized compositor layer backing.
///
/// Keeping these capabilities explicit avoids conflating texture-backed layers
/// with host/platform layers. The compositor can branch on actual support
/// instead of assuming all layers permit the same operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositorLayerCapabilities {
    pub supports_opacity: bool,
    pub supports_blend_mode: bool,
    pub supports_filters: bool,
    pub supports_clip_to_bounds: bool,
    pub supports_direct_raster: bool,
    pub supports_external_surface_import: bool,
}

impl Default for CompositorLayerCapabilities {
    fn default() -> Self {
        Self::for_backing(CompositorLayerBacking::TextureBacked)
    }
}

impl CompositorLayerCapabilities {
    #[must_use]
    pub const fn for_backing(backing: CompositorLayerBacking) -> Self {
        match backing {
            CompositorLayerBacking::TextureBacked => Self {
                supports_opacity: true,
                supports_blend_mode: true,
                supports_filters: true,
                supports_clip_to_bounds: true,
                supports_direct_raster: true,
                supports_external_surface_import: true,
            },
            CompositorLayerBacking::PlatformSurface => Self {
                supports_opacity: false,
                supports_blend_mode: false,
                supports_filters: false,
                supports_clip_to_bounds: true,
                supports_direct_raster: false,
                supports_external_surface_import: true,
            },
        }
    }
}

/// Retained descriptor for a compositor layer.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CompositorLayerDescriptor {
    pub kind: CompositorLayerKind,
    pub backing: CompositorLayerBacking,
    pub capabilities: CompositorLayerCapabilities,
    pub bounds: Rect,
    pub compositor_clip: Option<Rect>,
    pub z_index: i32,
    pub compositing_depth: u32,
    pub opacity: f32,
    pub alpha_mode: CompositorAlphaMode,
    pub blend_mode: BlendMode,
    pub isolated: bool,
}

/// Why the compositor wants a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameRequestReason {
    AppRequested,
    Animation,
    Resize,
    Damage,
    ExternalSurfaceReady(ExternalSurfaceId),
    LayerDirty(CompositorLayerId),
}

/// Presentation timing state exposed by the compositor.
#[derive(Debug, Default, Clone, Copy)]
pub struct CompositorTiming {
    pub frame_interval: Option<Duration>,
    pub predicted_next_present: Option<Instant>,
    pub last_present_started: Option<Instant>,
    pub last_present_completed: Option<Instant>,
    pub frame_number: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ExternalSurfaceState {
    pub handle: ExternalSurfaceHandle,
    pub descriptor: ExternalSurfaceDescriptor,
    pub latest_ready_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy)]
pub struct CompositorLayerState {
    pub descriptor: CompositorLayerDescriptor,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloemPaintedSurfaceRole {
    Root,
    Overlay,
    Promoted(ElementId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedPromotedLayer {
    pub element_id: ElementId,
    pub bounds: Rect,
    pub raster_clip: Option<Rect>,
    pub compositor_clip: Option<Rect>,
    pub z_index: i32,
    pub compositing_depth: u32,
    pub backing: CompositorLayerBacking,
    pub isolated: bool,
    pub alpha_mode: CompositorAlphaMode,
}

pub(crate) type FloemSurfaceRoleVisitor<'a> =
    dyn FnMut(FloemPaintedSurfaceRole, Rect, (u32, u32), wgpu::TextureFormat, &wgpu::TextureView)
        + 'a;

/// Retained compositor registry and frame-request state.
#[derive(Default)]
pub struct Compositor {
    next_layer_id: u64,
    next_surface_id: u64,
    layers: FxHashMap<CompositorLayerId, CompositorLayerState>,
    external_surfaces: FxHashMap<ExternalSurfaceId, ExternalSurfaceState>,
    promoted_layers: FxHashMap<ElementId, CompositorLayerId>,
    root_layer: Option<CompositorLayerId>,
    overlay_layer: Option<CompositorLayerId>,
    pending_frame_reasons: Vec<FrameRequestReason>,
    timing: CompositorTiming,
    backend: Option<Box<dyn CompositorBackend>>,
}

impl std::fmt::Debug for Compositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compositor")
            .field("next_layer_id", &self.next_layer_id)
            .field("next_surface_id", &self.next_surface_id)
            .field("layers", &self.layers)
            .field("external_surfaces", &self.external_surfaces)
            .field("promoted_layers", &self.promoted_layers)
            .field("root_layer", &self.root_layer)
            .field("overlay_layer", &self.overlay_layer)
            .field("pending_frame_reasons", &self.pending_frame_reasons)
            .field("timing", &self.timing)
            .field("backend_name", &self.backend_name())
            .finish()
    }
}

impl Compositor {
    /// Installs a compositor backend and synchronizes the retained state into it.
    pub fn install_backend(&mut self, mut backend: Box<dyn CompositorBackend>) {
        for (&id, surface) in &self.external_surfaces {
            backend.register_external_surface(id, surface.handle, surface.descriptor);
        }

        for (&id, layer) in &self.layers {
            backend.register_layer(id, layer.descriptor);
            if layer.dirty {
                backend.mark_layer_dirty(id);
            }
        }

        for &reason in &self.pending_frame_reasons {
            backend.request_frame(reason);
        }

        backend.update_timing(self.timing);
        self.backend = Some(backend);
    }

    /// Attaches shared wgpu presentation resources to the installed backend.
    pub fn attach_wgpu_presenter(
        &mut self,
        gpu_resources: &GpuResources,
        output_format: wgpu::TextureFormat,
        output_size: (u32, u32),
    ) {
        if let Some(backend) = self.backend.as_mut() {
            backend.attach_wgpu_presenter(gpu_resources, output_format, output_size);
        }
    }

    /// Synchronizes compositor-owned Floem layers from retained display-list
    /// layer candidates.
    pub(crate) fn sync_promoted_layers(&mut self, promoted_layers: &[ResolvedPromotedLayer]) {
        let mut active = FxHashMap::default();

        for promoted in promoted_layers {
            let descriptor = CompositorLayerDescriptor {
                kind: CompositorLayerKind::FloemPainted,
                backing: promoted.backing,
                capabilities: CompositorLayerCapabilities::for_backing(promoted.backing),
                bounds: promoted.bounds,
                compositor_clip: promoted.compositor_clip,
                z_index: promoted.z_index,
                compositing_depth: promoted.compositing_depth,
                opacity: 1.0,
                alpha_mode: promoted.alpha_mode,
                blend_mode: BlendMode::default(),
                isolated: promoted.isolated,
            };

            let layer_id = if let Some(layer_id) = self.promoted_layers.get(&promoted.element_id).copied() {
                let _ = self.update_layer(layer_id, descriptor);
                layer_id
            } else {
                let layer_id = self.register_layer(descriptor);
                self.promoted_layers.insert(promoted.element_id, layer_id);
                layer_id
            };

            active.insert(promoted.element_id, layer_id);
        }

        let stale: Vec<_> = self
            .promoted_layers
            .iter()
            .filter_map(|(&element_id, &layer_id)| (!active.contains_key(&element_id)).then_some((element_id, layer_id)))
            .collect();

        for (element_id, layer_id) in stale {
            let _ = self.update_layer(
                layer_id,
                CompositorLayerDescriptor {
                    kind: CompositorLayerKind::FloemPainted,
                    backing: CompositorLayerBacking::TextureBacked,
                    capabilities: CompositorLayerCapabilities::for_backing(
                        CompositorLayerBacking::TextureBacked,
                    ),
                    bounds: Rect::ZERO,
                    compositor_clip: None,
                    z_index: 0,
                    compositing_depth: 0,
                    opacity: 0.0,
                    alpha_mode: CompositorAlphaMode::Straight,
                    blend_mode: BlendMode::default(),
                    isolated: false,
                },
            );
            self.promoted_layers.remove(&element_id);
        }
    }

    pub(crate) fn ensure_root_layer(&mut self, bounds: Rect) -> CompositorLayerId {
        let descriptor = CompositorLayerDescriptor {
            kind: CompositorLayerKind::FloemPainted,
            backing: CompositorLayerBacking::TextureBacked,
            capabilities: CompositorLayerCapabilities::for_backing(
                CompositorLayerBacking::TextureBacked,
            ),
            bounds,
            compositor_clip: None,
            z_index: i32::MIN,
            compositing_depth: 0,
            opacity: 1.0,
            alpha_mode: CompositorAlphaMode::Straight,
            blend_mode: BlendMode::default(),
            isolated: false,
        };

        if let Some(layer_id) = self.root_layer {
            let _ = self.update_layer(layer_id, descriptor);
            layer_id
        } else {
            let layer_id = self.register_layer(descriptor);
            self.root_layer = Some(layer_id);
            layer_id
        }
    }

    pub(crate) fn ensure_overlay_layer(&mut self, bounds: Rect) -> CompositorLayerId {
        let descriptor = CompositorLayerDescriptor {
            kind: CompositorLayerKind::FloemPainted,
            backing: CompositorLayerBacking::TextureBacked,
            capabilities: CompositorLayerCapabilities::for_backing(
                CompositorLayerBacking::TextureBacked,
            ),
            bounds,
            compositor_clip: None,
            z_index: i32::MAX,
            compositing_depth: 0,
            opacity: 1.0,
            alpha_mode: CompositorAlphaMode::Straight,
            blend_mode: BlendMode::default(),
            isolated: false,
        };

        if let Some(layer_id) = self.overlay_layer {
            let _ = self.update_layer(layer_id, descriptor);
            layer_id
        } else {
            let layer_id = self.register_layer(descriptor);
            self.overlay_layer = Some(layer_id);
            layer_id
        }
    }

    pub(crate) fn for_each_floem_painted_surface(
        &self,
        visit: &mut FloemSurfaceRoleVisitor<'_>,
    ) {
        let Some(backend) = self.backend.as_ref() else {
            return;
        };

        let mut visit_backend: &mut FloemPaintedSurfaceVisitor<'_> =
            &mut |layer_id, bounds, size, format, view| {
                let Some(layer_state) = self.layers.get(&layer_id) else {
                    return;
                };
                if !matches!(layer_state.descriptor.kind, CompositorLayerKind::FloemPainted)
                    || !layer_state.descriptor.capabilities.supports_direct_raster
                {
                    return;
                }

                let role = if self.root_layer == Some(layer_id) {
                    Some(FloemPaintedSurfaceRole::Root)
                } else if self.overlay_layer == Some(layer_id) {
                    Some(FloemPaintedSurfaceRole::Overlay)
                } else {
                    self.promoted_layers
                        .iter()
                    .find_map(|(&element_id, &promoted_layer_id)| {
                        (promoted_layer_id == layer_id)
                            .then_some(FloemPaintedSurfaceRole::Promoted(element_id))
                    })
            };

            if let Some(role) = role {
                visit(role, bounds, size, format, view);
            }
        };
        backend.for_each_floem_painted_surface(&mut visit_backend);
    }

    pub(crate) fn promoted_layer_ids(&self) -> Vec<ElementId> {
        self.promoted_layers.keys().copied().collect()
    }

    /// Returns the installed backend name, if any.
    #[must_use]
    pub fn backend_name(&self) -> Option<&'static str> {
        self.backend.as_ref().map(|backend| backend.name())
    }

    /// Returns the preferred compositor frame interval if a backend can provide one.
    #[must_use]
    pub fn preferred_frame_interval(&self) -> Option<Duration> {
        self.backend
            .as_ref()
            .and_then(|backend| backend.preferred_frame_interval())
            .or(self.timing.frame_interval)
    }

    /// Registers a new externally managed surface and returns its id.
    pub fn register_external_surface(
        &mut self,
        handle: ExternalSurfaceHandle,
        descriptor: ExternalSurfaceDescriptor,
    ) -> ExternalSurfaceId {
        let id = ExternalSurfaceId(self.next_surface_id);
        self.next_surface_id += 1;
        self.external_surfaces.insert(
            id,
            ExternalSurfaceState {
                handle,
                descriptor,
                latest_ready_at: None,
            },
        );
        if let Some(backend) = self.backend.as_mut() {
            backend.register_external_surface(id, handle, descriptor);
        }
        id
    }

    /// Updates the metadata for an existing external surface.
    pub fn update_external_surface(
        &mut self,
        id: ExternalSurfaceId,
        handle: ExternalSurfaceHandle,
        descriptor: ExternalSurfaceDescriptor,
    ) -> bool {
        let Some(surface) = self.external_surfaces.get_mut(&id) else {
            return false;
        };
        surface.handle = handle;
        surface.descriptor = descriptor;
        if let Some(backend) = self.backend.as_mut() {
            backend.update_external_surface(id, handle, descriptor);
        }
        true
    }

    /// Returns the retained state for an external surface.
    pub fn external_surface(&self, id: ExternalSurfaceId) -> Option<&ExternalSurfaceState> {
        self.external_surfaces.get(&id)
    }

    /// Registers a compositor layer and returns its id.
    pub fn register_layer(
        &mut self,
        descriptor: CompositorLayerDescriptor,
    ) -> CompositorLayerId {
        let id = CompositorLayerId(self.next_layer_id);
        self.next_layer_id += 1;
        self.layers.insert(
            id,
            CompositorLayerState {
                descriptor,
                dirty: true,
            },
        );
        if let Some(backend) = self.backend.as_mut() {
            backend.register_layer(id, descriptor);
            backend.mark_layer_dirty(id);
        }
        id
    }

    /// Updates the retained descriptor for a compositor layer.
    pub fn update_layer(
        &mut self,
        id: CompositorLayerId,
        descriptor: CompositorLayerDescriptor,
    ) -> bool {
        let Some(layer) = self.layers.get_mut(&id) else {
            return false;
        };
        layer.descriptor = descriptor;
        layer.dirty = true;
        if let Some(backend) = self.backend.as_mut() {
            backend.update_layer(id, descriptor);
            backend.mark_layer_dirty(id);
        }
        true
    }

    /// Returns the retained state for a compositor layer.
    pub fn layer(&self, id: CompositorLayerId) -> Option<&CompositorLayerState> {
        self.layers.get(&id)
    }

    /// Marks a layer dirty and schedules a compositor frame.
    pub fn mark_layer_dirty(&mut self, id: CompositorLayerId) {
        if let Some(layer) = self.layers.get_mut(&id) {
            layer.dirty = true;
            self.pending_frame_reasons
                .push(FrameRequestReason::LayerDirty(id));
            if let Some(backend) = self.backend.as_mut() {
                backend.mark_layer_dirty(id);
                backend.request_frame(FrameRequestReason::LayerDirty(id));
            }
        }
    }

    /// Notifies the compositor that a new frame is ready on an external surface.
    pub fn notify_external_surface_ready(&mut self, id: ExternalSurfaceId) -> bool {
        let Some(surface) = self.external_surfaces.get_mut(&id) else {
            return false;
        };
        surface.latest_ready_at = Some(Instant::now());
        self.pending_frame_reasons
            .push(FrameRequestReason::ExternalSurfaceReady(id));
        if let Some(backend) = self.backend.as_mut() {
            backend.notify_external_surface_ready(id);
            backend.request_frame(FrameRequestReason::ExternalSurfaceReady(id));
        }
        true
    }

    /// Requests a compositor frame for an explicit reason.
    pub fn request_frame(&mut self, reason: FrameRequestReason) {
        self.pending_frame_reasons.push(reason);
        if let Some(backend) = self.backend.as_mut() {
            backend.request_frame(reason);
        }
    }

    /// Returns whether a compositor frame has been requested.
    pub fn has_pending_frame(&self) -> bool {
        !self.pending_frame_reasons.is_empty()
    }

    /// Drains and returns pending frame reasons.
    pub fn take_pending_frame_reasons(&mut self) -> Vec<FrameRequestReason> {
        std::mem::take(&mut self.pending_frame_reasons)
    }

    /// Clears the dirty bit for all retained layers.
    pub fn clear_layer_dirtiness(&mut self) {
        for layer in self.layers.values_mut() {
            layer.dirty = false;
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.clear_layer_dirtiness();
        }
    }

    /// Returns the most recent compositor timing state.
    pub const fn timing(&self) -> &CompositorTiming {
        &self.timing
    }

    /// Updates compositor timing state after presentation work has advanced.
    pub fn update_timing(&mut self, timing: CompositorTiming) {
        self.timing = timing;
        if let Some(backend) = self.backend.as_mut() {
            backend.update_timing(timing);
        }
    }

    /// Marks the start of a compositor-driven frame.
    pub fn begin_frame(&mut self, output_size: (u32, u32), started_at: Instant) {
        let mut timing = self.timing;
        timing.last_present_started = Some(started_at);
        timing.predicted_next_present = timing.frame_interval.map(|interval| started_at + interval);
        self.update_timing(timing);

        if let Some(backend) = self.backend.as_mut() {
            backend.begin_frame(output_size, started_at);
        }
    }

    pub(crate) fn prepare_floem_surfaces(&mut self, output_size: (u32, u32)) {
        if let Some(backend) = self.backend.as_mut() {
            backend.prepare_floem_surfaces(output_size);
        }
    }

    /// Marks the completion of a compositor-driven frame.
    pub fn finish_frame(
        &mut self,
        output_size: (u32, u32),
        started_at: Instant,
        completed_at: Instant,
    ) {
        let mut timing = self.timing;
        if let Some(last_completed) = timing.last_present_completed {
            timing.frame_interval = Some(completed_at.saturating_duration_since(last_completed));
        }
        timing.last_present_started = Some(started_at);
        timing.last_present_completed = Some(completed_at);
        timing.frame_number = timing.frame_number.saturating_add(1);
        timing.predicted_next_present = timing.frame_interval.map(|interval| completed_at + interval);
        self.update_timing(timing);

        if let Some(backend) = self.backend.as_mut() {
            backend.finish_frame(output_size, started_at, completed_at);
        }
    }

    pub(crate) fn composite_to_output(&mut self, output: &wgpu::TextureView) {
        if let Some(backend) = self.backend.as_mut() {
            backend.composite_to_output(output);
        }
    }
}
