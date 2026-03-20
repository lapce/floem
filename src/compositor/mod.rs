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

use std::time::{Duration, Instant};

use peniko::BlendMode;
use peniko::kurbo::{Rect, Size};
use rustc_hash::FxHashMap;

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

/// Retained descriptor for a compositor layer.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CompositorLayerDescriptor {
    pub kind: CompositorLayerKind,
    pub bounds: Rect,
    pub z_index: i32,
    pub opacity: f32,
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

/// Retained compositor registry and frame-request state.
#[derive(Debug, Default)]
pub struct Compositor {
    next_layer_id: u64,
    next_surface_id: u64,
    layers: FxHashMap<CompositorLayerId, CompositorLayerState>,
    external_surfaces: FxHashMap<ExternalSurfaceId, ExternalSurfaceState>,
    pending_frame_reasons: Vec<FrameRequestReason>,
    timing: CompositorTiming,
}

impl Compositor {
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
        true
    }

    /// Requests a compositor frame for an explicit reason.
    pub fn request_frame(&mut self, reason: FrameRequestReason) {
        self.pending_frame_reasons.push(reason);
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
    }

    /// Returns the most recent compositor timing state.
    pub const fn timing(&self) -> &CompositorTiming {
        &self.timing
    }

    /// Updates compositor timing state after presentation work has advanced.
    pub fn update_timing(&mut self, timing: CompositorTiming) {
        self.timing = timing;
    }
}
