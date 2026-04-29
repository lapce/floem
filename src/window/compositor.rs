use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    effects::ColorEffect,
    external_surface::{ExternalSurfaceContent, ExternalSurfaceId, ExternalTexture},
    gpu_resources::GpuResources,
    paint::{
        composition::{
            CompositionItem, CompositionKey, CompositionPlan, ExternalSurfaceLayer,
            SceneExternalImage, SceneLayer,
        },
        display_list,
        renderer::{ExternalImageResources, RendererTimingRecorder, TimingSpan, WindowRenderer},
    },
};
use imaging::{
    Brush, Composite, ExternalImage, ExternalImageId, ImageBrush, PaintSink, RenderSource,
    record::{Draw, Geometry, Scene},
};
use imaging_wgpu::ResolvedExternalImage;
use peniko::kurbo::{Affine, Rect, Size};
use peniko::{BlendMode, Fill, ImageAlphaType};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use subduction_core::{
    layer::{FrameChanges, LayerId, LayerStore, SurfaceId},
    transform::Transform3d,
};

use super::external_surface::ExternalSurfaceEntry;

static COMPOSITOR_RENDER_CALL_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(crate) struct WindowCompositor {
    layers_by_key: FxHashMap<CompositionKey, CompositorLayerState>,
    layer_ids_by_key: FxHashMap<CompositionKey, LayerId>,
    layer_store: LayerStore,
    root_layer: Option<LayerId>,
    order: Vec<CompositionKey>,
    next_surface_id: u32,

    layer_host: Option<subduction::LayerHost>,
    layer_host_failed: bool,
    unsupported_publications: FxHashSet<UnsupportedPublication>,
    unused_resource_releases: Arc<Mutex<Vec<u64>>>,
    scene_content_by_key: FxHashMap<CompositionKey, ExternalTextureContent>,
    scene_render_signatures: FxHashMap<CompositionKey, SceneRenderSignature>,
    effect_renderer: ColorEffectRenderer,
    pending_layer_changes: Option<FrameChanges>,
    #[cfg(target_os = "macos")]
    metal_capture_active: bool,
}

impl WindowCompositor {
    pub(crate) fn invalidate_scene_content(&mut self) {
        self.scene_content_by_key.clear();
        self.scene_render_signatures.clear();
    }

    pub(crate) fn invalidate_external_surface_content(&mut self, surface_id: ExternalSurfaceId) {
        let keys = self
            .layers_by_key
            .iter()
            .filter_map(|(key, state)| match state {
                CompositorLayerState::Scene(layer)
                    if layer
                        .external_images
                        .iter()
                        .any(|image| image.surface_id == surface_id) =>
                {
                    Some(key.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for key in keys {
            self.scene_content_by_key.remove(&key);
            self.scene_render_signatures.remove(&key);
        }
    }

    pub(crate) fn has_layer_host(&self) -> bool {
        self.layer_host.is_some()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn mark_metal_capture_active(&mut self) {
        self.metal_capture_active = true;
    }

    pub(crate) fn ensure_platform_presenter(
        &mut self,
        window: &(impl raw_window_handle::HasWindowHandle + ?Sized),
    ) {
        if self.layer_host.is_some() || self.layer_host_failed {
            return;
        }
        match subduction::LayerHost::from_window(window) {
            Ok(layer_host) => {
                if crate::frame_clock::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem compositor layer host backend={}",
                        layer_host.backend_name()
                    );
                }
                self.layer_host = Some(layer_host);
            }
            Err(err) => {
                self.layer_host_failed = true;
                eprintln!("floem compositor layer host unavailable: {err}");
            }
        }
    }

    pub(crate) fn apply_plan(
        &mut self,
        plan: &CompositionPlan,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
        _gpu_resources: Option<&GpuResources>,
    ) -> CompositorDiff {
        let mut diff = CompositorDiff::default();
        let mut new_order = Vec::with_capacity(plan.items.len());
        let mut live_keys = FxHashSet::default();

        for item in &plan.items {
            let state = CompositorLayerState::from_item(item, external_surfaces);
            let key = state.key().clone();
            live_keys.insert(key.clone());
            new_order.push(key.clone());

            match self.layers_by_key.get(&key) {
                Some(previous) if previous.equivalent(&state) => {}
                Some(_) => diff.updated.push(key.clone()),
                None => diff.created.push(key.clone()),
            }

            self.layers_by_key.insert(key, state);
        }

        let removed = self
            .order
            .iter()
            .filter(|key| !live_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in &removed {
            self.layers_by_key.remove(key);
            self.scene_content_by_key.remove(key);
            self.scene_render_signatures.remove(key);
            if let Some(layer_id) = self.layer_ids_by_key.remove(key) {
                self.destroy_layer_recursive(layer_id);
            }
        }
        diff.removed = removed;

        if self.order != new_order {
            diff.order_changed = true;
        }
        self.order = new_order;
        let changes = self.sync_layer_store();
        if crate::frame_clock::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor layer tree layers={} roots={} plan_items={} scene_layers={} external_layers={} flattened_external_images={}",
                self.layer_store.len(),
                usize::from(self.root_layer.is_some()),
                self.order.len(),
                self.layers_by_key
                    .values()
                    .filter(|state| matches!(state, CompositorLayerState::Scene(_)))
                    .count(),
                self.layers_by_key
                    .values()
                    .filter(|state| matches!(state, CompositorLayerState::ExternalSurface(_)))
                    .count(),
                self.layers_by_key
                    .values()
                    .map(|state| match state {
                        CompositorLayerState::Scene(scene) => scene.external_images.len(),
                        CompositorLayerState::ExternalSurface(_) => 0,
                    })
                    .sum::<usize>(),
            );
        }
        self.stage_layer_changes(changes);

        diff
    }

    fn stage_layer_changes(&mut self, changes: FrameChanges) {
        let Some(pending) = &mut self.pending_layer_changes else {
            self.pending_layer_changes = Some(changes);
            return;
        };
        merge_frame_changes(pending, changes);
    }

    fn sync_layer_store(&mut self) -> FrameChanges {
        self.ensure_root_layers();
        let root = self.root_layer.expect("root layer is initialized");

        for key in self.order.clone() {
            let Some(state) = self.layers_by_key.get(&key).cloned() else {
                continue;
            };
            let layer_id = if let Some(layer_id) = self.layer_ids_by_key.get(&key).copied() {
                layer_id
            } else {
                let layer_id = self.layer_store.create_layer();
                self.layer_store.add_child(root, layer_id);
                self.layer_ids_by_key.insert(key.clone(), layer_id);
                layer_id
            };
            if self.layer_store.parent(layer_id) != Some(root) {
                self.layer_store.reparent(layer_id, root);
            }
            match state {
                CompositorLayerState::Scene(layer) => {
                    self.sync_scene_layer(layer_id, &layer);
                }
                CompositorLayerState::ExternalSurface(layer) => {
                    self.sync_external_layer(layer_id, &layer);
                }
            }
        }
        self.layer_store.evaluate()
    }

    fn ensure_root_layers(&mut self) {
        if self.root_layer.is_some() {
            return;
        }
        let root = self.layer_store.create_layer();
        self.root_layer = Some(root);
        self.next_surface_id = 0;
    }

    fn ensure_layer_content(&mut self, layer_id: LayerId) -> SurfaceId {
        if let Some(surface_id) = self.layer_store.content(layer_id) {
            return surface_id;
        }
        let surface_id = SurfaceId(self.next_surface_id);
        self.next_surface_id += 1;
        self.layer_store.set_content(layer_id, Some(surface_id));
        surface_id
    }

    pub(crate) fn content_surface_for_key(&self, key: &CompositionKey) -> Option<SurfaceId> {
        let layer_id = self.layer_ids_by_key.get(key).copied()?;
        self.layer_store.content(layer_id)
    }

    pub(crate) fn create_wgpu_surface_frame(
        &mut self,
        device: &wgpu::Device,
        opportunity: subduction::wgpu::SurfaceFrameOpportunity,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Result<subduction::wgpu::SurfaceFrameLease, subduction::wgpu::SurfaceFrameError> {
        self.drain_unused_resource_releases();
        let Some(layer_host) = &mut self.layer_host else {
            return Err(subduction::wgpu::SurfaceFrameError::Unsupported);
        };
        let release_queue = self.unused_resource_releases.clone();
        layer_host
            .create_wgpu_surface_frame(device, opportunity, size, format, usage)
            .map(|lease| {
                lease.with_release(Arc::new(move |resource_key| {
                    if let Ok(mut releases) = release_queue.lock() {
                        releases.push(resource_key);
                    }
                }))
            })
            .map_err(|err| subduction::wgpu::SurfaceFrameError::Producer(err.to_string()))
    }

    fn drain_unused_resource_releases(&mut self) {
        let Some(layer_host) = &mut self.layer_host else {
            return;
        };
        let Ok(mut releases) = self.unused_resource_releases.lock() else {
            return;
        };
        for resource_key in releases.drain(..) {
            if std::env::var_os("SUBDUCTION_SURFACE_POOL_DIAG").is_some() {
                eprintln!(
                    "floem surface pool drain_release resource_key={}",
                    resource_key,
                );
            }
            layer_host.release_wgpu_surface_resource(resource_key);
        }
    }

    fn sync_scene_layer(&mut self, layer_id: LayerId, layer: &SceneCompositorLayer) {
        self.ensure_layer_content(layer_id);
        self.layer_store.set_bounds(layer_id, layer.bounds.size());
        let origin = layer.transform * layer.bounds.origin();
        self.layer_store.set_transform(
            layer_id,
            Transform3d::from_translation(origin.x, origin.y, 0.0),
        );
        self.layer_store.set_clip(layer_id, None);
        self.layer_store.set_opacity(layer_id, layer.opacity);
    }

    fn sync_external_layer(&mut self, layer_id: LayerId, layer: &ExternalSurfaceCompositorLayer) {
        self.ensure_layer_content(layer_id);
        self.layer_store.set_bounds(layer_id, layer.rect.size());
        let origin = layer.transform * layer.rect.origin();
        self.layer_store.set_transform(
            layer_id,
            Transform3d::from_translation(origin.x, origin.y, 0.0),
        );
        self.layer_store.set_clip(layer_id, None);
        self.layer_store.set_opacity(layer_id, layer.opacity);
    }

    fn destroy_layer_recursive(&mut self, layer_id: LayerId) {
        let children = self.layer_store.children(layer_id).collect::<Vec<_>>();
        for child in children {
            self.destroy_layer_recursive(child);
        }
        self.layer_store.destroy_layer(layer_id);
    }

    pub(crate) fn render_scene_layers(
        &mut self,
        plan: &CompositionPlan,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
        gpu_resources: &GpuResources,
        renderer: &mut dyn WindowRenderer,
        effective_scale: f64,
    ) {
        let render_call_id = COMPOSITOR_RENDER_CALL_ID.fetch_add(1, Ordering::Relaxed);
        if crate::frame_clock::frame_pacing_diag_enabled() {
            let scene_layers = plan
                .items
                .iter()
                .filter(|item| matches!(item, CompositionItem::Scene(_)))
                .count();
            let effect_scene_layers = plan
                .items
                .iter()
                .filter(|item| {
                    matches!(item, CompositionItem::Scene(layer) if !layer.color_effects.is_empty())
                })
                .count();
            eprintln!(
                "floem compositor render_scene_layers begin call={} plan_items={} scene_layers={} effect_scene_layers={}",
                render_call_id,
                plan.items.len(),
                scene_layers,
                effect_scene_layers,
            );
        }
        let rendered_scene_frames = self.render_scene_content(
            render_call_id,
            plan,
            external_surfaces,
            gpu_resources,
            renderer,
            effective_scale,
        );
        if crate::frame_clock::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor render_scene_layers end call={} rendered_frames={}",
                render_call_id,
                rendered_scene_frames.len(),
            );
        }
        let mut publications = rendered_scene_frames
            .iter()
            .filter_map(|rendered| publication_for_frame(&rendered.frame))
            .collect::<Vec<_>>();
        publications.extend(self.submitted_content_publications());
        self.commit_layer_tree_and_publications(&publications, &gpu_resources.queue);
        for rendered in rendered_scene_frames {
            rendered.frame.mark_published();
            self.scene_content_by_key.insert(
                rendered.key.clone(),
                ExternalTextureContent::from_submitted_frame(rendered.frame),
            );
            self.scene_render_signatures
                .insert(rendered.key, rendered.signature);
        }
        self.mark_submitted_content_published();
    }

    fn render_scene_content(
        &mut self,
        render_call_id: u64,
        plan: &CompositionPlan,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
        gpu_resources: &GpuResources,
        renderer: &mut dyn WindowRenderer,
        effective_scale: f64,
    ) -> Vec<RenderedSceneFrame> {
        let mut rendered_frames = Vec::new();
        let mut timing = NoopRendererTimingRecorder;
        for item in &plan.items {
            let CompositionItem::Scene(layer) = item else {
                continue;
            };
            let Some(surface_id) = self.content_surface_for_key(&layer.key) else {
                continue;
            };
            let bounds = layer.bounds;
            let width = (bounds.width() * effective_scale).ceil().max(1.0) as u32;
            let height = (bounds.height() * effective_scale).ceil().max(1.0) as u32;
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let Some(format) = renderer.compositor_texture_format() else {
                let failure = UnsupportedPublication::Scene {
                    key: layer.key.clone(),
                    revision: layer.content_revision,
                };
                if self.unsupported_publications.insert(failure) {
                    eprintln!(
                        "floem compositor: scene layer {:?} renderer has no Subduction wgpu target format",
                        layer.key,
                    );
                }
                continue;
            };
            let signature =
                scene_render_signature(layer, external_surfaces, effective_scale, format, size);
            if self.scene_render_signatures.get(&layer.key) == Some(&signature)
                && self.scene_content_by_key.contains_key(&layer.key)
            {
                continue;
            }
            let Some(external_images) =
                self.external_image_resources_for_scene(layer, external_surfaces)
            else {
                if crate::frame_clock::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem compositor scene render skip key={:?} reason=external_image_unavailable",
                        layer.key,
                    );
                }
                continue;
            };
            let opportunity = subduction::wgpu::SurfaceFrameOpportunity {
                surface_id,
                frame_index: layer.content_revision,
                now: subduction_core::time::HostTime(0),
                target_timestamp: None,
                target_present: None,
                previous_present: None,
                refresh_interval: None,
                confidence: subduction_core::timing::TimingConfidence::PacingOnly,
            };
            let Ok(lease) = self.create_wgpu_surface_frame(
                &gpu_resources.device,
                opportunity,
                size,
                format,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ) else {
                let failure = UnsupportedPublication::Scene {
                    key: layer.key.clone(),
                    revision: layer.content_revision,
                };
                if self.unsupported_publications.insert(failure) {
                    eprintln!(
                        "floem compositor: scene layer {:?} could not acquire a Subduction wgpu target",
                        layer.key,
                    );
                }
                continue;
            };
            let scene_texture = if layer.color_effects.is_empty() {
                None
            } else {
                let texture = create_effect_intermediate_texture(
                    &gpu_resources.device,
                    size,
                    format,
                    "floem compositor effect scene input",
                );
                initialize_texture_for_external_writer(
                    &gpu_resources.device,
                    &gpu_resources.queue,
                    &texture,
                    "floem compositor effect scene input init",
                );
                Some(texture)
            };
            let render_texture = scene_texture.as_ref().unwrap_or(&lease.texture);
            let target_origin = (layer.transform * bounds.origin()).to_vec2() * effective_scale;
            let base_transform = layer
                .transform
                .then_scale(effective_scale)
                .then_translate(-target_origin);
            let render_size = Size::new(f64::from(width), f64::from(height));
            let mut source = SceneLayerSource {
                scene: &layer.scene,
                base_transform,
                clip: layer.clip,
                render_size,
            };
            if crate::frame_clock::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem compositor scene render call={} key={:?} revision={} size={}x{} bounds={:?} transform={:?} commands={} external_images={} color_effects={}",
                    render_call_id,
                    layer.key,
                    layer.content_revision,
                    width,
                    height,
                    layer.bounds,
                    layer.transform,
                    layer.scene.commands().len(),
                    layer.external_images.len(),
                    layer.color_effects.len(),
                );
            }
            if !renderer.render_into_texture_with_external_images(
                render_size,
                &mut source,
                render_texture,
                external_images,
                &mut timing,
            ) {
                let failure = UnsupportedPublication::Scene {
                    key: layer.key.clone(),
                    revision: layer.content_revision,
                };
                if self.unsupported_publications.insert(failure) {
                    eprintln!(
                        "floem compositor: scene layer {:?} renderer cannot render into a Subduction wgpu target",
                        layer.key,
                    );
                }
                if crate::frame_clock::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem compositor scene render skip key={:?} reason=renderer_failed",
                        layer.key,
                    );
                }
                continue;
            }
            if let Some(scene_texture) = &scene_texture
                && let Err(err) = self.effect_renderer.render_effect_chain(
                    &gpu_resources.device,
                    &gpu_resources.queue,
                    render_call_id,
                    &layer.key,
                    scene_texture,
                    &lease.texture,
                    format,
                    size,
                    &layer.color_effects,
                    layer.content_revision,
                    effective_scale,
                )
            {
                let failure = UnsupportedPublication::SceneEffect {
                    key: layer.key.clone(),
                    revision: layer.content_revision,
                };
                if self.unsupported_publications.insert(failure) {
                    eprintln!(
                        "floem compositor: scene layer {:?} failed compositor color effect pass: {err}",
                        layer.key,
                    );
                }
                if crate::frame_clock::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem compositor scene render skip key={:?} reason=color_effect_failed",
                        layer.key,
                    );
                }
                continue;
            }
            #[cfg(debug_assertions)]
            if crate::frame_clock::frame_pacing_diag_enabled() && !layer.color_effects.is_empty() {
                eprintln!(
                    "floem compositor scene effect rendered call={} key={:?} revision={} effects={}",
                    render_call_id,
                    layer.key,
                    layer.content_revision,
                    layer.color_effects.len(),
                );
            }
            let subduction::wgpu::SurfaceFrameCompletion::Submitted(frame) = lease.submit() else {
                if crate::frame_clock::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem compositor scene render skip key={:?} reason=lease_not_submitted",
                        layer.key,
                    );
                }
                continue;
            };
            if crate::frame_clock::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem compositor scene rendered key={:?} surface={:?} size={}x{}",
                    layer.key, surface_id, width, height,
                );
            }
            rendered_frames.push(RenderedSceneFrame {
                key: layer.key.clone(),
                frame,
                signature,
            });
        }
        rendered_frames
    }

    pub(crate) fn capture_scene(
        &self,
        plan: &CompositionPlan,
        frame_size: Size,
        effective_scale: f64,
        background: Option<Brush>,
    ) -> Result<CompositorCaptureScene, String> {
        let mut scene = Scene::new();
        if let Some(background) = background {
            scene.draw(Draw::Fill {
                transform: Affine::IDENTITY,
                fill_rule: Fill::NonZero,
                brush: background,
                brush_transform: None,
                shape: Geometry::Rect(frame_size.to_rect().expand()),
                composite: Composite::default(),
            });
        }

        let mut resources = ExternalImageResources::default();
        let mut next_image_id = 1;
        for item in &plan.items {
            match item {
                CompositionItem::Scene(layer) => {
                    let Some(content) = self.scene_content_by_key.get(&layer.key) else {
                        return Err(format!(
                            "compositor capture missing rendered scene layer {:?}",
                            layer.key
                        ));
                    };
                    append_texture_layer(
                        &mut scene,
                        &mut resources,
                        &mut next_image_id,
                        content,
                        layer.transform,
                        layer.bounds,
                        layer.opacity,
                        effective_scale,
                    );
                }
                CompositionItem::ExternalSurface(layer) => {
                    let Some(CompositorLayerState::ExternalSurface(state)) =
                        self.layers_by_key.get(&layer.key)
                    else {
                        return Err(format!(
                            "compositor capture missing external layer {:?}",
                            layer.key
                        ));
                    };
                    let ExternalSurfaceContent::Texture(texture) = &state.content else {
                        return Err(format!(
                            "compositor capture external surface {:?} has no submitted texture",
                            state.surface_id
                        ));
                    };
                    let Some(content) = ExternalTextureContent::from_external_texture(texture)
                    else {
                        return Err(format!(
                            "compositor capture external surface {:?} submitted non-Subduction texture",
                            state.surface_id
                        ));
                    };
                    append_texture_layer(
                        &mut scene,
                        &mut resources,
                        &mut next_image_id,
                        &content,
                        layer.transform,
                        layer.rect,
                        layer.opacity,
                        effective_scale,
                    );
                }
            }
        }

        Ok(CompositorCaptureScene { scene, resources })
    }

    fn external_image_resources_for_scene(
        &mut self,
        layer: &SceneLayer,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Option<ExternalImageResources> {
        let mut resources = ExternalImageResources::default();
        for external in &layer.external_images {
            let content = external_surfaces
                .get(&external.surface_id)
                .map(|entry| entry.content.clone())
                .unwrap_or(ExternalSurfaceContent::Empty);
            let ExternalSurfaceContent::Texture(texture) = content else {
                let failure = UnsupportedPublication::SceneExternalTexture {
                    key: layer.key.clone(),
                    revision: layer.content_revision,
                    surface_id: external.surface_id,
                };
                if self.unsupported_publications.insert(failure) {
                    eprintln!(
                        "floem compositor: scene layer {:?} cannot flatten external surface {:?} without a submitted texture",
                        layer.key, external.surface_id,
                    );
                }
                return None;
            };
            let Some(frame) = texture
                .payload
                .downcast_ref::<subduction::wgpu::SubmittedSurfaceFrame>()
            else {
                let failure = UnsupportedPublication::SceneExternalTexture {
                    key: layer.key.clone(),
                    revision: layer.content_revision,
                    surface_id: external.surface_id,
                };
                if self.unsupported_publications.insert(failure) {
                    eprintln!(
                        "floem compositor: scene layer {:?} cannot flatten non-Subduction external surface {:?}; refusing silent copy/fallback",
                        layer.key, external.surface_id,
                    );
                }
                return None;
            };
            resources.insert(
                external.image_id,
                ResolvedExternalImage {
                    texture: frame.texture.clone(),
                    view: frame.view.clone(),
                    format: frame.format,
                    width: frame.size.width,
                    height: frame.size.height,
                },
            );
        }
        Some(resources)
    }

    fn commit_layer_tree_and_publications(
        &mut self,
        publications: &[(subduction::SubmittedContentInfo, subduction::ResourceKey)],
        queue: &wgpu::Queue,
    ) {
        let Some(layer_host) = &mut self.layer_host else {
            return;
        };
        let changes = self.pending_layer_changes.take().unwrap_or_default();
        layer_host.apply_and_publish_surface_resources(&self.layer_store, &changes, publications);
        #[cfg(target_os = "macos")]
        if self.metal_capture_active {
            self.metal_capture_active = false;
            let _ = queue;
            subduction_backend_apple::stop_active_metal_capture();
        }
    }

    fn submitted_content_publications(
        &mut self,
    ) -> Vec<(subduction::SubmittedContentInfo, subduction::ResourceKey)> {
        let mut publications = Vec::new();
        for (key, state) in &self.layers_by_key {
            let Some(layer_id) = self.layer_ids_by_key.get(key).copied() else {
                continue;
            };
            let Some(surface_id) = self.layer_store.content(layer_id) else {
                continue;
            };
            match state {
                CompositorLayerState::Scene(_) => {}
                CompositorLayerState::ExternalSurface(layer) => match &layer.content {
                    ExternalSurfaceContent::Texture(texture) => {
                        if let Some(frame) = texture
                            .payload
                            .downcast_ref::<subduction::wgpu::SubmittedSurfaceFrame>()
                        {
                            publications.extend(publication_for_frame(frame));
                        } else {
                            let failure = UnsupportedPublication::ExternalTexture {
                                key: key.clone(),
                                version: layer.content_version,
                            };
                            if self.unsupported_publications.insert(failure) {
                                eprintln!(
                                    "floem compositor: external surface {:?} submitted a non-Subduction texture {:?} for surface {:?}; refusing silent copy/fallback",
                                    layer.surface_id, texture.size, surface_id,
                                );
                            }
                        }
                    }
                    ExternalSurfaceContent::Image(image) => {
                        let failure = UnsupportedPublication::ExternalTexture {
                            key: key.clone(),
                            version: layer.content_version,
                        };
                        if self.unsupported_publications.insert(failure) {
                            eprintln!(
                                "floem compositor: external surface {:?} submitted CPU image {}x{} for surface {:?}; refusing silent copy/fallback",
                                layer.surface_id, image.width, image.height, surface_id,
                            );
                        }
                    }
                    ExternalSurfaceContent::Empty | ExternalSurfaceContent::Subduction(_) => {}
                },
            }
        }
        publications
    }

    fn mark_submitted_content_published(&self) {
        for state in self.layers_by_key.values() {
            let CompositorLayerState::ExternalSurface(layer) = state else {
                continue;
            };
            let ExternalSurfaceContent::Texture(texture) = &layer.content else {
                continue;
            };
            if let Some(frame) = texture
                .payload
                .downcast_ref::<subduction::wgpu::SubmittedSurfaceFrame>()
            {
                frame.mark_published();
            }
        }
    }
}

struct RenderedSceneFrame {
    key: CompositionKey,
    frame: subduction::wgpu::SubmittedSurfaceFrame,
    signature: SceneRenderSignature,
}

fn merge_frame_changes(target: &mut FrameChanges, source: FrameChanges) {
    target.transforms.extend(source.transforms);
    target.opacities.extend(source.opacities);
    target.clips.extend(source.clips);
    target.content.extend(source.content);
    target.bounds.extend(source.bounds);
    target.hidden.extend(source.hidden);
    target.unhidden.extend(source.unhidden);
    target.added.extend(source.added);
    target.removed.extend(source.removed);
    target.topology_changed |= source.topology_changed;
}

fn publication_for_frame(
    frame: &subduction::wgpu::SubmittedSurfaceFrame,
) -> Option<(subduction::SubmittedContentInfo, subduction::ResourceKey)> {
    let resource_key = frame.resource_key?;
    Some((
        subduction::SubmittedContentInfo {
            surface_id: frame.opportunity.surface_id,
            revision: subduction_render::SurfaceContentRevision(frame.opportunity.frame_index),
            width: frame.size.width,
            height: frame.size.height,
        },
        subduction::ResourceKey(resource_key),
    ))
}

pub(crate) struct CompositorCaptureScene {
    pub(crate) scene: Scene,
    pub(crate) resources: ExternalImageResources,
}

#[derive(Clone, Debug)]
struct ExternalTextureContent {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: wgpu::Extent3d,
}

impl ExternalTextureContent {
    fn from_submitted_frame(frame: subduction::wgpu::SubmittedSurfaceFrame) -> Self {
        Self {
            texture: frame.texture.clone(),
            view: frame.view.clone(),
            format: frame.format,
            size: frame.size,
        }
    }

    fn from_external_texture(texture: &ExternalTexture) -> Option<Self> {
        let frame = texture
            .payload
            .downcast_ref::<subduction::wgpu::SubmittedSurfaceFrame>()?;
        Some(Self {
            texture: frame.texture.clone(),
            view: frame.view.clone(),
            format: frame.format,
            size: frame.size,
        })
    }
}

fn append_texture_layer(
    scene: &mut Scene,
    resources: &mut ExternalImageResources,
    next_image_id: &mut u64,
    content: &ExternalTextureContent,
    transform: Affine,
    logical_bounds: Rect,
    opacity: f32,
    effective_scale: f64,
) {
    let width = content.size.width.max(1);
    let height = content.size.height.max(1);
    let image_id = ExternalImageId(*next_image_id);
    *next_image_id += 1;
    resources.insert(
        image_id,
        ResolvedExternalImage {
            texture: content.texture.clone(),
            view: content.view.clone(),
            format: content.format,
            width,
            height,
        },
    );

    let origin = (transform * logical_bounds.origin()).to_vec2() * effective_scale;
    let target_width = logical_bounds.width() * effective_scale;
    let target_height = logical_bounds.height() * effective_scale;
    let image = ExternalImage::new(image_id, width, height, ImageAlphaType::AlphaPremultiplied);
    let brush = Brush::Image(ImageBrush::from(image).with_alpha(opacity));
    scene.draw(Draw::Fill {
        transform: Affine::new([
            target_width / f64::from(width),
            0.0,
            0.0,
            target_height / f64::from(height),
            origin.x,
            origin.y,
        ]),
        fill_rule: Fill::NonZero,
        brush,
        brush_transform: None,
        shape: Geometry::Rect(Rect::new(0.0, 0.0, f64::from(width), f64::from(height))),
        composite: Composite::new(BlendMode::default(), 1.0),
    });
}

#[derive(Clone, Debug, PartialEq)]
struct SceneRenderSignature {
    content_revision: u64,
    command_count: usize,
    bounds: Rect,
    content_bounds: Option<Rect>,
    transform: Affine,
    clip: Option<peniko::kurbo::RoundedRect>,
    opacity: f32,
    effective_scale_bits: u64,
    format: wgpu::TextureFormat,
    target_size: wgpu::Extent3d,
    external_versions: Vec<(ExternalSurfaceId, u64)>,
    color_effect_hashes: Vec<u64>,
}

fn scene_render_signature(
    layer: &SceneLayer,
    external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    effective_scale: f64,
    format: wgpu::TextureFormat,
    target_size: wgpu::Extent3d,
) -> SceneRenderSignature {
    SceneRenderSignature {
        content_revision: layer.content_revision,
        command_count: layer.scene.commands().len(),
        bounds: layer.bounds,
        content_bounds: layer.content_bounds,
        transform: layer.transform,
        clip: layer.clip,
        opacity: layer.opacity,
        effective_scale_bits: effective_scale.to_bits(),
        format,
        target_size,
        external_versions: layer
            .external_images
            .iter()
            .map(|image| {
                (
                    image.surface_id,
                    external_surfaces
                        .get(&image.surface_id)
                        .map(|entry| entry.version)
                        .unwrap_or(0),
                )
            })
            .collect(),
        color_effect_hashes: layer
            .color_effects
            .iter()
            .map(color_effect_shader_hash)
            .collect(),
    }
}

#[derive(Default)]
struct ColorEffectRenderer {
    pipelines: FxHashMap<ColorEffectPipelineKey, ColorEffectPipeline>,
}

impl ColorEffectRenderer {
    fn render_effect_chain(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        render_call_id: u64,
        key: &CompositionKey,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
        format: wgpu::TextureFormat,
        size: wgpu::Extent3d,
        effects: &[ColorEffect],
        frame_index: u64,
        effective_scale: f64,
    ) -> Result<(), String> {
        let Some((last, leading)) = effects.split_last() else {
            return Ok(());
        };
        if crate::frame_clock::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor color effect chain call={} key={:?} revision={} effects={} size={}x{}",
                render_call_id,
                key,
                frame_index,
                effects.len(),
                size.width,
                size.height,
            );
        }

        let mut input_texture = input.clone();
        let mut ping_pong = Vec::new();
        if !leading.is_empty() {
            ping_pong.push(create_effect_intermediate_texture(
                device,
                size,
                format,
                "floem compositor color effect ping",
            ));
            if leading.len() > 1 {
                ping_pong.push(create_effect_intermediate_texture(
                    device,
                    size,
                    format,
                    "floem compositor color effect pong",
                ));
            }
        }

        for (index, effect) in leading.iter().enumerate() {
            let output_texture = &ping_pong[index % ping_pong.len()];
            self.render_single_effect(
                device,
                queue,
                render_call_id,
                &input_texture,
                output_texture,
                format,
                size,
                effect,
                frame_index,
                effective_scale,
            )?;
            input_texture = output_texture.clone();
        }

        self.render_single_effect(
            device,
            queue,
            render_call_id,
            &input_texture,
            output,
            format,
            size,
            last,
            frame_index,
            effective_scale,
        )
    }

    fn render_single_effect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        render_call_id: u64,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
        format: wgpu::TextureFormat,
        size: wgpu::Extent3d,
        effect: &ColorEffect,
        frame_index: u64,
        effective_scale: f64,
    ) -> Result<(), String> {
        let pipeline = self.pipeline(device, format, effect)?;
        let input_view = input.create_view(&wgpu::TextureViewDescriptor {
            label: Some("floem compositor color effect input view"),
            ..Default::default()
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor {
            label: Some("floem compositor color effect output view"),
            ..Default::default()
        });
        let args = padded_uniform_bytes(&effect.args.bytes);
        let args_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("floem compositor color effect args"),
            size: args.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&args_buffer, 0, &args);
        let frame_bytes = color_effect_frame_bytes(frame_index, effective_scale, size);
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("floem compositor color effect frame"),
            size: frame_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&frame_buffer, 0, &frame_bytes);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("floem compositor color effect bind group"),
            layout: &pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: args_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: frame_buffer.as_entire_binding(),
                },
            ],
        });
        let encoder_label = format!(
            "floem compositor color effect encoder call={render_call_id} revision={frame_index}"
        );
        let pass_label = format!(
            "floem compositor color effect pass call={render_call_id} revision={frame_index}"
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(&encoder_label),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&pass_label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_viewport(0.0, 0.0, size.width as f32, size.height as f32, 0.0, 1.0);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        effect: &ColorEffect,
    ) -> Result<&ColorEffectPipeline, String> {
        let shader_hash = color_effect_shader_hash(effect);
        let key = ColorEffectPipelineKey {
            id: effect.id,
            shader_hash,
            format,
        };
        if !self.pipelines.contains_key(&key) {
            let pipeline = ColorEffectPipeline::new(device, format, effect)?;
            self.pipelines.insert(key, pipeline);
        }
        self.pipelines
            .get(&key)
            .ok_or_else(|| "failed to cache color effect pipeline".to_owned())
    }
}

struct ColorEffectPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl ColorEffectPipeline {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        effect: &ColorEffect,
    ) -> Result<Self, String> {
        let shader_source = color_effect_shader_source(effect)?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(color_effect_label(effect)),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("floem compositor color effect bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floem compositor color effect pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(color_effect_label(effect)),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("floem compositor color effect sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Ok(Self {
            pipeline,
            bind_group_layout,
            sampler,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ColorEffectPipelineKey {
    id: crate::effects::ColorEffectId,
    shader_hash: u64,
    format: wgpu::TextureFormat,
}

fn create_effect_intermediate_texture(
    device: &wgpu::Device,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    label: &'static str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn initialize_texture_for_external_writer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    label: &'static str,
) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some(label),
        ..Default::default()
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit([encoder.finish()]);
}

fn color_effect_shader_hash(effect: &ColorEffect) -> u64 {
    let mut hasher = DefaultHasher::new();
    effect.id.hash(&mut hasher);
    match &effect.shader {
        crate::effects::ColorEffectShader::Wgsl {
            label,
            fragment_body,
        } => {
            label.hash(&mut hasher);
            fragment_body.hash(&mut hasher);
        }
    }
    effect.args.bytes.hash(&mut hasher);
    hasher.finish()
}

fn color_effect_label(effect: &ColorEffect) -> &str {
    match &effect.shader {
        crate::effects::ColorEffectShader::Wgsl { label, .. } => {
            label.as_deref().unwrap_or("floem compositor color effect")
        }
    }
}

fn color_effect_shader_source(effect: &ColorEffect) -> Result<String, String> {
    let crate::effects::ColorEffectShader::Wgsl { fragment_body, .. } = &effect.shader;
    Ok(format!(
        r#"
struct ColorEffectArgs {{
    data: vec4<u32>,
}};

struct ColorEffectFrame {{
    time_seconds: f32,
    delta_seconds: f32,
    frame_index: u32,
    _pad0: u32,
    effective_scale: f32,
    target_width: f32,
    target_height: f32,
    _pad1: f32,
}};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> args: ColorEffectArgs;
@group(0) @binding(3) var<uniform> frame: ColorEffectFrame;

struct VsOut {{
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {{
    var out: VsOut;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}}

fn color_effect(
    position: vec2<f32>,
    uv: vec2<f32>,
    color: vec4<f32>,
    args: ColorEffectArgs,
    frame: ColorEffectFrame,
) -> vec4<f32> {{
{}
}}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {{
    let color = textureSample(input_texture, input_sampler, in.uv);
    let logical_position = in.position.xy / vec2<f32>(frame.effective_scale);
    return color_effect(logical_position, in.uv, color, args, frame);
}}
"#,
        fragment_body
    ))
}

fn padded_uniform_bytes(bytes: &[u8]) -> Vec<u8> {
    let len = bytes.len().max(16).next_multiple_of(16);
    let mut padded = vec![0; len];
    padded[..bytes.len()].copy_from_slice(bytes);
    padded
}

fn color_effect_frame_bytes(
    frame_index: u64,
    effective_scale: f64,
    target_size: wgpu::Extent3d,
) -> [u8; 32] {
    let time_seconds = frame_index as f32 / 60.0;
    let delta_seconds = 1.0 / 60.0f32;
    let frame_index = frame_index.min(u64::from(u32::MAX)) as u32;
    let effective_scale = effective_scale as f32;
    let target_width = target_size.width as f32 / effective_scale;
    let target_height = target_size.height as f32 / effective_scale;
    let mut bytes = [0; 32];
    bytes[0..4].copy_from_slice(&time_seconds.to_ne_bytes());
    bytes[4..8].copy_from_slice(&delta_seconds.to_ne_bytes());
    bytes[8..12].copy_from_slice(&frame_index.to_ne_bytes());
    bytes[16..20].copy_from_slice(&effective_scale.to_ne_bytes());
    bytes[20..24].copy_from_slice(&target_width.to_ne_bytes());
    bytes[24..28].copy_from_slice(&target_height.to_ne_bytes());
    bytes
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum UnsupportedPublication {
    Scene {
        key: CompositionKey,
        revision: u64,
    },
    SceneEffect {
        key: CompositionKey,
        revision: u64,
    },
    SceneExternalTexture {
        key: CompositionKey,
        revision: u64,
        surface_id: ExternalSurfaceId,
    },
    ExternalTexture {
        key: CompositionKey,
        version: u64,
    },
}

struct SceneLayerSource<'a> {
    scene: &'a Scene,
    base_transform: Affine,
    clip: Option<peniko::kurbo::RoundedRect>,
    render_size: Size,
}

impl RenderSource for SceneLayerSource<'_> {
    fn paint_into(&mut self, sink: &mut dyn PaintSink) {
        if let Some(clip) = self.clip {
            display_list::replay_view_clip(sink, clip, self.base_transform, self.render_size);
        }
        display_list::replay_scene(self.scene, sink, self.base_transform, self.render_size);
        if self.clip.is_some() {
            sink.pop_clip();
        }
    }
}

struct NoopRendererTimingRecorder;

impl RendererTimingRecorder for NoopRendererTimingRecorder {
    fn record_span(
        &mut self,
        _label: &'static str,
        _span: Option<TimingSpan>,
        _kind: crate::inspector::TimingKind,
    ) {
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CompositorLayerState {
    Scene(SceneCompositorLayer),
    ExternalSurface(ExternalSurfaceCompositorLayer),
}

impl CompositorLayerState {
    fn key(&self) -> &CompositionKey {
        match self {
            Self::Scene(layer) => &layer.key,
            Self::ExternalSurface(layer) => &layer.key,
        }
    }

    fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Scene(a), Self::Scene(b)) => a == b,
            (Self::ExternalSurface(a), Self::ExternalSurface(b)) => a.equivalent(b),
            _ => false,
        }
    }

    fn from_item(
        item: &CompositionItem,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Self {
        match item {
            CompositionItem::Scene(layer) => {
                Self::Scene(SceneCompositorLayer::from_layer(layer, external_surfaces))
            }
            CompositionItem::ExternalSurface(layer) => Self::ExternalSurface(
                ExternalSurfaceCompositorLayer::from_layer(layer, external_surfaces),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneCompositorLayer {
    pub key: CompositionKey,
    pub external_images: Vec<SceneExternalImageCompositorLayer>,
    pub color_effects: Vec<ColorEffect>,
    pub transform: peniko::kurbo::Affine,
    pub clip: Option<peniko::kurbo::RoundedRect>,
    pub bounds: peniko::kurbo::Rect,
    pub content_bounds: Option<peniko::kurbo::Rect>,
    pub opacity: f32,
    pub content_revision: u64,
    pub command_count: usize,
    pub promoted: bool,
}

impl SceneCompositorLayer {
    fn from_layer(
        layer: &SceneLayer,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Self {
        Self {
            key: layer.key.clone(),
            external_images: layer
                .external_images
                .iter()
                .map(|image| {
                    SceneExternalImageCompositorLayer::from_image(image, external_surfaces)
                })
                .collect(),
            color_effects: layer.color_effects.clone(),
            transform: layer.transform,
            clip: layer.clip,
            bounds: layer.bounds,
            content_bounds: layer.content_bounds,
            opacity: layer.opacity,
            content_revision: layer.content_revision,
            command_count: layer.scene.commands().len(),
            promoted: layer.promoted,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SceneExternalImageCompositorLayer {
    pub image_id: imaging::ExternalImageId,
    pub surface_id: ExternalSurfaceId,
    pub content: ExternalSurfaceContent,
    pub content_version: u64,
}

impl PartialEq for SceneExternalImageCompositorLayer {
    fn eq(&self, other: &Self) -> bool {
        self.image_id == other.image_id
            && self.surface_id == other.surface_id
            && self.content_version == other.content_version
            && external_content_key(&self.content) == external_content_key(&other.content)
    }
}

impl SceneExternalImageCompositorLayer {
    fn from_image(
        image: &SceneExternalImage,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Self {
        Self {
            image_id: image.image_id,
            surface_id: image.surface_id,
            content: external_surfaces
                .get(&image.surface_id)
                .map(|entry| entry.content.clone())
                .unwrap_or(ExternalSurfaceContent::Empty),
            content_version: external_surfaces
                .get(&image.surface_id)
                .map(|entry| entry.version)
                .unwrap_or(0),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalSurfaceCompositorLayer {
    pub key: CompositionKey,
    pub surface_id: ExternalSurfaceId,
    pub rect: peniko::kurbo::Rect,
    pub source_size: peniko::kurbo::Size,
    pub transform: peniko::kurbo::Affine,
    pub clip: Option<peniko::kurbo::RoundedRect>,
    pub opacity: f32,
    pub content: ExternalSurfaceContent,
    pub content_version: u64,
}

impl ExternalSurfaceCompositorLayer {
    fn from_layer(
        layer: &ExternalSurfaceLayer,
        external_surfaces: &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    ) -> Self {
        Self {
            key: layer.key.clone(),
            surface_id: layer.surface_id,
            rect: layer.rect,
            source_size: layer.source_size,
            transform: layer.transform,
            clip: layer.clip,
            opacity: layer.opacity,
            content: external_surfaces
                .get(&layer.surface_id)
                .map(|entry| entry.content.clone())
                .unwrap_or(ExternalSurfaceContent::Empty),
            content_version: external_surfaces
                .get(&layer.surface_id)
                .map(|entry| entry.version)
                .unwrap_or(0),
        }
    }

    fn equivalent(&self, other: &Self) -> bool {
        self.key == other.key
            && self.surface_id == other.surface_id
            && self.rect == other.rect
            && self.source_size == other.source_size
            && self.transform == other.transform
            && self.clip == other.clip
            && self.opacity == other.opacity
            && self.content_version == other.content_version
            && external_content_key(&self.content) == external_content_key(&other.content)
    }
}

fn external_content_key(content: &ExternalSurfaceContent) -> ExternalContentKey {
    match content {
        ExternalSurfaceContent::Empty => ExternalContentKey::Empty,
        ExternalSurfaceContent::Texture(texture) => {
            ExternalContentKey::Texture { size: texture.size }
        }
        ExternalSurfaceContent::Image(image) => ExternalContentKey::Image {
            size: peniko::kurbo::Size::new(image.width as f64, image.height as f64),
        },
        ExternalSurfaceContent::Subduction(surface) => ExternalContentKey::Subduction {
            ptr: std::sync::Arc::as_ptr(surface).cast::<()>() as usize,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ExternalContentKey {
    Empty,
    Texture { size: peniko::kurbo::Size },
    Image { size: peniko::kurbo::Size },
    Subduction { ptr: usize },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompositorDiff {
    pub created: Vec<CompositionKey>,
    pub updated: Vec<CompositionKey>,
    pub removed: Vec<CompositionKey>,
    pub order_changed: bool,
}
