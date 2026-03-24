//! `subduction` backend adapter for Floem's compositor state.
//!
//! This adapter keeps Floem's compositor API backend-agnostic while mirroring
//! retained layers and surfaces into `subduction_core`'s `LayerStore`.
//!
//! The current integration scope is intentionally narrow:
//! - stable Floem compositor ids are mapped to `subduction` layer/surface ids
//! - layer descriptors are mirrored into `LayerStore` topology and properties
//! - external surfaces are mapped onto `subduction` `SurfaceId`s
//! - frame reasons and timing are retained so the future presenter/frame loop
//!   can consume them from one place
//!
//! This is enough to establish the architectural seam. It is not yet full
//! presentation integration.

use peniko::kurbo::{Rect, RoundedRect};
use rustc_hash::FxHashMap;
use subduction_backend_wgpu::WgpuPresenter;
use subduction_core::{
    backend::Presenter as _,
    layer::{ClipShape, FrameChanges, LayerFlags, LayerId, LayerStore, SurfaceId},
    transform::Transform3d,
};

use super::{
    CompositorAlphaMode, CompositorLayerBacking, CompositorLayerDescriptor, CompositorLayerId,
    CompositorLayerKind, CompositorTiming, ExternalSurfaceDescriptor, ExternalSurfaceHandle,
    ExternalSurfaceId, FrameRequestReason,
    backend::{CompositorBackend, FloemPaintedSurfaceVisitor},
};

/// `subduction`-backed compositor mirror.
#[derive(Debug)]
pub struct SubductionCompositorBackend {
    store: LayerStore,
    root_layer: LayerId,
    layer_ids: FxHashMap<CompositorLayerId, LayerId>,
    layer_descriptors: FxHashMap<CompositorLayerId, CompositorLayerDescriptor>,
    floem_surface_ids: FxHashMap<CompositorLayerId, SurfaceId>,
    surface_ids: FxHashMap<ExternalSurfaceId, SurfaceId>,
    surface_descriptors: FxHashMap<ExternalSurfaceId, ExternalSurfaceDescriptor>,
    surface_handles: FxHashMap<ExternalSurfaceId, ExternalSurfaceHandle>,
    next_surface_id: u32,
    pending_frame_reasons: Vec<FrameRequestReason>,
    timing: CompositorTiming,
    needs_reorder: bool,
    presenter: Option<WgpuPresenter>,
}

impl Default for SubductionCompositorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SubductionCompositorBackend {
    /// Creates a new backend adapter with a synthetic root layer.
    #[must_use]
    pub fn new() -> Self {
        let mut store = LayerStore::new();
        let root_layer = store.create_layer();

        Self {
            store,
            root_layer,
            layer_ids: FxHashMap::default(),
            layer_descriptors: FxHashMap::default(),
            floem_surface_ids: FxHashMap::default(),
            surface_ids: FxHashMap::default(),
            surface_descriptors: FxHashMap::default(),
            surface_handles: FxHashMap::default(),
            next_surface_id: 1,
            pending_frame_reasons: Vec::new(),
            timing: CompositorTiming::default(),
            needs_reorder: false,
            presenter: None,
        }
    }

    /// Returns the mirrored `subduction` layer store.
    #[must_use]
    pub const fn store(&self) -> &LayerStore {
        &self.store
    }

    /// Returns the current compositor timing mirrored into the backend.
    #[must_use]
    pub const fn timing(&self) -> &CompositorTiming {
        &self.timing
    }

    /// Returns the `subduction` surface id mapped from a Floem external surface.
    #[must_use]
    pub fn surface_id_for(&self, id: ExternalSurfaceId) -> Option<SurfaceId> {
        self.surface_ids.get(&id).copied()
    }

    /// Evaluates the mirrored `subduction` layer store.
    pub fn evaluate(&mut self) -> FrameChanges {
        self.sync_order_if_needed();
        self.store.evaluate()
    }

    fn register_surface_id(&mut self, id: ExternalSurfaceId) -> SurfaceId {
        if let Some(surface_id) = self.surface_ids.get(&id).copied() {
            return surface_id;
        }

        let surface_id = SurfaceId(self.next_surface_id);
        self.next_surface_id += 1;
        self.surface_ids.insert(id, surface_id);
        surface_id
    }

    fn ensure_layer_id(&mut self, id: CompositorLayerId) -> LayerId {
        if let Some(layer_id) = self.layer_ids.get(&id).copied() {
            return layer_id;
        }

        let layer_id = self.store.create_layer();
        self.store.add_child(self.root_layer, layer_id);
        self.layer_ids.insert(id, layer_id);
        self.needs_reorder = true;
        layer_id
    }

    fn apply_layer_descriptor(
        &mut self,
        id: CompositorLayerId,
        descriptor: CompositorLayerDescriptor,
    ) {
        let layer_id = self.ensure_layer_id(id);
        self.layer_descriptors.insert(id, descriptor);

        // Floem layer bounds are rects; `subduction` splits that into a
        // translation (rect origin) and a local size (rect dimensions).
        let bounds = descriptor.bounds;
        let is_empty = !bounds.is_finite() || bounds.width() <= 0.0 || bounds.height() <= 0.0;
        self.store.set_transform(
            layer_id,
            Transform3d::from_translation(bounds.x0, bounds.y0, 0.0),
        );
        self.store.set_bounds(
            layer_id,
            if is_empty {
                (1.0, 1.0).into()
            } else {
                bounds.size()
            },
        );
        self.store.set_opacity(layer_id, descriptor.opacity);
        self.store.set_clip(
            layer_id,
            clip_shape_for_bounds(
                bounds,
                descriptor.compositor_clip,
                descriptor.isolated && descriptor.capabilities.supports_clip_to_bounds,
            ),
        );
        self.store.set_flags(
            layer_id,
            LayerFlags {
                hidden: descriptor.opacity <= 0.0 || is_empty,
            },
        );
        let content = self.content_for_kind(id, descriptor.kind);
        self.store.set_content(layer_id, content);
        if matches!(descriptor.alpha_mode, CompositorAlphaMode::Straight) {
            // The current `subduction_backend_wgpu` presenter composites all layers as
            // premultiplied-alpha textures. Retaining the alpha mode here lets Floem
            // describe the true contract now so backend work can make composition
            // alpha-mode-aware instead of relying on this implicit assumption.
        }
        self.needs_reorder = true;
    }

    fn content_for_kind(
        &mut self,
        id: CompositorLayerId,
        kind: CompositorLayerKind,
    ) -> Option<SurfaceId> {
        let backing = self
            .layer_descriptors
            .get(&id)
            .map(|descriptor| descriptor.backing)
            .unwrap_or(CompositorLayerBacking::TextureBacked);
        match kind {
            CompositorLayerKind::FloemPainted => {
                matches!(backing, CompositorLayerBacking::TextureBacked)
                    .then(|| self.floem_surface_id_for(id))
            }
            CompositorLayerKind::ExternalSurface { surface_id }
            | CompositorLayerKind::Mixed { surface_id } => {
                self.surface_ids.get(&surface_id).copied()
            }
        }
    }

    fn floem_surface_id_for(&mut self, id: CompositorLayerId) -> SurfaceId {
        if let Some(surface_id) = self.floem_surface_ids.get(&id).copied() {
            return surface_id;
        }

        let surface_id = SurfaceId(self.next_surface_id);
        self.next_surface_id += 1;
        self.floem_surface_ids.insert(id, surface_id);
        surface_id
    }

    fn sync_order_if_needed(&mut self) {
        if !self.needs_reorder {
            return;
        }

        let mut ordered_layers: Vec<_> = self
            .layer_descriptors
            .iter()
            .map(|(&id, &descriptor)| (id, descriptor))
            .collect();
        ordered_layers.sort_by_key(|(id, descriptor)| {
            (
                descriptor.z_index,
                descriptor.compositing_depth,
                id.as_u64(),
            )
        });

        for (id, _) in ordered_layers {
            let Some(layer_id) = self.layer_ids.get(&id).copied() else {
                continue;
            };

            if self.store.parent(layer_id).is_some() {
                self.store.remove_from_parent(layer_id);
            }
            self.store.add_child(self.root_layer, layer_id);
        }

        self.needs_reorder = false;
    }
}

impl CompositorBackend for SubductionCompositorBackend {
    fn name(&self) -> &'static str {
        "subduction"
    }

    fn register_external_surface(
        &mut self,
        id: ExternalSurfaceId,
        handle: ExternalSurfaceHandle,
        descriptor: ExternalSurfaceDescriptor,
    ) {
        self.register_surface_id(id);
        self.surface_handles.insert(id, handle);
        self.surface_descriptors.insert(id, descriptor);
    }

    fn update_external_surface(
        &mut self,
        id: ExternalSurfaceId,
        handle: ExternalSurfaceHandle,
        descriptor: ExternalSurfaceDescriptor,
    ) {
        self.register_surface_id(id);
        self.surface_handles.insert(id, handle);
        self.surface_descriptors.insert(id, descriptor);

        let affected_layers: Vec<_> = self
            .layer_descriptors
            .iter()
            .filter_map(|(&layer_id, descriptor)| match descriptor.kind {
                CompositorLayerKind::ExternalSurface { surface_id }
                | CompositorLayerKind::Mixed { surface_id }
                    if surface_id == id =>
                {
                    Some((layer_id, *descriptor))
                }
                _ => None,
            })
            .collect();

        for (layer_id, descriptor) in affected_layers {
            self.apply_layer_descriptor(layer_id, descriptor);
        }
    }

    fn register_layer(&mut self, id: CompositorLayerId, descriptor: CompositorLayerDescriptor) {
        self.apply_layer_descriptor(id, descriptor);
    }

    fn update_layer(&mut self, id: CompositorLayerId, descriptor: CompositorLayerDescriptor) {
        self.apply_layer_descriptor(id, descriptor);
    }

    fn mark_layer_dirty(&mut self, _id: CompositorLayerId) {}

    fn notify_external_surface_ready(&mut self, id: ExternalSurfaceId) {
        self.pending_frame_reasons
            .push(FrameRequestReason::ExternalSurfaceReady(id));
    }

    fn request_frame(&mut self, reason: FrameRequestReason) {
        self.pending_frame_reasons.push(reason);
    }

    fn clear_layer_dirtiness(&mut self) {}

    fn update_timing(&mut self, timing: CompositorTiming) {
        self.timing = timing;
    }

    fn attach_wgpu_presenter(
        &mut self,
        gpu_resources: &floem_renderer::gpu_resources::GpuResources,
        output_format: wgpu::TextureFormat,
        output_size: (u32, u32),
    ) {
        let default_layer_size = (output_size.0.clamp(1, 2048), output_size.1.clamp(1, 2048));
        self.presenter = Some(WgpuPresenter::new(
            gpu_resources.device.clone(),
            gpu_resources.queue.clone(),
            output_format,
            output_size,
            default_layer_size,
        ));
    }

    fn begin_frame(&mut self, output_size: (u32, u32), _started_at: std::time::Instant) {
        self.prepare_floem_surfaces(output_size);
    }

    fn preferred_frame_interval(&self) -> Option<std::time::Duration> {
        self.timing.frame_interval
    }

    fn prepare_floem_surfaces(&mut self, output_size: (u32, u32)) {
        let has_presenter = self.presenter.is_some();
        if has_presenter {
            let changes = self.evaluate();
            if let Some(presenter) = self.presenter.as_mut() {
                presenter.resize_output(output_size.0, output_size.1);
                presenter.apply(&self.store, &changes);
            }
        }
    }

    fn for_each_floem_painted_surface(&self, visit: &mut FloemPaintedSurfaceVisitor<'_>) {
        let Some(presenter) = self.presenter.as_ref() else {
            return;
        };

        for (&layer_id, &surface_id) in &self.floem_surface_ids {
            let Some(descriptor) = self.layer_descriptors.get(&layer_id).copied() else {
                continue;
            };
            let Some(view) = presenter.texture_for_surface(surface_id) else {
                continue;
            };
            let size = (
                descriptor.bounds.width().max(1.0).round() as u32,
                descriptor.bounds.height().max(1.0).round() as u32,
            );
            visit(
                layer_id,
                descriptor.bounds,
                size,
                presenter.layer_format(),
                view,
            );
        }
    }

    fn composite_to_output(&mut self, output: &wgpu::TextureView) {
        let Some(presenter) = self.presenter.as_mut() else {
            return;
        };
        let command_buffer = presenter.composite(&self.store, output);
        presenter.queue().submit([command_buffer]);
    }
}

fn clip_shape_for_bounds(
    bounds: Rect,
    compositor_clip: Option<Rect>,
    isolated: bool,
) -> Option<ClipShape> {
    if !isolated || !bounds.is_finite() {
        return None;
    }

    let clip = compositor_clip.unwrap_or(bounds);
    let local_clip = clip.with_origin(clip.origin() - bounds.origin().to_vec2());
    let width = local_clip.width();
    let height = local_clip.height();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some(ClipShape::RoundedRect(RoundedRect::from_rect(
        local_clip, 0.0,
    )))
}
