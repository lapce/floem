use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

use crate::{
    external_surface::{
        ExternalSurfaceContent, ExternalSurfaceId, ExternalSurfaceOutcome,
        ExternalSurfaceProviderHandle,
    },
    frame::FrameTime,
    gpu_resources::GpuResources,
    paint::composition::{CompositionItem, CompositionPlan, WindowPrefixFingerprint},
};
use peniko::kurbo::{Rect, Size};
use subduction_core::layer::SurfaceId;

use super::compositor::WindowCompositor;

#[derive(Default)]
pub(crate) struct WindowExternalSurfaces {
    entries: FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
    intermediate_pool: IntermediateTexturePool,
    frame_time: Option<FrameTime>,
    needs_frame_pull: bool,
    pending_outcomes: Vec<ExternalSurfaceOutcome>,
}

impl WindowExternalSurfaces {
    pub(crate) fn entries(&self) -> &FxHashMap<ExternalSurfaceId, ExternalSurfaceEntry> {
        &self.entries
    }

    pub(crate) fn set_content(
        &mut self,
        surface_id: ExternalSurfaceId,
        content: ExternalSurfaceContent,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.content = content;
        entry.content_dirty = true;
        entry.version = entry.version.wrapping_add(1);
    }

    pub(crate) fn set_provider(
        &mut self,
        surface_id: ExternalSurfaceId,
        provider: ExternalSurfaceProviderHandle,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.provider = Some(provider);
        entry.content_dirty = true;
        entry.version = entry.version.wrapping_add(1);
    }

    pub(crate) fn request_frame(&mut self, surface_id: ExternalSurfaceId) {
        self.entries.entry(surface_id).or_default();
        self.needs_frame_pull = true;
    }

    pub(crate) fn update_providers(
        &mut self,
        plan: &CompositionPlan,
        compositor: Option<&mut WindowCompositor>,
        effective_scale: f64,
        gpu_resources: Option<&GpuResources>,
    ) -> WindowExternalSurfaceFrameUpdate {
        let Some(frame_time) = self.frame_time else {
            return WindowExternalSurfaceFrameUpdate::default();
        };
        let mut compositor = compositor;
        let mut frame_update = WindowExternalSurfaceFrameUpdate::default();
        self.pending_outcomes.clear();
        for planned_surface in planned_external_surfaces(plan) {
            let Some(entry) = self.entries.get_mut(&planned_surface.surface_id) else {
                continue;
            };
            let Some(provider) = &entry.provider else {
                continue;
            };
            let Ok(mut provider) = provider.lock() else {
                continue;
            };
            let poll_update = provider.poll_current_content();
            frame_update.request_next_frame |= poll_update.request_next_frame;
            if poll_update.content_changed {
                frame_update.mark_content_changed(planned_surface.surface_id);
                entry.content = provider
                    .current_content()
                    .unwrap_or(ExternalSurfaceContent::Empty);
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            let size_px = planned_surface.source_size * effective_scale;
            let target = match (provider.can_accept_frame_target(), gpu_resources) {
                (false, _) => None,
                (true, Some(gpu_resources)) => {
                    let surface_id = if let Some(key) = planned_surface.key.as_ref() {
                        compositor
                            .as_deref()
                            .and_then(|compositor| compositor.content_surface_for_key(key))
                    } else {
                        Some(SurfaceId(planned_surface.surface_id.get() as u32))
                    };
                    let Some(surface_id) = surface_id else {
                        continue;
                    };
                    let size = wgpu::Extent3d {
                        width: size_px.width.ceil().max(1.0) as u32,
                        height: size_px.height.ceil().max(1.0) as u32,
                        depth_or_array_layers: 1,
                    };
                    let opportunity = subduction::wgpu::SurfaceFrameOpportunity {
                        surface_id,
                        frame_index: frame_time.frame_index,
                        now: subduction_core::time::HostTime(0),
                        target_timestamp: None,
                        target_present: None,
                        previous_present: None,
                        refresh_interval: None,
                        confidence: subduction_core::timing::TimingConfidence::PacingOnly,
                    };
                    let format = subduction::wgpu::ExternalSurfaceConfig::default().format;
                    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING;
                    if planned_surface.key.is_some() {
                        let Some(compositor) = compositor.as_deref_mut() else {
                            continue;
                        };
                        compositor
                            .create_wgpu_surface_frame(
                                &gpu_resources.device,
                                opportunity,
                                size,
                                format,
                                usage,
                            )
                            .ok()
                    } else {
                        Some(create_intermediate_wgpu_surface_frame(
                            &mut self.intermediate_pool,
                            &gpu_resources.device,
                            opportunity,
                            size,
                            format,
                            usage,
                        ))
                    }
                }
                _ => None,
            };
            let args = crate::external_surface::ExternalSurfaceFrameArgs {
                surface_id: planned_surface.surface_id,
                frame_index: frame_time.frame_index,
                interval: frame_time.interval,
                visible: true,
                rect: planned_surface.rect,
                size_px,
                gpu_resources: gpu_resources.cloned(),
                target,
                previous_outcome: entry.previous_outcome,
            };
            let update = provider.update_current_content(args);
            frame_update.request_next_frame |= update.request_next_frame;
            if update.content_changed {
                frame_update.mark_content_changed(planned_surface.surface_id);
                entry.content = provider
                    .current_content()
                    .unwrap_or(ExternalSurfaceContent::Empty);
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            let post_update = provider.poll_current_content();
            frame_update.request_next_frame |= post_update.request_next_frame;
            if post_update.content_changed {
                frame_update.mark_content_changed(planned_surface.surface_id);
                entry.content = provider
                    .current_content()
                    .unwrap_or(ExternalSurfaceContent::Empty);
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            self.pending_outcomes.push(ExternalSurfaceOutcome {
                surface_id: planned_surface.surface_id,
                frame_index: frame_time.frame_index,
                visible: true,
                outcome: crate::frame::FrameOutcome {
                    draw_attempted: true,
                    draw_completed: false,
                    missed_deadline: None,
                },
            });
        }
        if frame_update.request_next_frame {
            self.needs_frame_pull = true;
        }
        frame_update
    }

    pub(crate) fn has_frame_pull(&self) -> bool {
        self.needs_frame_pull
    }

    pub(crate) fn take_frame_pull(&mut self) -> bool {
        std::mem::take(&mut self.needs_frame_pull)
    }

    pub(crate) fn pull_frame(
        &mut self,
        frame_time: FrameTime,
        plan: &CompositionPlan,
        compositor: &mut WindowCompositor,
        effective_scale: f64,
        gpu_resources: Option<&GpuResources>,
    ) -> WindowExternalSurfaceFrameUpdate {
        self.take_frame_pull();
        self.frame_time = Some(frame_time);
        let _composition_diff = compositor.apply_plan(plan, &self.entries, gpu_resources);
        let update = self.update_providers(plan, Some(compositor), effective_scale, gpu_resources);
        if update.content_changed {
            let _composition_diff = compositor.apply_plan(plan, &self.entries, gpu_resources);
        }
        update
    }

    pub(crate) fn begin_composition_update(
        &mut self,
        frame_time: FrameTime,
        plan: &CompositionPlan,
    ) -> WindowPrefixFingerprint {
        let old_prefix = plan.window_prefix_fingerprint();
        self.frame_time = Some(frame_time);
        old_prefix
    }

    pub(crate) fn release_outcomes(&mut self, mut update: impl FnMut(&mut ExternalSurfaceOutcome)) {
        for mut outcome in std::mem::take(&mut self.pending_outcomes) {
            update(&mut outcome);
            if let Some(entry) = self.entries.get_mut(&outcome.surface_id) {
                entry.previous_outcome = Some(outcome);
                if let Some(provider) = &entry.provider
                    && let Ok(mut provider) = provider.lock()
                {
                    provider.release_current_content(outcome);
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct WindowExternalSurfaceFrameUpdate {
    pub content_changed: bool,
    pub request_next_frame: bool,
    pub changed_surfaces: Vec<ExternalSurfaceId>,
}

impl WindowExternalSurfaceFrameUpdate {
    fn mark_content_changed(&mut self, surface_id: ExternalSurfaceId) {
        self.content_changed = true;
        if !self.changed_surfaces.contains(&surface_id) {
            self.changed_surfaces.push(surface_id);
        }
    }
}

#[derive(Clone)]
struct PlannedExternalSurface {
    surface_id: ExternalSurfaceId,
    rect: Rect,
    source_size: Size,
    key: Option<crate::paint::composition::CompositionKey>,
}

fn planned_external_surfaces(plan: &CompositionPlan) -> Vec<PlannedExternalSurface> {
    let mut surfaces = Vec::new();
    let mut requested = FxHashMap::default();
    for item in &plan.items {
        match item {
            CompositionItem::ExternalSurface(layer) => {
                let request_key = (layer.surface_id, size_key(layer.source_size));
                if let Some(index) = requested.get(&request_key).copied() {
                    let planned: &mut PlannedExternalSurface = &mut surfaces[index];
                    if planned.key.is_none() {
                        planned.key = Some(layer.key.clone());
                        planned.rect = layer.rect;
                    }
                } else {
                    requested.insert(request_key, surfaces.len());
                    surfaces.push(PlannedExternalSurface {
                        surface_id: layer.surface_id,
                        rect: layer.rect,
                        source_size: layer.source_size,
                        key: Some(layer.key.clone()),
                    });
                }
            }
            CompositionItem::Scene(layer) => {
                for image in &layer.external_images {
                    let request_key = (image.surface_id, size_key(image.source_size));
                    if requested.contains_key(&request_key) {
                        continue;
                    }
                    requested.insert(request_key, surfaces.len());
                    surfaces.push(PlannedExternalSurface {
                        surface_id: image.surface_id,
                        rect: image.rect,
                        source_size: image.source_size,
                        key: None,
                    });
                }
            }
        }
    }
    surfaces
}

fn size_key(size: Size) -> (u64, u64) {
    (size.width.to_bits(), size.height.to_bits())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        external_surface::ExternalSurfaceId,
        paint::composition::{
            CompositionKey, ExternalSurfaceLayer, SceneExternalImage, SceneLayer,
        },
    };

    #[test]
    fn planned_external_surfaces_dedupes_repeated_direct_placements() {
        let surface_id = ExternalSurfaceId::test_new(42);
        let mut plan = CompositionPlan::new();
        plan.items
            .push(CompositionItem::ExternalSurface(ExternalSurfaceLayer {
                key: CompositionKey::ExternalSurface {
                    surface_id,
                    occurrence: 0,
                },
                surface_id,
                rect: Rect::new(0.0, 0.0, 100.0, 50.0),
                source_size: Size::new(100.0, 50.0),
                transform: peniko::kurbo::Affine::IDENTITY,
                clip: None,
                opacity: 1.0,
            }));
        plan.items
            .push(CompositionItem::ExternalSurface(ExternalSurfaceLayer {
                key: CompositionKey::ExternalSurface {
                    surface_id,
                    occurrence: 1,
                },
                surface_id,
                rect: Rect::new(200.0, 0.0, 300.0, 50.0),
                source_size: Size::new(100.0, 50.0),
                transform: peniko::kurbo::Affine::IDENTITY,
                clip: None,
                opacity: 1.0,
            }));

        let planned = planned_external_surfaces(&plan);
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].surface_id, surface_id);
        assert_eq!(planned[0].source_size, Size::new(100.0, 50.0));
        assert_eq!(
            planned[0].key,
            Some(CompositionKey::ExternalSurface {
                surface_id,
                occurrence: 0,
            })
        );
    }

    #[test]
    fn planned_external_surfaces_upgrades_scene_request_to_direct_layer_request() {
        let surface_id = ExternalSurfaceId::test_new(43);
        let mut plan = CompositionPlan::new();
        plan.items.push(CompositionItem::Scene(SceneLayer {
            key: CompositionKey::SceneRun { run_index: 0 },
            scene: imaging::record::Scene::new(),
            external_images: vec![SceneExternalImage {
                image_id: imaging::ExternalImageId(43),
                surface_id,
                rect: Rect::new(0.0, 0.0, 100.0, 50.0),
                source_size: Size::new(100.0, 50.0),
            }],
            color_effects: Vec::new(),
            content_revision: 0,
            transform: peniko::kurbo::Affine::IDENTITY,
            clip: None,
            bounds: Rect::new(0.0, 0.0, 100.0, 50.0),
            content_bounds: None,
            opacity: 1.0,
            promoted: false,
        }));
        plan.items
            .push(CompositionItem::ExternalSurface(ExternalSurfaceLayer {
                key: CompositionKey::ExternalSurface {
                    surface_id,
                    occurrence: 0,
                },
                surface_id,
                rect: Rect::new(200.0, 0.0, 300.0, 50.0),
                source_size: Size::new(100.0, 50.0),
                transform: peniko::kurbo::Affine::IDENTITY,
                clip: None,
                opacity: 1.0,
            }));

        let planned = planned_external_surfaces(&plan);
        assert_eq!(planned.len(), 1);
        assert_eq!(
            planned[0].key,
            Some(CompositionKey::ExternalSurface {
                surface_id,
                occurrence: 0,
            })
        );
        assert_eq!(planned[0].rect, Rect::new(200.0, 0.0, 300.0, 50.0));
    }
}

fn create_intermediate_wgpu_surface_frame(
    pool: &mut IntermediateTexturePool,
    device: &wgpu::Device,
    opportunity: subduction::wgpu::SurfaceFrameOpportunity,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> subduction::wgpu::SurfaceFrameLease {
    pool.drain_releases();
    let key = pool.acquire(device, size, format, usage);
    let resource = pool
        .resources
        .get(&key)
        .expect("acquired intermediate texture resource exists");
    let release_queue = pool.releases.clone();
    subduction::wgpu::SurfaceFrameLease::with_resource_key(
        opportunity,
        size,
        format,
        resource.texture.clone(),
        resource.view.clone(),
        key,
    )
    .with_release(Arc::new(move |resource_key| {
        if let Ok(mut releases) = release_queue.lock() {
            releases.push(resource_key);
        }
    }))
}

#[derive(Default)]
struct IntermediateTexturePool {
    resources: FxHashMap<u64, IntermediateTextureResource>,
    next_key: u64,
    clock: u64,
    releases: Arc<Mutex<Vec<u64>>>,
}

struct IntermediateTextureResource {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    checked_out: bool,
    last_used: u64,
}

impl IntermediateTexturePool {
    fn acquire(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> u64 {
        let width = size.width.max(1);
        let height = size.height.max(1);
        self.clock = self.clock.wrapping_add(1).max(1);
        if let Some((&key, resource)) = self.resources.iter_mut().find(|(_, resource)| {
            !resource.checked_out
                && resource.width == width
                && resource.height == height
                && resource.format == format
                && resource.usage == usage
        }) {
            resource.checked_out = true;
            resource.last_used = self.clock;
            return key;
        }

        self.prune_idle_for_key(width, height, format, usage);
        self.prune_idle_total();
        let key = self.next_key;
        self.next_key = self.next_key.wrapping_add(1).max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("floem flattened external image intermediate texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("floem flattened external image intermediate texture view"),
            ..Default::default()
        });
        self.resources.insert(
            key,
            IntermediateTextureResource {
                texture,
                view,
                width,
                height,
                format,
                usage,
                checked_out: true,
                last_used: self.clock,
            },
        );
        key
    }

    fn drain_releases(&mut self) {
        let Ok(mut releases) = self.releases.lock() else {
            return;
        };
        for key in releases.drain(..) {
            self.clock = self.clock.wrapping_add(1).max(1);
            if let Some(resource) = self.resources.get_mut(&key) {
                resource.checked_out = false;
                resource.last_used = self.clock;
            }
        }
        drop(releases);
        self.prune_idle_total();
    }

    fn prune_idle_for_key(
        &mut self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) {
        const MAX_IDLE_PER_KEY: usize = 3;
        let mut idle = self
            .resources
            .iter()
            .filter_map(|(&key, resource)| {
                (!resource.checked_out
                    && resource.width == width
                    && resource.height == height
                    && resource.format == format
                    && resource.usage == usage)
                    .then_some(key)
            })
            .collect::<Vec<_>>();
        let remove_count = idle.len().saturating_sub(MAX_IDLE_PER_KEY);
        idle.truncate(remove_count);
        for key in idle {
            self.resources.remove(&key);
        }
    }

    fn prune_idle_total(&mut self) {
        const MAX_IDLE_TOTAL: usize = 8;
        let mut idle = self
            .resources
            .iter()
            .filter_map(|(&key, resource)| {
                (!resource.checked_out).then_some((key, resource.last_used))
            })
            .collect::<Vec<_>>();
        let remove_count = idle.len().saturating_sub(MAX_IDLE_TOTAL);
        if remove_count == 0 {
            return;
        }
        idle.sort_by_key(|(_, last_used)| *last_used);
        idle.truncate(remove_count);
        for (key, _) in idle {
            self.resources.remove(&key);
        }
    }
}

#[derive(Clone)]
pub(crate) struct ExternalSurfaceEntry {
    pub content: ExternalSurfaceContent,
    pub provider: Option<ExternalSurfaceProviderHandle>,
    pub content_dirty: bool,
    pub version: u64,
    pub previous_outcome: Option<ExternalSurfaceOutcome>,
}

impl Default for ExternalSurfaceEntry {
    fn default() -> Self {
        Self {
            content: ExternalSurfaceContent::Empty,
            provider: None,
            content_dirty: false,
            version: 0,
            previous_outcome: None,
        }
    }
}
