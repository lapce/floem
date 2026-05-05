use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{
    compositor_surface::{
        CompositorSurfaceContent, CompositorSurfaceId, CompositorSurfaceOutcome,
        CompositorSurfaceProviderHandle,
    },
    frame::{FrameTime, target_frame_due},
    gpu_resources::GpuResources,
    paint::composition::{CompositionItem, CompositionPlan},
};
use peniko::kurbo::{Rect, Size};
use subduction_core::layer::SurfaceId;

use super::compositor::WindowCompositor;

#[derive(Default)]
pub(crate) struct WindowCompositorSurfaces {
    entries: FxHashMap<CompositorSurfaceId, CompositorSurfaceEntry>,
    intermediate_pool: IntermediateTexturePool,
    needs_frame_pull: bool,
    pending_outcomes: Vec<CompositorSurfaceOutcome>,
}

impl WindowCompositorSurfaces {
    pub(crate) fn from_entries(
        entries: FxHashMap<CompositorSurfaceId, CompositorSurfaceEntry>,
    ) -> Self {
        Self {
            entries,
            ..Self::default()
        }
    }

    pub(crate) fn entries(&self) -> &FxHashMap<CompositorSurfaceId, CompositorSurfaceEntry> {
        &self.entries
    }

    pub(crate) fn set_content(
        &mut self,
        surface_id: CompositorSurfaceId,
        content: CompositorSurfaceContent,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.content = content;
        entry.content_dirty = true;
        entry.version = entry.version.wrapping_add(1);
    }

    pub(crate) fn set_provider(
        &mut self,
        surface_id: CompositorSurfaceId,
        provider: CompositorSurfaceProviderHandle,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.provider = Some(provider);
        entry.content_dirty = true;
        entry.version = entry.version.wrapping_add(1);
    }

    pub(crate) fn set_target_fps(
        &mut self,
        surface_id: CompositorSurfaceId,
        target_fps: Option<f64>,
    ) {
        let entry = self.entries.entry(surface_id).or_default();
        entry.target_fps = sanitize_target_fps(target_fps);
        entry.last_delivered_frame_index = None;
    }

    pub(crate) fn reset_pacing_state(&mut self) {
        for entry in self.entries.values_mut() {
            entry.last_delivered_frame_index = None;
        }
        self.needs_frame_pull = true;
    }

    pub(crate) fn request_frame(&mut self, surface_id: CompositorSurfaceId) {
        self.entries.entry(surface_id).or_default();
        self.needs_frame_pull = true;
    }

    pub(crate) fn update_providers(
        &mut self,
        plan: &CompositionPlan,
        compositor: Option<&mut WindowCompositor>,
        effective_scale: f64,
        gpu_resources: Option<&GpuResources>,
        frame_time_for_surface: &mut impl FnMut(CompositorSurfaceId) -> FrameTime,
    ) -> WindowCompositorSurfaceFrameUpdate {
        let mut compositor = compositor;
        let mut frame_update = WindowCompositorSurfaceFrameUpdate::default();
        self.pending_outcomes.clear();
        for planned_surface in planned_compositor_surfaces(plan) {
            let Some(entry) = self.entries.get_mut(&planned_surface.surface_id) else {
                continue;
            };
            let Some(provider) = entry.provider.clone() else {
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
                    .unwrap_or(CompositorSurfaceContent::Empty);
                entry.presents_without_transaction =
                    provider.current_presents_without_transaction();
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            if let Some(latency) = poll_update.producer_observed_latency {
                entry.record_producer_latency(latency);
                frame_update.record_producer_latency(planned_surface.surface_id, latency);
            }
            let frame_time = frame_time_for_surface(planned_surface.surface_id);
            if !entry.should_deliver_frame_opportunity(frame_time) {
                frame_update.request_next_frame = true;
                continue;
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
            let args = crate::compositor_surface::CompositorSurfaceFrameArgs {
                surface_id: planned_surface.surface_id,
                frame_index: frame_time.frame_index,
                frame_time,
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
                    .unwrap_or(CompositorSurfaceContent::Empty);
                entry.presents_without_transaction =
                    provider.current_presents_without_transaction();
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            if let Some(latency) = update.producer_observed_latency {
                entry.record_producer_latency(latency);
                frame_update.record_producer_latency(planned_surface.surface_id, latency);
            }
            let post_update = provider.poll_current_content();
            frame_update.request_next_frame |= post_update.request_next_frame;
            if post_update.content_changed {
                frame_update.mark_content_changed(planned_surface.surface_id);
                entry.content = provider
                    .current_content()
                    .unwrap_or(CompositorSurfaceContent::Empty);
                entry.presents_without_transaction =
                    provider.current_presents_without_transaction();
                entry.content_dirty = true;
                entry.version = entry.version.wrapping_add(1);
            }
            if let Some(latency) = post_update.producer_observed_latency {
                entry.record_producer_latency(latency);
                frame_update.record_producer_latency(planned_surface.surface_id, latency);
            }
            self.pending_outcomes.push(CompositorSurfaceOutcome {
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
        mut frame_time_for_surface: impl FnMut(CompositorSurfaceId) -> FrameTime,
        plan: &CompositionPlan,
        compositor: &mut WindowCompositor,
        effective_scale: f64,
        gpu_resources: Option<&GpuResources>,
    ) -> WindowCompositorSurfaceFrameUpdate {
        self.take_frame_pull();
        let _composition_diff = compositor.apply_plan(plan, &self.entries, gpu_resources);
        let update = self.update_providers(
            plan,
            Some(compositor),
            effective_scale,
            gpu_resources,
            &mut frame_time_for_surface,
        );
        if update.content_changed {
            let _composition_diff = compositor.apply_plan(plan, &self.entries, gpu_resources);
        }
        update
    }

    pub(crate) fn release_outcomes(
        &mut self,
        mut update: impl FnMut(&mut CompositorSurfaceOutcome),
    ) {
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

    pub(crate) fn max_producer_work_estimate(&self) -> Duration {
        self.entries
            .values()
            .map(|entry| entry.producer_work_estimate)
            .max()
            .unwrap_or_default()
    }
}

#[derive(Debug, Default)]
pub(crate) struct WindowCompositorSurfaceFrameUpdate {
    pub content_changed: bool,
    pub request_next_frame: bool,
    pub changed_surfaces: Vec<CompositorSurfaceId>,
    pub producer_latency: Vec<(CompositorSurfaceId, Duration)>,
}

impl WindowCompositorSurfaceFrameUpdate {
    fn mark_content_changed(&mut self, surface_id: CompositorSurfaceId) {
        self.content_changed = true;
        if !self.changed_surfaces.contains(&surface_id) {
            self.changed_surfaces.push(surface_id);
        }
    }

    fn record_producer_latency(&mut self, surface_id: CompositorSurfaceId, latency: Duration) {
        self.producer_latency.push((surface_id, latency));
    }
}

#[derive(Clone)]
struct PlannedCompositorSurface {
    surface_id: CompositorSurfaceId,
    rect: Rect,
    source_size: Size,
    key: Option<crate::paint::composition::CompositionKey>,
}

fn planned_compositor_surfaces(plan: &CompositionPlan) -> Vec<PlannedCompositorSurface> {
    let mut surfaces = Vec::new();
    let mut requested = FxHashMap::default();
    for item in &plan.items {
        match item {
            CompositionItem::CompositorSurface(layer) => {
                let request_key = (layer.surface_id, size_key(layer.source_size));
                if let Some(index) = requested.get(&request_key).copied() {
                    let planned: &mut PlannedCompositorSurface = &mut surfaces[index];
                    if planned.key.is_none() {
                        planned.key = Some(layer.key.clone());
                        planned.rect = layer.rect;
                    }
                } else {
                    requested.insert(request_key, surfaces.len());
                    surfaces.push(PlannedCompositorSurface {
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
                    surfaces.push(PlannedCompositorSurface {
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
        compositor_surface::CompositorSurfaceId,
        paint::composition::{
            CompositionKey, CompositorSurfaceLayer, SceneExternalImage, SceneLayer,
        },
    };

    #[test]
    fn planned_compositor_surfaces_dedupes_repeated_direct_placements() {
        let surface_id = CompositorSurfaceId::test_new(42);
        let mut plan = CompositionPlan::new();
        plan.items
            .push(CompositionItem::CompositorSurface(CompositorSurfaceLayer {
                key: CompositionKey::CompositorSurface {
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
            .push(CompositionItem::CompositorSurface(CompositorSurfaceLayer {
                key: CompositionKey::CompositorSurface {
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

        let planned = planned_compositor_surfaces(&plan);
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].surface_id, surface_id);
        assert_eq!(planned[0].source_size, Size::new(100.0, 50.0));
        assert_eq!(
            planned[0].key,
            Some(CompositionKey::CompositorSurface {
                surface_id,
                occurrence: 0,
            })
        );
    }

    #[test]
    fn planned_compositor_surfaces_upgrades_scene_request_to_direct_layer_request() {
        let surface_id = CompositorSurfaceId::test_new(43);
        let mut plan = CompositionPlan::new();
        plan.items.push(CompositionItem::Scene(SceneLayer {
            key: CompositionKey::SceneRun { run_index: 0 },
            source_element_id: None,
            debug_name: None,
            scene: imaging::record::Scene::new(),
            external_images: vec![SceneExternalImage {
                image_id: imaging::ExternalImageId(43),
                surface_id,
                rect: Rect::new(0.0, 0.0, 100.0, 50.0),
                source_size: Size::new(100.0, 50.0),
            }],
            color_filters: Vec::new(),
            content_revision: 0,
            transform: peniko::kurbo::Affine::IDENTITY,
            clip: None,
            bounds: Rect::new(0.0, 0.0, 100.0, 50.0),
            content_bounds: None,
            opacity: 1.0,
            promoted: false,
            target_fps: None,
        }));
        plan.items
            .push(CompositionItem::CompositorSurface(CompositorSurfaceLayer {
                key: CompositionKey::CompositorSurface {
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

        let planned = planned_compositor_surfaces(&plan);
        assert_eq!(planned.len(), 1);
        assert_eq!(
            planned[0].key,
            Some(CompositionKey::CompositorSurface {
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
pub(crate) struct CompositorSurfaceEntry {
    pub content: CompositorSurfaceContent,
    pub provider: Option<CompositorSurfaceProviderHandle>,
    pub presents_without_transaction: bool,
    pub content_dirty: bool,
    pub version: u64,
    pub previous_outcome: Option<CompositorSurfaceOutcome>,
    pub target_fps: Option<f64>,
    last_delivered_frame_index: Option<u64>,
    producer_work_estimate: Duration,
}

impl Default for CompositorSurfaceEntry {
    fn default() -> Self {
        Self {
            content: CompositorSurfaceContent::Empty,
            provider: None,
            presents_without_transaction: false,
            content_dirty: false,
            version: 0,
            previous_outcome: None,
            target_fps: None,
            last_delivered_frame_index: None,
            producer_work_estimate: Duration::ZERO,
        }
    }
}

impl CompositorSurfaceEntry {
    fn record_producer_latency(&mut self, observed: Duration) {
        self.producer_work_estimate =
            smooth_duration_estimate(self.producer_work_estimate, observed);
    }

    fn should_deliver_frame_opportunity(&mut self, frame_time: FrameTime) -> bool {
        let Some(target_fps) = self.target_fps else {
            self.last_delivered_frame_index = Some(frame_time.frame_index);
            return true;
        };
        let should_deliver = target_frame_due(frame_time, Some(target_fps))
            || self
                .last_delivered_frame_index
                .is_some_and(|last| frame_time.frame_index <= last);
        if should_deliver {
            self.last_delivered_frame_index = Some(frame_time.frame_index);
        }
        should_deliver
    }
}

fn sanitize_target_fps(target_fps: Option<f64>) -> Option<f64> {
    target_fps.filter(|fps| fps.is_finite() && *fps > 0.0)
}

fn smooth_duration_estimate(previous: Duration, observed: Duration) -> Duration {
    if observed >= previous {
        return observed;
    }
    let previous_ns = previous.as_nanos();
    let observed_ns = observed.as_nanos();
    Duration::from_nanos(((previous_ns * 7 + observed_ns) / 8).min(u64::MAX as u128) as u64)
}
